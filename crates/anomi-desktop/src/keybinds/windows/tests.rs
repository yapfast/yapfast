use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
};

use super::super::{Action, Key, Shortcut, Toggled, Watched};
use super::*;

/// The hook tests share the microphone's held state.
static TALK_STATE: Mutex<()> = Mutex::new(());

fn watched(action: Action, codes: &[u32]) -> Watched {
    Watched {
        id: String::new(),
        action,
        keys: Shortcut::new(
            codes
                .iter()
                .map(|&scancode| Key {
                    scancode,
                    name: format!("{scancode:#x}"),
                    trigger: None,
                })
                .collect(),
        )
        .unwrap(),
    }
}

fn sink() -> (Sink, Arc<Mutex<Vec<Action>>>) {
    let toggles = Arc::new(Mutex::new(Vec::new()));
    let heard = toggles.clone();
    let toggled: Toggled = Arc::new(move |action| heard.lock().unwrap().push(action));
    (Sink { toggled }, toggles)
}

/// Types a key by its scan code, as a keyboard would.
fn send(scancode: u16, up: bool) {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: scancode,
                dwFlags: KEYEVENTF_SCANCODE | if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    // SAFETY: one input, of the size Windows expects.
    let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
    assert_eq!(sent, 1);
}

/// Clicks a side button, as a mouse would.
fn send_mouse(button: u16, up: bool) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: u32::from(button),
                dwFlags: if up {
                    MOUSEEVENTF_XUP
                } else {
                    MOUSEEVENTF_XDOWN
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    // SAFETY: one input, of the size Windows expects.
    let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
    assert_eq!(sent, 1);
}

fn wait_held(held: bool) {
    let until = Instant::now() + Duration::from_secs(2);
    while yapfast_core::push_to_talk::held() != held {
        assert!(Instant::now() < until, "the key wasn't held: {held}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Hands the keyboard hook a key, as Windows would.
fn key(message: u32, scan: u32, virtual_key: u32, flags: u32) {
    let details = KBDLLHOOKSTRUCT {
        vkCode: virtual_key,
        scanCode: scan,
        flags,
        ..Default::default()
    };
    // SAFETY: details has the shape a keyboard hook's lparam points at.
    unsafe {
        proc(
            HC_ACTION as i32,
            message as usize,
            &details as *const KBDLLHOOKSTRUCT as LPARAM,
        );
    }
}

/// A key with a scan code of 0, as keyboard software sends, is the
/// key its virtual key names; issues #78 and #182.
#[test]
fn a_key_without_a_scan_code_is_known_by_its_virtual_key() {
    let _talk = TALK_STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    hooked_here(
        Matcher::new(&[watched(Action::PushToTalk, &[0x2F])]),
        sink().0,
    ); // V
    yapfast_core::push_to_talk::hold(false);
    key(WM_KEYDOWN, 0, 0x56, LLKHF_INJECTED); // VK_V
    assert!(yapfast_core::push_to_talk::held());
    key(WM_KEYUP, 0, 0x56, LLKHF_INJECTED);
    assert!(!yapfast_core::push_to_talk::held());
    // The made-up Left Ctrl of AltGr holds nothing.
    hooked_here(
        Matcher::new(&[watched(Action::PushToTalk, &[0x1D])]),
        sink().0,
    );
    key(WM_KEYDOWN, 0x21D, 0xA2, 0);
    assert!(!yapfast_core::push_to_talk::held());
    HOOKED.take();
}

#[test]
fn an_extended_key_is_its_own() {
    let _talk = TALK_STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    hooked_here(
        Matcher::new(&[watched(Action::PushToTalk, &[0xE01D])]),
        sink().0,
    ); // Right Ctrl
    yapfast_core::push_to_talk::hold(false);
    key(WM_KEYDOWN, 0x1D, 0xA2, 0);
    assert!(!yapfast_core::push_to_talk::held(), "Left Ctrl");
    key(WM_KEYUP, 0x1D, 0xA2, 0);
    key(WM_KEYDOWN, 0x1D, 0xA3, LLKHF_EXTENDED);
    assert!(yapfast_core::push_to_talk::held());
    key(WM_KEYUP, 0x1D, 0xA3, LLKHF_EXTENDED);
    assert!(!yapfast_core::push_to_talk::held());
    HOOKED.take();
}

#[test]
fn the_mouse_hook_hears_the_bound_side_button_go_down_and_up() {
    let _talk = TALK_STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    fn event(message: u32, button: u16) {
        let details = MSLLHOOKSTRUCT {
            mouseData: u32::from(button) << 16,
            ..Default::default()
        };
        // SAFETY: details has the shape a mouse hook's lparam points at.
        unsafe {
            mouse_proc(
                HC_ACTION as i32,
                message as usize,
                &details as *const MSLLHOOKSTRUCT as LPARAM,
            );
        }
    }

    yapfast_core::push_to_talk::hold(false);
    hooked_here(
        Matcher::new(&[watched(Action::PushToTalk, &[MOUSE_4])]),
        sink().0,
    );
    event(WM_XBUTTONDOWN, XBUTTON2);
    assert!(!yapfast_core::push_to_talk::held());
    event(WM_XBUTTONDOWN, XBUTTON1);
    assert!(yapfast_core::push_to_talk::held());
    event(WM_XBUTTONUP, XBUTTON2);
    assert!(yapfast_core::push_to_talk::held());
    event(WM_XBUTTONUP, XBUTTON1);
    assert!(!yapfast_core::push_to_talk::held());

    hooked_here(
        Matcher::new(&[watched(Action::PushToTalk, &[MOUSE_5])]),
        sink().0,
    );
    event(WM_XBUTTONDOWN, XBUTTON2);
    assert!(yapfast_core::push_to_talk::held());
    event(WM_XBUTTONUP, XBUTTON2);
    assert!(!yapfast_core::push_to_talk::held());
    HOOKED.take();
}

/// The installed mouse hook hears Mouse 4 anywhere, down and up. Run it
/// alone, on a desktop, or under Wine with an X server.
#[test]
#[ignore = "clicks the desktop's side buttons"]
fn the_installed_mouse_hook_hears_mouse_4_go_down_and_up() {
    let _talk = TALK_STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let hook = Hook::start(
        Matcher::new(&[watched(Action::PushToTalk, &[MOUSE_4])]),
        sink().0,
        Arc::new(|_| {}),
    )
    .expect("no hook");
    send_mouse(XBUTTON1, false);
    wait_held(true);
    send_mouse(XBUTTON1, true);
    wait_held(false);
    send_mouse(XBUTTON2, false);
    send_mouse(XBUTTON2, true);
    std::thread::sleep(Duration::from_millis(100));
    assert!(!yapfast_core::push_to_talk::held());
    drop(hook);
}

/// The hook hears the keys typed anywhere, down and up, and a
/// combination toggles once. Run it alone, on a desktop, or under
/// Wine with an X server.
#[test]
#[ignore = "types into the desktop"]
fn the_hook_hears_the_keys_go_down_and_up() {
    let _talk = TALK_STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (sink, toggles) = sink();
    let hook = Hook::start(
        Matcher::new(&[
            watched(Action::PushToTalk, &[0x2F]),       // V
            watched(Action::ToggleMute, &[0x1D, 0x32]), // Ctrl+M
        ]),
        sink,
        Arc::new(|_| {}),
    )
    .expect("no hook");
    send(0x2F, false);
    wait_held(true);
    send(0x2F, true);
    wait_held(false);
    send(0x30, false); // B
    send(0x30, true);
    std::thread::sleep(Duration::from_millis(100));
    assert!(!yapfast_core::push_to_talk::held());
    send(0x1D, false);
    send(0x32, false);
    send(0x32, false); // a repeat
    send(0x32, true);
    send(0x1D, true);
    let until = Instant::now() + Duration::from_secs(2);
    while toggles.lock().unwrap().is_empty() {
        assert!(Instant::now() < until, "Ctrl+M didn't toggle");
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(*toggles.lock().unwrap(), [Action::ToggleMute]);
    drop(hook);
}
