//! Toggle keybinds for calls: mute microphone, mute sound (deafen), and mute
//! everything, working both while Yapfast's window is focused and in any other
//! app (through the GlobalShortcuts portal on Wayland, XInput 2's raw key
//! events on X11, and low-level keyboard/mouse hooks on Windows).
//!
//! Kept in its own module so [`crate::push_to_talk`] stays untouched when
//! upstream updates push-to-talk.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use serde::{Deserialize, Serialize};
#[cfg(windows)]
use slint::winit_030::winit::event::MouseButton;
use slint::winit_030::winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    platform::scancode::PhysicalKeyExtScancode,
};
use yapfast_core::runtime::BackgroundRuntime;

pub use crate::push_to_talk::{Keybind, Reach};

#[cfg(windows)]
const MOUSE_4: u32 = 0xFFFF_0004;
#[cfg(windows)]
const MOUSE_5: u32 = 0xFFFF_0005;

/// What a toggle keybind does when pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Turns the microphone off, or back on.
    ToggleMute,
    /// Deafens, or undeafens.
    ToggleDeafen,
    /// Deafens and mutes together, or brings both back.
    ToggleMuteAll,
}

impl Action {
    /// The keybinds in the order Settings lists them.
    pub const ALL: [Self; 3] = [Self::ToggleMute, Self::ToggleDeafen, Self::ToggleMuteAll];

    const fn index(self) -> usize {
        match self {
            Self::ToggleMute => 0,
            Self::ToggleDeafen => 1,
            Self::ToggleMuteAll => 2,
        }
    }

    /// The portal's shortcut id, which must stay a valid D-Bus member name.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    const fn id(self) -> &'static str {
        match self {
            Self::ToggleMute => "toggle-mute",
            Self::ToggleDeafen => "toggle-deafen",
            Self::ToggleMuteAll => "toggle-mute-all",
        }
    }

    /// How the desktop's global shortcuts settings name the action.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    const fn description(self) -> &'static str {
        match self {
            Self::ToggleMute => "Mute microphone",
            Self::ToggleDeafen => "Mute sound",
            Self::ToggleMuteAll => "Mute everything",
        }
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.id() == id)
    }
}

/// What the Keybinds page in Settings says under the keys.
pub fn note(reach: &Reach) -> String {
    match reach {
        Reach::Asking => "Asking your desktop to let Yapfast use these keys in every app...".into(),
        Reach::Everywhere(None) => "Press a key in any app to mute or deafen.".into(),
        Reach::Everywhere(Some(shortcut)) if shortcut.is_empty() => {
            "Your desktop hasn't allowed these shortcuts yet. Approve them in your system's global shortcuts settings.".into()
        }
        Reach::Everywhere(Some(shortcuts)) => format!(
            "Your desktop gave Yapfast {shortcuts}. Change them in your system's global shortcuts settings."
        ),
        Reach::Focused(why) => format!(
            "{why}, so these keybinds work only while Yapfast's window is in front."
        ),
    }
}

const NONE: u32 = u32::MAX;

/// Each action's scan code while [`Watcher`] watches it; [`NONE`] otherwise.
static KEYS: [AtomicU32; 3] = [
    AtomicU32::new(NONE),
    AtomicU32::new(NONE),
    AtomicU32::new(NONE),
];
/// Whether each action's key is held right now, so a held key acts once
/// rather than on each key-repeat event.
static DOWN: [AtomicBool; 3] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];
/// Whether the window's own key events are the only source of key releases.
static FOCUSED_ONLY: AtomicBool = AtomicBool::new(false);
/// Whether settings are waiting for the next key to record it.
static RECORDING: AtomicBool = AtomicBool::new(false);
/// The scan code of the key just pressed to record a keybind, until it comes up.
static RECORDED: AtomicU32 = AtomicU32::new(NONE);
/// Whether the window's latest key press was one of the toggle keybinds.
static PRESSED_KEY: AtomicBool = AtomicBool::new(false);

type Handler = Arc<dyn Fn(Action) + Send + Sync>;
/// What hears each press of a toggle keybind.
static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);

/// Tells the watcher whether settings are recording a key, so pressing a key
/// to record it doesn't also act on its current keybind.
pub fn set_recording(recording: bool) {
    RECORDING.store(recording, Ordering::Relaxed);
}

fn release_recorded(scancode: u32) {
    let _ = RECORDED.compare_exchange(scancode, NONE, Ordering::Relaxed, Ordering::Relaxed);
}

/// Records a keyboard key press for a keybind and suppresses its current
/// action until the key is released.
pub fn recorded(event: &KeyEvent) -> Option<Keybind> {
    let key = Keybind::recorded(event)?;
    RECORDED.store(key.scancode, Ordering::Relaxed);
    Some(key)
}

/// Records Mouse 4 or Mouse 5 on Windows for a keybind and suppresses its
/// current action until the button is released.
#[cfg(windows)]
pub fn recorded_mouse(event: &WindowEvent) -> Option<Keybind> {
    let key = Keybind::recorded_mouse(event)?;
    RECORDED.store(key.scancode, Ordering::Relaxed);
    Some(key)
}

fn bind(action: Action, scancode: Option<u32>) {
    KEYS[action.index()].store(scancode.unwrap_or(NONE), Ordering::Relaxed);
}

fn action_for(scancode: u32) -> Option<Action> {
    Action::ALL
        .into_iter()
        .find(|action| KEYS[action.index()].load(Ordering::Relaxed) == scancode)
}

/// A watched key went down or came up. Toggle keybinds act once as the key
/// goes down, and not again until it comes up.
fn press(action: Action, down: bool) {
    if !down {
        RECORDED.store(NONE, Ordering::Relaxed);
    }
    let recorded = RECORDED.load(Ordering::Relaxed);
    let suppressed = down
        && (RECORDING.load(Ordering::Relaxed)
            || (recorded != NONE && KEYS[action.index()].load(Ordering::Relaxed) == recorded));
    if DOWN[action.index()].swap(down, Ordering::Relaxed) != down && down && !suppressed {
        let handler = HANDLER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(handler) = handler {
            handler(action);
        }
    }
}

fn release_all() {
    RECORDED.store(NONE, Ordering::Relaxed);
    for action in Action::ALL {
        press(action, false);
    }
}

/// Whether the key press Slint is handling is a toggle keybind, which
/// shouldn't also type into the composer.
pub fn pressed_key() -> bool {
    PRESSED_KEY.load(Ordering::Relaxed)
}

/// Handles window key/mouse events for toggle keybinds while the window is
/// focused, and returns `true` when the event matched a bound toggle keybind
/// so the window doesn't pass it on to other controls.
pub fn window_event(event: &WindowEvent) -> bool {
    match event {
        WindowEvent::KeyboardInput { event, .. } => {
            let scancode = event.physical_key.to_scancode();
            if event.state == ElementState::Released
                && let Some(scancode) = scancode
            {
                release_recorded(scancode);
            }
            let action = scancode.and_then(action_for);
            if event.state == ElementState::Pressed {
                PRESSED_KEY.store(action.is_some(), Ordering::Relaxed);
            }
            if !event.repeat
                && let Some(action) = action
            {
                press(action, event.state == ElementState::Pressed);
            }
            event.state == ElementState::Pressed && action.is_some()
        }
        #[cfg(windows)]
        WindowEvent::MouseInput { state, button, .. } => {
            let scancode = match button {
                MouseButton::Back => Some(MOUSE_4),
                MouseButton::Forward => Some(MOUSE_5),
                _ => None,
            };
            if *state == ElementState::Released
                && let Some(scancode) = scancode
            {
                release_recorded(scancode);
            }
            let action = scancode.and_then(action_for);
            if let Some(action) = action {
                press(action, *state == ElementState::Pressed);
            }
            *state == ElementState::Pressed && action.is_some()
        }
        WindowEvent::Focused(false) => {
            if FOCUSED_ONLY.load(Ordering::Relaxed) {
                release_all();
            }
            false
        }
        _ => false,
    }
}

/// Watches the configured toggle keybinds in every app it can, until dropped.
pub struct Watcher {
    keys: Vec<(Action, Keybind)>,
    reach: Arc<Mutex<Reach>>,
    #[cfg(target_os = "linux")]
    _global: Option<linux::Global>,
    #[cfg(windows)]
    _global: Option<windows::Hook>,
}

impl Watcher {
    /// Starts watching `keys`; `told` hears where they work whenever that
    /// changes, and `acted` hears each toggle keybind press, from any thread.
    pub fn start(
        keys: Vec<(Action, Keybind)>,
        runtime: &BackgroundRuntime,
        told: impl Fn(Reach) + Send + Sync + 'static,
        acted: impl Fn(Action) + Send + Sync + 'static,
    ) -> Self {
        let reach = Arc::new(Mutex::new(Reach::Asking));
        let shared = reach.clone();
        *HANDLER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(acted));
        for action in Action::ALL {
            bind(action, None);
        }
        for &(action, ref key) in &keys {
            bind(action, Some(key.scancode));
        }
        let report = Arc::new(move |now: Reach| {
            let focused = matches!(now, Reach::Focused(_));
            FOCUSED_ONLY.store(focused, Ordering::Relaxed);
            *shared
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = now.clone();
            told(now);
        });
        #[cfg(target_os = "linux")]
        let global = linux::Global::start(&keys, runtime, report);
        #[cfg(windows)]
        let global = {
            let _ = runtime;
            windows::Hook::start(&keys, report)
        };
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = runtime;
            report(Reach::Focused(
                "Yapfast can't watch keys in other apps on this system yet",
            ));
        }
        Self {
            keys,
            reach,
            #[cfg(any(target_os = "linux", windows))]
            _global: global,
        }
    }

    pub fn keys(&self) -> &[(Action, Keybind)] {
        &self.keys
    }

    pub fn reach(&self) -> Reach {
        self.reach
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        FOCUSED_ONLY.store(false, Ordering::Relaxed);
        for action in Action::ALL {
            bind(action, None);
        }
        *HANDLER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        release_all();
    }
}

type Report = Arc<dyn Fn(Reach) + Send + Sync>;

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    use anyhow::{Context, Result, anyhow, bail};
    use futures_util::StreamExt;
    use x11rb::{
        connection::Connection as _,
        protocol::{
            Event,
            xinput::{self, ConnectionExt as _},
            xproto::{self, ConnectionExt as _},
        },
        rust_connection::RustConnection,
    };
    use yapfast_core::runtime::{BackgroundRuntime, BackgroundTask};
    use zbus::{
        Connection, MessageStream,
        zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
    };

    use super::{Action, Keybind, Reach, Report};

    /// Global keys on Wayland or X11; stops when dropped.
    pub enum Global {
        Portal { _session: BackgroundTask<()> },
        X11 { _events: X11 },
    }

    impl Global {
        pub fn start(
            keys: &[(Action, Keybind)],
            runtime: &BackgroundRuntime,
            report: Report,
        ) -> Option<Self> {
            let wayland =
                std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
            if wayland {
                report(Reach::Asking);
                let shortcuts: Vec<(Action, Option<String>)> = keys
                    .iter()
                    .map(|(action, key)| (*action, key.trigger.clone()))
                    .collect();
                return Some(Global::Portal {
                    _session: runtime.spawn(async move {
                        if let Err(error) = portal(shortcuts, &report).await {
                            tracing::info!(
                                error = format!("{error:#}"),
                                "no global keybinds through the portal"
                            );
                            report(Reach::Focused(
                                "Your desktop doesn't offer Yapfast global shortcuts",
                            ));
                        }
                    }),
                });
            }
            match X11::start(report.clone()) {
                Ok(x11) => {
                    report(Reach::Everywhere(None));
                    Some(Global::X11 { _events: x11 })
                }
                Err(error) => {
                    tracing::info!(
                        error = format!("{error:#}"),
                        "no global keybinds on X11"
                    );
                    report(Reach::Focused("Yapfast couldn't watch keys in other apps"));
                    None
                }
            }
        }
    }

    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";
    const SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";

    async fn portal(shortcuts: Vec<(Action, Option<String>)>, report: &Report) -> Result<()> {
        let connection = Connection::session().await?;
        let _ = connection
            .call_method(
                Some(PORTAL),
                PATH,
                Some("org.freedesktop.host.portal.Registry"),
                "Register",
                &("io.github.yapfast.Yapfast", HashMap::<&str, Value>::new()),
            )
            .await;

        let mut tokens = 0u32..;
        let mut next_token = || format!("yapfast_keybinds_{}", tokens.next().unwrap_or(0));

        let session_token = next_token();
        let session_options = HashMap::from([
            ("handle_token", Value::from(next_token())),
            ("session_handle_token", Value::from(session_token.as_str())),
        ]);
        let results = request(&connection, "CreateSession", &(session_options,)).await?;
        let session = match results.get("session_handle").map(|value| &**value) {
            Some(Value::Str(path)) => ObjectPath::try_from(path.as_str())?.into(),
            Some(Value::ObjectPath(path)) => path.clone().into(),
            _ => bail!("the portal gave no session handle"),
        };

        let mut signals = MessageStream::from(&connection);
        let shortcuts: Vec<(&str, HashMap<&str, Value>)> = shortcuts
            .iter()
            .map(|(action, trigger)| {
                let mut shortcut = HashMap::from([("description", Value::from(action.description()))]);
                if let Some(trigger) = trigger.as_deref() {
                    shortcut.insert("preferred_trigger", Value::from(trigger));
                }
                (action.id(), shortcut)
            })
            .collect();
        let bind_options = HashMap::from([("handle_token", Value::from(next_token()))]);
        let bound = request(
            &connection,
            "BindShortcuts",
            &(&session, shortcuts, "", bind_options),
        )
        .await?;
        let description = bound
            .get("shortcuts")
            .and_then(|value| <Vec<(String, HashMap<String, OwnedValue>)>>::try_from(value.clone()).ok())
            .map(|shortcuts| trigger_description(&shortcuts))
            .unwrap_or_default();
        report(Reach::Everywhere(Some(description)));

        while let Some(message) = signals.next().await {
            let Ok(message) = message else { continue };
            let header = message.header();
            if header.interface().map(|name| name.as_str()) != Some(SHORTCUTS) {
                continue;
            }
            let down = match header.member().map(|name| name.as_str()) {
                Some("Activated") => true,
                Some("Deactivated") => false,
                _ => continue,
            };
            let body = message.body();
            let Ok((for_session, id, _, _)) =
                body.deserialize::<(OwnedObjectPath, &str, u64, HashMap<&str, OwnedValue>)>()
            else {
                continue;
            };
            if for_session == session
                && let Some(action) = Action::from_id(id)
            {
                super::press(action, down);
            }
        }
        Ok(())
    }

    async fn request<B: serde::Serialize + zbus::DynamicType>(
        connection: &Connection,
        method: &str,
        body: &B,
    ) -> Result<HashMap<String, OwnedValue>> {
        let mut signals = MessageStream::from(connection);
        let reply = connection
            .call_method(Some(PORTAL), PATH, Some(SHORTCUTS), method, body)
            .await
            .with_context(|| format!("{method} failed"))?;
        let request: OwnedObjectPath = reply.body().deserialize()?;
        while let Some(message) = signals.next().await {
            let message = message?;
            let header = message.header();
            if header.path() == Some(&request.as_ref())
                && header.member().map(|name| name.as_str()) == Some("Response")
            {
                let (code, results): (u32, HashMap<String, OwnedValue>) =
                    message.body().deserialize()?;
                if code != 0 {
                    return Err(anyhow!("{method} was refused or cancelled ({code})"));
                }
                return Ok(results);
            }
        }
        bail!("the portal closed before answering {method}")
    }

    fn trigger_description(shortcuts: &[(String, HashMap<String, OwnedValue>)]) -> String {
        let triggers: Vec<String> = shortcuts
            .iter()
            .filter_map(|(id, details)| {
                let action = Action::from_id(id)?;
                match details.get("trigger_description").map(|value| &**value) {
                    Some(Value::Str(text)) if !text.is_empty() => {
                        Some(format!("{text} ({})", action.description()))
                    }
                    _ => None,
                }
            })
            .collect();
        triggers.join(", ")
    }

    pub struct X11 {
        connection: Arc<RustConnection>,
        window: xproto::Window,
        stop: Arc<AtomicBool>,
    }

    impl X11 {
        fn start(report: Report) -> Result<Self> {
            let (connection, screen) = x11rb::connect(None)?;
            let root = connection.setup().roots[screen].root;
            let version = connection.xinput_xi_query_version(2, 2)?.reply()?;
            if version.major_version < 2 {
                bail!("the X server has no XInput 2");
            }
            let window = connection.generate_id()?;
            connection.create_window(
                0,
                window,
                root,
                0,
                0,
                1,
                1,
                0,
                xproto::WindowClass::INPUT_ONLY,
                0,
                &xproto::CreateWindowAux::new(),
            )?;
            connection.xinput_xi_select_events(
                root,
                &[xinput::EventMask {
                    deviceid: xinput::Device::ALL_MASTER.into(),
                    mask: vec![
                        xinput::XIEventMask::RAW_KEY_PRESS | xinput::XIEventMask::RAW_KEY_RELEASE,
                    ],
                }],
            )?;
            connection.flush()?;
            let connection = Arc::new(connection);
            let stop = Arc::new(AtomicBool::new(false));
            let (waiting, stopped) = (connection.clone(), stop.clone());
            // X11 keycodes are evdev's plus 8.
            let action = |keycode: u32| keycode.checked_sub(8).and_then(super::action_for);
            std::thread::Builder::new()
                .name("yapfast-keybinds".into())
                .spawn(move || {
                    while !stopped.load(Ordering::Relaxed) {
                        match waiting.wait_for_event() {
                            Ok(Event::XinputRawKeyPress(event)) => {
                                if let Some(action) = action(event.detail) {
                                    super::press(action, true);
                                }
                            }
                            Ok(Event::XinputRawKeyRelease(event)) => {
                                if let Some(action) = action(event.detail) {
                                    super::press(action, false);
                                }
                            }
                            Ok(_) => {}
                            Err(error) => {
                                tracing::warn!(%error, "lost the X server's keys");
                                super::release_all();
                                report(Reach::Focused("Yapfast lost the X server's keys"));
                                break;
                            }
                        }
                    }
                })?;
            Ok(Self {
                connection,
                window,
                stop,
            })
        }
    }

    impl Drop for X11 {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let wake =
                xproto::ClientMessageEvent::new(32, self.window, xproto::AtomEnum::NONE, [0u32; 5]);
            let _ =
                self.connection
                    .send_event(false, self.window, xproto::EventMask::NO_EVENT, wake);
            let _ = self.connection.flush();
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::{ptr, sync::mpsc};

    use windows_sys::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG,
            MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
            WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
            WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2,
        },
    };

    use super::{Action, Keybind, Reach, Report};

    /// Low-level keyboard and/or mouse hooks on a thread of their own, which
    /// see every input event, releases included, and pass each on. Removed when
    /// dropped.
    pub struct Hook {
        thread: u32,
    }

    impl Hook {
        pub fn start(keys: &[(Action, Keybind)], report: Report) -> Option<Self> {
            let need_keyboard = keys
                .iter()
                .any(|(_, k)| !matches!(k.scancode, super::MOUSE_4 | super::MOUSE_5));
            let need_mouse = keys
                .iter()
                .any(|(_, k)| matches!(k.scancode, super::MOUSE_4 | super::MOUSE_5));
            let (started, thread) = mpsc::sync_channel(1);
            let spawned = std::thread::Builder::new()
                .name("yapfast-keybinds".into())
                .spawn(move || {
                    // SAFETY: the hook procedures below only read the event
                    // Windows hands them; the message loop runs on this thread,
                    // which the hooks need, until WM_QUIT.
                    unsafe {
                        let module = GetModuleHandleW(ptr::null());
                        let kbd_hook = if need_keyboard {
                            SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), module, 0)
                        } else {
                            ptr::null_mut()
                        };
                        let mouse_hook = if need_mouse {
                            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0)
                        } else {
                            ptr::null_mut()
                        };
                        if (need_keyboard && kbd_hook.is_null())
                            || (need_mouse && mouse_hook.is_null())
                        {
                            if !kbd_hook.is_null() {
                                UnhookWindowsHookEx(kbd_hook);
                            }
                            if !mouse_hook.is_null() {
                                UnhookWindowsHookEx(mouse_hook);
                            }
                            let _ = started.send(None);
                            return;
                        }
                        let _ = started.send(Some(GetCurrentThreadId()));
                        let mut message: MSG = std::mem::zeroed();
                        while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {}
                        if !kbd_hook.is_null() {
                            UnhookWindowsHookEx(kbd_hook);
                        }
                        if !mouse_hook.is_null() {
                            UnhookWindowsHookEx(mouse_hook);
                        }
                    }
                });
            let thread = spawned.ok().and_then(|_| thread.recv().ok().flatten());
            match thread {
                Some(thread) => {
                    report(Reach::Everywhere(None));
                    Some(Self { thread })
                }
                None => {
                    report(Reach::Focused(
                        "Windows didn't let Yapfast watch keys in other apps",
                    ));
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

    unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            // SAFETY: for HC_ACTION, lparam points at the key's details.
            let key = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
            let extended = if key.flags & LLKHF_EXTENDED != 0 {
                0xE000
            } else {
                0
            };
            let scancode = key.scanCode | extended;
            if wparam as u32 == WM_KEYUP || wparam as u32 == WM_SYSKEYUP {
                super::release_recorded(scancode);
            }
            if let Some(action) = super::action_for(scancode) {
                match wparam as u32 {
                    WM_KEYDOWN | WM_SYSKEYDOWN => super::press(action, true),
                    WM_KEYUP | WM_SYSKEYUP => super::press(action, false),
                    _ => {}
                }
            }
        }
        // SAFETY: the next hook gets the event as it came.
        unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
    }

    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            // SAFETY: for HC_ACTION, lparam points at the mouse event's details.
            let mouse = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
            let button = match (mouse.mouseData >> 16) as u16 {
                XBUTTON1 => super::MOUSE_4,
                XBUTTON2 => super::MOUSE_5,
                _ => super::NONE,
            };
            if wparam as u32 == WM_XBUTTONUP {
                super::release_recorded(button);
            }
            if let Some(action) = super::action_for(button) {
                match wparam as u32 {
                    WM_XBUTTONDOWN => super::press(action, true),
                    WM_XBUTTONUP => super::press(action, false),
                    _ => {}
                }
            }
        }
        // SAFETY: the next hook gets the event as it came.
        unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_keybind_acts_once_per_press_and_respects_recording() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let recorded = heard.clone();
        *HANDLER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some(Arc::new(move |action| recorded.lock().unwrap().push(action)));
        bind(Action::ToggleMute, Some(50));
        set_recording(false);
        press(Action::ToggleMute, true);
        press(Action::ToggleMute, true);
        assert_eq!(*heard.lock().unwrap(), [Action::ToggleMute]);
        press(Action::ToggleMute, false);
        press(Action::ToggleMute, true);
        assert_eq!(
            *heard.lock().unwrap(),
            [Action::ToggleMute, Action::ToggleMute]
        );
        press(Action::ToggleMute, false);

        // Recording suppresses the keybind until the recorded key is released.
        set_recording(true);
        press(Action::ToggleMute, true);
        assert_eq!(
            *heard.lock().unwrap(),
            [Action::ToggleMute, Action::ToggleMute]
        );
        press(Action::ToggleMute, false);
        set_recording(false);

        RECORDED.store(50, Ordering::Relaxed);
        press(Action::ToggleMute, true);
        assert_eq!(
            *heard.lock().unwrap(),
            [Action::ToggleMute, Action::ToggleMute]
        );
        release_recorded(50);
        press(Action::ToggleMute, false);
        press(Action::ToggleMute, true);
        assert_eq!(
            *heard.lock().unwrap(),
            [Action::ToggleMute, Action::ToggleMute, Action::ToggleMute]
        );
        release_all();
        bind(Action::ToggleMute, None);
        *HANDLER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}
