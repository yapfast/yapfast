//! A low-level keyboard hook, and a mouse hook when Mouse 4 or 5 is bound,
//! on a thread of their own: they see releases and pass every event on.

use std::{cell::RefCell, collections::HashMap, ptr, sync::mpsc};

use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetKeyboardLayout, MAPVK_VK_TO_VSC_EX, MAPVK_VSC_TO_VK_EX,
            MapVirtualKeyExW, MapVirtualKeyW, VK_XBUTTON1, VK_XBUTTON2,
        },
        WindowsAndMessaging::{
            CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, HC_ACTION,
            KBDLLHOOKSTRUCT, KillTimer, LLKHF_EXTENDED, LLKHF_INJECTED, MSG, MSLLHOOKSTRUCT,
            PostThreadMessageW, SetTimer, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL,
            WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER,
            WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2,
        },
    },
};
use winit::{keyboard::PhysicalKey, platform::scancode::PhysicalKeyExtScancode};

use super::{MOUSE_4, MOUSE_5, Matcher, Reach, Report, Sink, Status};

#[cfg(test)]
mod tests;

/// How often a held keybind checks that its keys are still down, in ms.
const CHECK_EVERY: u32 = 250;
/// Key events logged at debug level, to tell why a key isn't heard.
const LOGGED: u32 = 32;

/// The hook thread's keybinds; the hook procedures run on it.
struct Hooked {
    matcher: Matcher,
    sink: Sink,
    /// The timer checking held keys, while some keybind holds.
    timer: usize,
    /// The virtual key each bound key last went down with, which
    /// Windows keeps the state of.
    seen: Vec<(u32, u32)>,
    /// The virtual keys of the bound keys, to log a key that has one of
    /// them but another scan code.
    virtual_keys: Vec<u32>,
    logged: u32,
}

thread_local! {
    static HOOKED: RefCell<Option<Hooked>> = const { RefCell::new(None) };
}

/// Low-level keyboard and mouse hooks on a thread of their own, which see
/// every input event, releases included, and pass each on. Removed when
/// dropped.
pub struct Hook {
    thread: u32,
}

impl Hook {
    pub(super) fn start(matcher: Matcher, sink: Sink, report: Report) -> Option<Self> {
        let mouse = |code: &u32| matches!(*code, MOUSE_4 | MOUSE_5);
        let (keys, buttons) = (
            matcher.codes.iter().any(|code| !mouse(code)),
            matcher.codes.iter().any(mouse),
        );
        let virtual_keys = matcher
            .codes
            .iter()
            .filter(|code| !mouse(code))
            // SAFETY: MapVirtualKeyW only reads its arguments.
            .map(|&code| unsafe { MapVirtualKeyW(code, MAPVK_VSC_TO_VK_EX) })
            .filter(|&virtual_key| virtual_key != 0)
            .collect();
        let (started, thread) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("yapfast-keybinds".into())
            .spawn(move || {
                HOOKED.set(Some(Hooked {
                    matcher,
                    sink,
                    timer: 0,
                    seen: Vec::new(),
                    virtual_keys,
                    logged: 0,
                }));
                // SAFETY: the hook procedures below only read the event
                // Windows hands them; the message loop runs on this
                // thread, which the hooks need, until WM_QUIT.
                unsafe {
                    let module = GetModuleHandleW(ptr::null());
                    let keyboard = if keys {
                        SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), module, 0)
                    } else {
                        ptr::null_mut()
                    };
                    let mouse = if buttons {
                        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0)
                    } else {
                        ptr::null_mut()
                    };
                    if (keys && keyboard.is_null()) || (buttons && mouse.is_null()) {
                        for hook in [keyboard, mouse] {
                            if !hook.is_null() {
                                UnhookWindowsHookEx(hook);
                            }
                        }
                        let _ = started.send(None);
                        return;
                    }
                    let _ = started.send(Some(GetCurrentThreadId()));
                    let mut message: MSG = std::mem::zeroed();
                    while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
                        if message.message == WM_TIMER {
                            check_held();
                        }
                    }
                    for hook in [keyboard, mouse] {
                        if !hook.is_null() {
                            UnhookWindowsHookEx(hook);
                        }
                    }
                }
                if let Some(mut hooked) = HOOKED.take() {
                    let sink = hooked.sink.clone();
                    hooked
                        .matcher
                        .release_all(&mut |action, down| sink.act(action, down));
                }
            });
        let thread = spawned.ok().and_then(|_| thread.recv().ok().flatten());
        match thread {
            Some(thread) => {
                tracing::info!(target: crate::LOG, keys, buttons, "keybind hooks installed");
                report(Status {
                    reach: Reach::Everywhere,
                    given: HashMap::new(),
                });
                Some(Self { thread })
            }
            None => {
                tracing::warn!(target: crate::LOG, "Windows didn't install the keybind hooks");
                report(Status {
                    reach: Reach::Focused("Windows didn't let Yapfast watch keys in other apps"),
                    given: HashMap::new(),
                });
                None
            }
        }
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        // SAFETY: posting to a thread id is harmless if it has ended.
        unsafe {
            PostThreadMessageW(self.thread, WM_QUIT, 0, 0);
        }
    }
}

/// The key as winit names it, which is how settings recorded it: a scan
/// code of 0, as keyboard software and remappers send, comes from the
/// virtual key in the focused window's layout, and the code goes
/// through winit's own mapping, as the window's key events do. None for
/// a key Windows makes up, such as the Left Ctrl that AltGr sends.
fn code(key: &KBDLLHOOKSTRUCT) -> Option<u32> {
    if key.scanCode > 0xFF {
        return None;
    }
    let scancode = if key.scanCode == 0 {
        // SAFETY: these only read the foreground window's thread's
        // layout and map a key with it.
        unsafe {
            let thread = GetWindowThreadProcessId(GetForegroundWindow(), ptr::null_mut());
            MapVirtualKeyExW(key.vkCode, MAPVK_VK_TO_VSC_EX, GetKeyboardLayout(thread))
        }
    } else if key.flags & LLKHF_EXTENDED != 0 {
        key.scanCode | 0xE000
    } else {
        key.scanCode
    };
    (scancode != 0).then(|| {
        PhysicalKey::from_scancode(scancode)
            .to_scancode()
            .unwrap_or(scancode)
    })
}

/// Whether Windows has `code` down now, by the virtual key it last went
/// down with; true for a key that never did, which can't be told.
fn key_down(seen: &[(u32, u32)], code: u32) -> bool {
    let virtual_key = match code {
        MOUSE_4 => u32::from(VK_XBUTTON1),
        MOUSE_5 => u32::from(VK_XBUTTON2),
        _ => match seen.iter().find(|&&(seen, _)| seen == code) {
            Some(&(_, virtual_key)) => virtual_key,
            None => return true,
        },
    };
    // SAFETY: GetAsyncKeyState only reads the key's state.
    unsafe { GetAsyncKeyState(virtual_key as i32) as u16 & 0x8000 != 0 }
}

/// A bound key went down or came up, on the hook thread, with the
/// virtual key Windows sent for it.
fn event(code: u32, down: bool, virtual_key: Option<u32>) {
    HOOKED.with_borrow_mut(|hooked| {
        let Some(Hooked {
            matcher,
            sink,
            timer,
            seen,
            ..
        }) = hooked.as_mut()
        else {
            return;
        };
        let out = &mut |action, down| sink.act(action, down);
        if down {
            // The hook runs before Windows notes the key down, so a key
            // held here that Windows has up lost its key-up, as when the
            // secure desktop took the keys; a repeat's reads down.
            if matcher.is_held(code) && !key_down(seen, code) {
                matcher.key(code, false, out);
            }
            if let Some(virtual_key) = virtual_key {
                match seen.iter_mut().find(|(seen, _)| *seen == code) {
                    Some(entry) => entry.1 = virtual_key,
                    None => seen.push((code, virtual_key)),
                }
            }
            // The other keys of a combination, likewise.
            matcher.resync(Some(code), |code| key_down(seen, code), out);
        }
        matcher.key(code, down, out);
        timing(matcher, timer);
    });
}

/// Checks the timer runs while some keybind holds, and only then.
fn timing(matcher: &Matcher, timer: &mut usize) {
    // SAFETY: a thread timer, without a window or callback; WM_TIMER
    // comes to this thread's message loop.
    unsafe {
        if matcher.holding() && *timer == 0 {
            *timer = SetTimer(ptr::null_mut(), 0, CHECK_EVERY, None);
        } else if !matcher.holding() && *timer != 0 {
            KillTimer(ptr::null_mut(), *timer);
            *timer = 0;
        }
    }
}

/// Lets a held keybind go once Windows says its key is up, as when the
/// key-up went to the secure desktop or a program run as administrator.
fn check_held() {
    HOOKED.with_borrow_mut(|hooked| {
        let Some(Hooked {
            matcher,
            sink,
            timer,
            seen,
            ..
        }) = hooked.as_mut()
        else {
            return;
        };
        matcher.resync(None, |code| key_down(seen, code), &mut |action, down| {
            sink.act(action, down)
        });
        timing(matcher, timer);
    });
}

/// Logs a few key events at debug level that might be a bound key: a
/// scan code of 0, or a bound key's virtual key with another scan code.
/// Keys bound to nothing else go unlogged.
fn log(key: &KBDLLHOOKSTRUCT, code: Option<u32>) {
    HOOKED.with_borrow_mut(|hooked| {
        let Some(hooked) = hooked.as_mut() else {
            return;
        };
        let suspect = key.scanCode == 0 || hooked.virtual_keys.contains(&key.vkCode);
        if hooked.logged >= LOGGED
            || !suspect
            || !tracing::enabled!(target: crate::LOG, tracing::Level::DEBUG)
        {
            return;
        }
        hooked.logged += 1;
        tracing::debug!(target: crate::LOG,
            scan = key.scanCode,
            virtual_key = key.vkCode,
            flags = key.flags,
            injected = key.flags & LLKHF_INJECTED != 0,
            ?code,
            "keybind hook heard a key"
        );
    });
}

/// Notes bound keys going down or up, and passes every key on.
pub(super) unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points at the key's details.
        let key = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let down = match wparam as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
            WM_KEYUP | WM_SYSKEYUP => Some(false),
            _ => None,
        };
        if let Some(down) = down {
            let scancode = self::code(key);
            let bound = scancode.is_some_and(|scancode| {
                HOOKED.with_borrow(|hooked| {
                    hooked
                        .as_ref()
                        .is_some_and(|hooked| hooked.matcher.bound(scancode))
                })
            });
            if down && !bound {
                log(key, scancode);
            }
            if let (Some(scancode), true) = (scancode, bound) {
                event(scancode, down, Some(key.vkCode));
            }
        }
    }
    // SAFETY: the next hook gets the event as it came.
    unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
}

/// Notes bound side buttons going down or up, and passes every mouse
/// event on. XBUTTON1 is Mouse 4 (Back); XBUTTON2 is Mouse 5 (Forward).
pub(super) unsafe extern "system" fn mouse_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points at the mouse event's details.
        let mouse = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        let button = match (mouse.mouseData >> 16) as u16 {
            XBUTTON1 => Some(MOUSE_4),
            XBUTTON2 => Some(MOUSE_5),
            _ => None,
        };
        let down = match wparam as u32 {
            WM_XBUTTONDOWN => Some(true),
            WM_XBUTTONUP => Some(false),
            _ => None,
        };
        if let (Some(button), Some(down)) = (button, down) {
            event(button, down, None);
        }
    }
    // SAFETY: the next hook gets the event as it came.
    unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
}

/// Only for the tests, which run the hook procedures on their own
/// thread.
#[cfg(test)]
fn hooked_here(matcher: Matcher, sink: Sink) {
    HOOKED.set(Some(Hooked {
        matcher,
        sink,
        timer: 0,
        seen: Vec::new(),
        virtual_keys: Vec::new(),
        logged: 0,
    }));
}
