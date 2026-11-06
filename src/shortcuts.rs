//! Global shortcuts: keys heard in any app, bound to the push-to-talk key and
//! to actions such as toggling the microphone. Each is a named shortcut, as
//! the XDG GlobalShortcuts portal has them, of one key and the modifiers held
//! with it, which goes down and comes up. They are heard through that portal
//! on Wayland, XInput 2's raw key events on X11, and a low-level keyboard
//! hook on Windows. Where none of these works, a shortcut works while
//! Yapfast's window has the focus, and settings say so. Push to talk tells
//! [`yapfast_core::push_to_talk::hold`] when its key goes down or comes up; an
//! action is told once each time its shortcut goes down.
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use slint::winit_030::winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    keyboard::{Key, KeyLocation, ModifiersState, NamedKey, PhysicalKey},
    platform::scancode::PhysicalKeyExtScancode,
};
use yapfast_core::runtime::BackgroundRuntime;

/// The modifier keys held with a shortcut's key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1);
    pub const ALT: Self = Self(2);
    pub const SHIFT: Self = Self(4);
    pub const SUPER: Self = Self(8);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn is_none(&self) -> bool {
        self.0 == 0
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    fn from_state(state: ModifiersState) -> Self {
        let mut held = Self::NONE;
        for (on, modifier) in [
            (state.control_key(), Self::CTRL),
            (state.alt_key(), Self::ALT),
            (state.shift_key(), Self::SHIFT),
            (state.super_key(), Self::SUPER),
        ] {
            if on {
                held = held | modifier;
            }
        }
        held
    }

    /// As settings write them in front of the key: "Ctrl+Alt+".
    fn prefix(self) -> String {
        [
            (Self::CTRL, "Ctrl"),
            (Self::ALT, "Alt"),
            (Self::SHIFT, "Shift"),
            (Self::SUPER, "Super"),
        ]
        .into_iter()
        .filter(|(modifier, _)| self.contains(*modifier))
        .map(|(_, name)| format!("{name}+"))
        .collect()
    }

    /// As the XDG shortcuts specification writes them in front of the
    /// keysym: "CTRL+ALT+".
    #[cfg(target_os = "linux")]
    fn trigger_prefix(self) -> String {
        [
            (Self::CTRL, "CTRL"),
            (Self::ALT, "ALT"),
            (Self::SHIFT, "SHIFT"),
            (Self::SUPER, "LOGO"),
        ]
        .into_iter()
        .filter(|(modifier, _)| self.contains(*modifier))
        .map(|(_, name)| format!("{name}+"))
        .collect()
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// What a shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shortcut {
    /// Sends the microphone while held, in the push-to-talk input mode.
    PushToTalk,
    ToggleMute,
    ToggleDeafen,
    LeaveCall,
}

impl Shortcut {
    /// The shortcuts that run an action, in the order settings list them.
    pub const ACTIONS: [Shortcut; 3] = [
        Shortcut::ToggleMute,
        Shortcut::ToggleDeafen,
        Shortcut::LeaveCall,
    ];

    /// The portal's name for it, and the layout's.
    pub fn id(self) -> &'static str {
        match self {
            Shortcut::PushToTalk => "push-to-talk",
            Shortcut::ToggleMute => "toggle-mute",
            Shortcut::ToggleDeafen => "toggle-deafen",
            Shortcut::LeaveCall => "leave-call",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Shortcut::PushToTalk]
            .into_iter()
            .chain(Self::ACTIONS)
            .find(|shortcut| shortcut.id() == id)
    }

    /// As the desktop's shortcut settings and Yapfast's show it.
    pub fn description(self) -> &'static str {
        match self {
            Shortcut::PushToTalk => "Push to talk",
            Shortcut::ToggleMute => "Toggle mute",
            Shortcut::ToggleDeafen => "Toggle deafen",
            Shortcut::LeaveCall => "Leave the call",
        }
    }

    /// What pressing it does, finishing "Press the shortcut in any app to".
    fn effect(self) -> &'static str {
        match self {
            Shortcut::PushToTalk => "talk",
            Shortcut::ToggleMute => "mute or unmute your microphone",
            Shortcut::ToggleDeafen => "deafen or undeafen yourself",
            Shortcut::LeaveCall => "leave the call",
        }
    }
}

/// A key recorded in settings, with the modifiers held with it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keybind {
    /// winit's scan code for it: evdev's on Linux, which X11 reports 8
    /// higher, and set 1's on Windows, with 0xE000 for an extended key.
    pub scancode: u32,
    /// As settings show it, such as "V" or "Right Ctrl".
    pub name: String,
    /// Its XKB keysym name, such as "v" or "Control_R", which the
    /// GlobalShortcuts portal takes as the shortcut it would like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    /// The modifiers that must be held with it; none for a key on its own,
    /// which works whatever else is held.
    #[serde(default, skip_serializing_if = "Modifiers::is_none")]
    pub modifiers: Modifiers,
}

impl Keybind {
    /// The key a key press names, unless it has no scan code, with the
    /// modifiers held with it.
    pub fn recorded(event: &KeyEvent, modifiers: Modifiers) -> Option<Self> {
        let scancode = event.physical_key.to_scancode()?;
        Some(Self {
            scancode,
            name: name(event),
            trigger: trigger(event),
            modifiers,
        })
    }

    /// As settings show it: "Ctrl+Alt+M".
    pub fn label(&self) -> String {
        format!("{}{}", self.modifiers.prefix(), self.name)
    }

    /// The shortcut the portal is offered as the one wanted.
    #[cfg(target_os = "linux")]
    fn portal_trigger(&self) -> Option<String> {
        let key = self.trigger.as_ref()?;
        Some(format!("{}{key}", self.modifiers.trigger_prefix()))
    }
}

/// Whether a key press is a modifier on its own, which a shortcut waits to
/// see the rest of before it takes it as the key.
fn is_modifier(event: &KeyEvent) -> bool {
    matches!(
        event.logical_key,
        Key::Named(
            NamedKey::Control | NamedKey::Shift | NamedKey::Alt | NamedKey::Super | NamedKey::Meta
        )
    )
}

/// The modifier a scan code is, if it is one: evdev's on Linux.
#[cfg(not(windows))]
fn modifier_of(scancode: u32) -> Modifiers {
    match scancode {
        29 | 97 => Modifiers::CTRL,
        42 | 54 => Modifiers::SHIFT,
        56 | 100 => Modifiers::ALT,
        125 | 126 => Modifiers::SUPER,
        _ => Modifiers::NONE,
    }
}

/// The modifier a scan code is, if it is one: set 1's on Windows.
#[cfg(windows)]
fn modifier_of(scancode: u32) -> Modifiers {
    match scancode {
        0x1D | 0xE01D => Modifiers::CTRL,
        0x2A | 0x36 => Modifiers::SHIFT,
        0x38 | 0xE038 => Modifiers::ALT,
        0xE05B | 0xE05C => Modifiers::SUPER,
        _ => Modifiers::NONE,
    }
}

/// What recording a shortcut makes of a key event.
#[derive(Debug, PartialEq, Eq)]
pub enum Recorded {
    /// Nothing yet.
    Nothing,
    /// Escape: stop without changing the shortcut.
    Cancelled,
    Key(Keybind),
}

/// Records a shortcut from the window's key events: the next key pressed,
/// with the modifiers held, or, for a modifier alone, once it comes up
/// without another key having been pressed.
#[derive(Default)]
pub struct Recorder {
    /// A modifier pressed and not yet let go.
    pending: Option<Keybind>,
}

impl Recorder {
    pub fn event(&mut self, event: &KeyEvent, held: Modifiers) -> Recorded {
        if event.repeat {
            return Recorded::Nothing;
        }
        match event.state {
            ElementState::Pressed if event.logical_key == Key::Named(NamedKey::Escape) => {
                self.pending = None;
                Recorded::Cancelled
            }
            ElementState::Pressed if is_modifier(event) => {
                self.pending = Keybind::recorded(event, Modifiers::NONE);
                Recorded::Nothing
            }
            ElementState::Pressed => {
                self.pending = None;
                Keybind::recorded(event, held).map_or(Recorded::Nothing, Recorded::Key)
            }
            ElementState::Released => match self.pending.take() {
                Some(key) if event.physical_key.to_scancode() == Some(key.scancode) => {
                    Recorded::Key(key)
                }
                other => {
                    self.pending = other;
                    Recorded::Nothing
                }
            },
        }
    }
}

/// The key without Shift or Caps Lock, so a letter reads as itself.
fn unmodified(event: &KeyEvent) -> Key {
    #[cfg(any(target_os = "linux", windows))]
    {
        use slint::winit_030::winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
        event.key_without_modifiers()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        event.logical_key.clone()
    }
}

fn name(event: &KeyEvent) -> String {
    let side = match event.location {
        KeyLocation::Left => "Left ",
        KeyLocation::Right => "Right ",
        KeyLocation::Numpad => "Numpad ",
        KeyLocation::Standard => "",
    };
    match unmodified(event) {
        Key::Character(text) => format!("{side}{}", text.to_uppercase()),
        Key::Named(named) => {
            let named = match named {
                NamedKey::Control => "Ctrl".to_owned(),
                NamedKey::Super | NamedKey::Meta => "Super".to_owned(),
                NamedKey::ArrowUp => "Up".to_owned(),
                NamedKey::ArrowDown => "Down".to_owned(),
                NamedKey::ArrowLeft => "Left".to_owned(),
                NamedKey::ArrowRight => "Right".to_owned(),
                NamedKey::Space => "Space".to_owned(),
                named => format!("{named:?}"),
            };
            format!("{side}{named}")
        }
        _ => match event.physical_key {
            PhysicalKey::Code(code) => format!("{code:?}"),
            PhysicalKey::Unidentified(_) => "Unknown key".to_owned(),
        },
    }
}

/// The key's keysym name, as the XDG shortcuts specification writes a
/// trigger; on Linux, where only the portal needs it.
#[cfg(target_os = "linux")]
fn trigger(event: &KeyEvent) -> Option<String> {
    let right = event.location == KeyLocation::Right;
    let side =
        |left: &str, right_name: &str| Some(if right { right_name } else { left }.to_owned());
    match unmodified(event) {
        Key::Character(text) => {
            let mut chars = text.chars();
            let (Some(char), None) = (chars.next(), chars.next()) else {
                return None;
            };
            let name = xkeysym::Keysym::from_char(char).name()?;
            Some(name.strip_prefix("XK_").unwrap_or(name).to_owned())
        }
        Key::Named(named) => match named {
            NamedKey::Control => side("Control_L", "Control_R"),
            NamedKey::Shift => side("Shift_L", "Shift_R"),
            NamedKey::Alt => side("Alt_L", "Alt_R"),
            NamedKey::Super | NamedKey::Meta => side("Super_L", "Super_R"),
            NamedKey::AltGraph => Some("ISO_Level3_Shift".to_owned()),
            NamedKey::CapsLock => Some("Caps_Lock".to_owned()),
            NamedKey::ScrollLock => Some("Scroll_Lock".to_owned()),
            NamedKey::NumLock => Some("Num_Lock".to_owned()),
            NamedKey::PageUp => Some("Prior".to_owned()),
            NamedKey::PageDown => Some("Next".to_owned()),
            NamedKey::Space => Some("space".to_owned()),
            NamedKey::Enter => Some("Return".to_owned()),
            NamedKey::Backspace => Some("BackSpace".to_owned()),
            NamedKey::ArrowUp => Some("Up".to_owned()),
            NamedKey::ArrowDown => Some("Down".to_owned()),
            NamedKey::ArrowLeft => Some("Left".to_owned()),
            NamedKey::ArrowRight => Some("Right".to_owned()),
            NamedKey::Tab
            | NamedKey::Insert
            | NamedKey::Delete
            | NamedKey::Home
            | NamedKey::End
            | NamedKey::Pause => Some(format!("{named:?}")),
            NamedKey::ContextMenu => Some("Menu".to_owned()),
            named => {
                let name = format!("{named:?}");
                // F1 to F35 are their own keysyms.
                (name.starts_with('F') && name[1..].parse::<u8>().is_ok()).then_some(name)
            }
        },
        _ => None,
    }
}

#[cfg(not(target_os = "linux"))]
fn trigger(_event: &KeyEvent) -> Option<String> {
    None
}

/// Where a shortcut works.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Asking the desktop for it.
    Asking,
    /// In every app; with the shortcut the desktop gave it, in its own
    /// words, when it says.
    Everywhere(Option<String>),
    /// Only while Yapfast's window has the focus, for this reason, which
    /// the note goes on from.
    Focused(&'static str),
}

impl Reach {
    /// What settings say under a shortcut.
    pub fn note(&self, shortcut: Shortcut) -> String {
        match self {
            Reach::Asking => {
                "Asking your desktop to let Yapfast use the key in every app...".into()
            }
            Reach::Everywhere(None) if shortcut == Shortcut::PushToTalk => {
                "Hold the key in any app to talk.".into()
            }
            Reach::Everywhere(None) => format!(
                "Press the shortcut in any app to {}.",
                shortcut.effect()
            ),
            Reach::Everywhere(Some(desktop)) if desktop.is_empty() => {
                "Your desktop hasn't allowed this shortcut yet. Approve it in your system's global shortcuts settings.".into()
            }
            Reach::Everywhere(Some(desktop)) => format!(
                "Your desktop gave {} the shortcut {desktop}. Use it in any app to {}; change it in your system's global shortcuts settings.",
                shortcut.description().to_lowercase(),
                shortcut.effect()
            ),
            Reach::Focused(why) => format!(
                "{why}, so this works only while Yapfast's window is in front."
            ),
        }
    }
}

/// A shortcut with the key it is bound to.
pub type Binding = (Shortcut, Keybind);

/// Follows which of some shortcuts are down as keys come and go, from any
/// source of key events, in the scan codes [`Keybind`] holds.
#[derive(Debug, Default)]
struct Matcher {
    bindings: Vec<Binding>,
    /// The modifier keys held, by scan code.
    down: Vec<u32>,
    /// The shortcuts down now.
    active: Vec<Shortcut>,
}

impl Matcher {
    fn new(bindings: &[Binding]) -> Self {
        Self {
            bindings: bindings.to_vec(),
            ..Self::default()
        }
    }

    fn held(&self) -> Modifiers {
        self.down
            .iter()
            .fold(Modifiers::NONE, |held, code| held | modifier_of(*code))
    }

    /// A key went down or came up; the shortcuts that did with it, down
    /// as `true`. A shortcut goes down with its key when exactly its
    /// modifiers are held, or whatever is held when it has none, and comes up
    /// when its key or any of its modifiers does. Key repeats change nothing.
    fn key(&mut self, scancode: u32, pressed: bool) -> Vec<(Shortcut, bool)> {
        let mut events = Vec::new();
        let modifier = modifier_of(scancode);
        if pressed {
            if !modifier.is_empty() && !self.down.contains(&scancode) {
                self.down.push(scancode);
            }
            let held = self.held();
            for (shortcut, key) in &self.bindings {
                if key.scancode == scancode
                    && (key.modifiers.is_empty() || key.modifiers == held)
                    && !self.active.contains(shortcut)
                {
                    self.active.push(*shortcut);
                    events.push((*shortcut, true));
                }
            }
        } else {
            self.down.retain(|code| *code != scancode);
            let held = self.held();
            let bindings = &self.bindings;
            self.active.retain(|shortcut| {
                let released = bindings.iter().any(|(bound, key)| {
                    bound == shortcut
                        && (key.scancode == scancode
                            || (!modifier.is_empty()
                                && key.modifiers.contains(modifier)
                                && !held.contains(modifier)))
                });
                if released {
                    events.push((*shortcut, false));
                }
                !released
            });
        }
        events
    }

    /// Every key is let go, as when the keyboard goes away.
    fn release_all(&mut self) -> Vec<(Shortcut, bool)> {
        self.down.clear();
        self.active
            .drain(..)
            .map(|shortcut| (shortcut, false))
            .collect()
    }
}

/// Runs the action of a shortcut that went down.
type Press = Arc<dyn Fn(Shortcut) + Send + Sync>;
static PRESS: Mutex<Option<Press>> = Mutex::new(None);

/// Tells what each shortcut's going down or up means: push to talk holds or
/// lets go of its key, and an action runs each time its shortcut goes down.
fn deliver(events: impl IntoIterator<Item = (Shortcut, bool)>) {
    for (shortcut, down) in events {
        if shortcut == Shortcut::PushToTalk {
            yapfast_core::push_to_talk::hold(down);
        } else if down {
            let press = lock(&PRESS).clone();
            if let Some(press) = press {
                press(shortcut);
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Follows the shortcuts that work only while the window has the focus, from
/// its own key events; none while every shortcut is heard in any app.
static FOCUSED: Mutex<Option<Matcher>> = Mutex::new(None);
const NONE: u32 = u32::MAX;
/// The push-to-talk key's scan code while it is watched, and whether the
/// window's latest key press was it.
static TALK_KEY: AtomicU32 = AtomicU32::new(NONE);
static PRESSED_TALK_KEY: AtomicBool = AtomicBool::new(false);
/// The modifiers the window has held down, as winit tells them.
static HELD: AtomicU8 = AtomicU8::new(0);

/// Whether the key press Slint is handling is the push-to-talk key, which
/// shouldn't also type into the composer.
pub fn pressed_talk_key() -> bool {
    PRESSED_TALK_KEY.load(Ordering::Relaxed)
}

/// The modifiers held in the window, for recording a shortcut.
pub fn held_modifiers() -> Modifiers {
    Modifiers(HELD.load(Ordering::Relaxed))
}

/// Holds the shortcuts from the window's own events while no global key works
/// for them. Losing the focus lets them go, as their release won't come.
pub fn window_event(event: &WindowEvent) {
    match event {
        WindowEvent::ModifiersChanged(modifiers) => {
            HELD.store(
                Modifiers::from_state(modifiers.state()).0,
                Ordering::Relaxed,
            );
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let code = event.physical_key.to_scancode();
            if event.state == ElementState::Pressed {
                let key = TALK_KEY.load(Ordering::Relaxed);
                PRESSED_TALK_KEY.store(key != NONE && code == Some(key), Ordering::Relaxed);
            }
            if event.repeat {
                return;
            }
            let Some(code) = code else { return };
            let events = lock(&FOCUSED)
                .as_mut()
                .map(|matcher| matcher.key(code, event.state == ElementState::Pressed));
            deliver(events.unwrap_or_default());
        }
        WindowEvent::Focused(false) => {
            let events = lock(&FOCUSED).as_mut().map(Matcher::release_all);
            deliver(events.unwrap_or_default());
        }
        _ => {}
    }
}

/// Watches the shortcuts in every app it can, until dropped.
pub struct Watcher {
    bindings: Vec<Binding>,
    reach: Arc<Mutex<HashMap<Shortcut, Reach>>>,
    #[cfg(target_os = "linux")]
    _global: Option<linux::Global>,
    #[cfg(windows)]
    _global: Option<windows::Hook>,
}

impl Watcher {
    /// Starts watching `bindings`; `told` hears where each works whenever that
    /// changes, from any thread, and `press` is told each time an action's
    /// shortcut goes down, from any thread too.
    pub fn start(
        bindings: Vec<Binding>,
        runtime: &BackgroundRuntime,
        told: impl Fn(Shortcut, Reach) + Send + Sync + 'static,
        press: impl Fn(Shortcut) + Send + Sync + 'static,
    ) -> Self {
        let reach = Arc::new(Mutex::new(HashMap::new()));
        *lock(&PRESS) = Some(Arc::new(press));
        let talk = bindings
            .iter()
            .find(|(shortcut, _)| *shortcut == Shortcut::PushToTalk)
            .map_or(NONE, |(_, key)| key.scancode);
        TALK_KEY.store(talk, Ordering::Relaxed);
        let (shared, wanted) = (reach.clone(), bindings.clone());
        let report: Report = Arc::new(move |shortcut: Shortcut, now: Reach| {
            let mut reach = lock(&shared);
            reach.insert(shortcut, now.clone());
            let focused: Vec<Binding> = wanted
                .iter()
                .filter(|(shortcut, _)| matches!(reach.get(shortcut), Some(Reach::Focused(_))))
                .cloned()
                .collect();
            *lock(&FOCUSED) = (!focused.is_empty()).then(|| Matcher::new(&focused));
            drop(reach);
            told(shortcut, now);
        });
        #[cfg(target_os = "linux")]
        let global = linux::Global::start(&bindings, runtime, report);
        #[cfg(windows)]
        let global = {
            let _ = runtime;
            windows::Hook::start(&bindings, report)
        };
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = runtime;
            for (shortcut, _) in &bindings {
                report(
                    *shortcut,
                    Reach::Focused("Yapfast can't watch keys in other apps on this system yet"),
                );
            }
        }
        Self {
            bindings,
            reach,
            #[cfg(any(target_os = "linux", windows))]
            _global: global,
        }
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    pub fn reach(&self, shortcut: Shortcut) -> Option<Reach> {
        lock(&self.reach).get(&shortcut).cloned()
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        *lock(&FOCUSED) = None;
        *lock(&PRESS) = None;
        TALK_KEY.store(NONE, Ordering::Relaxed);
        yapfast_core::push_to_talk::hold(false);
    }
}

type Report = Arc<dyn Fn(Shortcut, Reach) + Send + Sync>;

/// Tells where every shortcut of `bindings` works.
fn report_all(report: &Report, bindings: &[Binding], reach: &Reach) {
    for (shortcut, _) in bindings {
        report(*shortcut, reach.clone());
    }
}

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

    use super::{Binding, Matcher, Reach, Report, Shortcut, deliver, report_all};

    /// Global shortcuts on Wayland or X11; stop when dropped.
    pub enum Global {
        Portal { _session: BackgroundTask<()> },
        X11 { _events: X11 },
    }

    impl Global {
        /// Wayland's desktops grab keys through the portal; X11 lets any
        /// client hear every key.
        pub fn start(
            bindings: &[Binding],
            runtime: &BackgroundRuntime,
            report: Report,
        ) -> Option<Self> {
            let wayland =
                std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
            if wayland {
                report_all(&report, bindings, &Reach::Asking);
                let wanted = bindings.to_vec();
                return Some(Global::Portal {
                    _session: runtime.spawn(async move {
                        if let Err(error) = portal(&wanted, &report).await {
                            tracing::info!(
                                error = format!("{error:#}"),
                                "no global shortcuts through the portal"
                            );
                            report_all(
                                &report,
                                &wanted,
                                &Reach::Focused(
                                    "Your desktop doesn't offer Yapfast global shortcuts",
                                ),
                            );
                        }
                    }),
                });
            }
            match X11::start(bindings, report.clone()) {
                Ok(x11) => {
                    report_all(&report, bindings, &Reach::Everywhere(None));
                    Some(Global::X11 { _events: x11 })
                }
                Err(error) => {
                    tracing::info!(error = format!("{error:#}"), "no global shortcuts on X11");
                    report_all(
                        &report,
                        bindings,
                        &Reach::Focused("Yapfast couldn't watch keys in other apps"),
                    );
                    None
                }
            }
        }
    }

    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";
    const SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";

    /// Binds the shortcuts through the GlobalShortcuts portal and holds them
    /// as the desktop says, until the task is dropped, which ends the session
    /// with the connection.
    async fn portal(bindings: &[Binding], report: &Report) -> Result<()> {
        let connection = Connection::session().await?;
        // Names Yapfast to the portal, which outside a sandbox can't tell
        // the app otherwise; older portals don't have it.
        let _ = connection
            .call_method(
                Some(PORTAL),
                PATH,
                Some("org.freedesktop.host.portal.Registry"),
                "Register",
                &(crate::icon::APP_ID, HashMap::<&str, Value>::new()),
            )
            .await;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(PORTAL)?
            .build();
        let mut signals = MessageStream::for_match_rule(rule, &connection, Some(16)).await?;
        let sender = connection
            .unique_name()
            .context("no name on the session bus")?
            .trim_start_matches(':')
            .replace('.', "_");
        let token = format!("yapfast_{}", std::process::id());
        let request = |step: &str| format!("{PATH}/request/{sender}/{token}_{step}");

        let options = HashMap::from([
            ("handle_token", Value::from(format!("{token}_session"))),
            ("session_handle_token", Value::from(token.clone())),
        ]);
        connection
            .call_method(
                Some(PORTAL),
                PATH,
                Some(SHORTCUTS),
                "CreateSession",
                &(options,),
            )
            .await?;
        let results = response(&mut signals, &request("session")).await?;
        let session = match results.get("session_handle").map(|value| &**value) {
            Some(Value::Str(handle)) => OwnedObjectPath::try_from(handle.as_str())?,
            Some(Value::ObjectPath(handle)) => handle.clone().into(),
            _ => bail!("the portal gave no session"),
        };

        let wanted: Vec<(&str, HashMap<&str, Value>)> = bindings
            .iter()
            .map(|(shortcut, key)| {
                let mut details =
                    HashMap::from([("description", Value::from(shortcut.description()))]);
                if let Some(trigger) = key.portal_trigger() {
                    details.insert("preferred_trigger", Value::from(trigger));
                }
                (shortcut.id(), details)
            })
            .collect();
        let options = HashMap::from([("handle_token", Value::from(format!("{token}_bind")))]);
        connection
            .call_method(
                Some(PORTAL),
                PATH,
                Some(SHORTCUTS),
                "BindShortcuts",
                &(&session, wanted, "", options),
            )
            .await?;
        let results = response(&mut signals, &request("bind")).await?;
        report_given(report, bindings, &described(&results));

        while let Some(message) = signals.next().await {
            let message = message?;
            let header = message.header();
            let down = match header.member().map(|member| member.as_str()) {
                Some("Activated") => true,
                Some("Deactivated") => false,
                Some("ShortcutsChanged") => {
                    if let Ok((path, shortcuts)) = message
                        .body()
                        .deserialize::<(ObjectPath, Vec<(String, HashMap<String, OwnedValue>)>)>()
                        && path == *session
                    {
                        report_given(report, bindings, &trigger_descriptions(&shortcuts));
                    }
                    continue;
                }
                Some("Closed") if header.path().is_some_and(|path| *path == *session) => {
                    bail!("the desktop closed the shortcuts' session");
                }
                _ => continue,
            };
            if let Ok((path, id, ..)) =
                message
                    .body()
                    .deserialize::<(ObjectPath, &str, u64, HashMap<String, OwnedValue>)>()
                && path == *session
                && let Some(shortcut) = Shortcut::from_id(id)
            {
                deliver([(shortcut, down)]);
            }
        }
        Err(anyhow!("the session bus closed"))
    }

    /// Tells where each shortcut works, in the desktop's words for the
    /// shortcut it gave it, empty for none yet.
    fn report_given(report: &Report, bindings: &[Binding], given: &HashMap<String, String>) {
        for (shortcut, _) in bindings {
            let description = given.get(shortcut.id()).cloned().unwrap_or_default();
            report(*shortcut, Reach::Everywhere(Some(description)));
        }
    }

    /// Waits for the portal's answer to a request, at its object path.
    async fn response(
        signals: &mut MessageStream,
        request: &str,
    ) -> Result<HashMap<String, OwnedValue>> {
        while let Some(message) = signals.next().await {
            let message = message?;
            let header = message.header();
            if header
                .member()
                .is_none_or(|member| member.as_str() != "Response")
                || header.path().is_none_or(|path| path.as_str() != request)
            {
                continue;
            }
            let (code, results) = message
                .body()
                .deserialize::<(u32, HashMap<String, OwnedValue>)>()?;
            return match code {
                0 => Ok(results),
                1 => Err(anyhow!("the shortcuts were declined")),
                _ => Err(anyhow!("the portal failed ({code})")),
            };
        }
        Err(anyhow!("the session bus closed"))
    }

    /// The shortcuts the desktop bound, by name, in its words for each; one
    /// it bound none to yet is empty or missing.
    fn described(results: &HashMap<String, OwnedValue>) -> HashMap<String, String> {
        results
            .get("shortcuts")
            .and_then(|value| {
                Vec::<(String, HashMap<String, OwnedValue>)>::try_from(value.try_clone().ok()?).ok()
            })
            .map(|shortcuts| trigger_descriptions(&shortcuts))
            .unwrap_or_default()
    }

    fn trigger_descriptions(
        shortcuts: &[(String, HashMap<String, OwnedValue>)],
    ) -> HashMap<String, String> {
        shortcuts
            .iter()
            .filter_map(|(id, details)| {
                match details.get("trigger_description").map(|value| &**value) {
                    Some(Value::Str(text)) => Some((id.clone(), text.to_string())),
                    _ => None,
                }
            })
            .collect()
    }

    /// XInput 2's raw key events from the root window, which come whatever
    /// window has the focus and leave the key to it. A thread waits on them.
    pub struct X11 {
        connection: Arc<RustConnection>,
        window: xproto::Window,
        stop: Arc<AtomicBool>,
    }

    impl X11 {
        fn start(bindings: &[Binding], report: Report) -> Result<Self> {
            let (connection, screen) = x11rb::connect(None)?;
            let root = connection.setup().roots[screen].root;
            let version = connection.xinput_xi_query_version(2, 2)?.reply()?;
            if version.major_version < 2 {
                bail!("the X server has no XInput 2");
            }
            // A window of its own to wake the thread with when stopping.
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
            let mut matcher = Matcher::new(bindings);
            let wanted = bindings.to_vec();
            std::thread::Builder::new()
                .name("yapfast-shortcuts".into())
                .spawn(move || {
                    while !stopped.load(Ordering::Relaxed) {
                        // X11 keycodes are evdev's plus 8.
                        match waiting.wait_for_event() {
                            Ok(Event::XinputRawKeyPress(event)) => {
                                deliver(matcher.key(event.detail.saturating_sub(8), true));
                            }
                            Ok(Event::XinputRawKeyRelease(event)) => {
                                deliver(matcher.key(event.detail.saturating_sub(8), false));
                            }
                            Ok(_) => {}
                            Err(error) => {
                                tracing::warn!(%error, "lost the X server's keys");
                                deliver(matcher.release_all());
                                report_all(
                                    &report,
                                    &wanted,
                                    &Reach::Focused("Yapfast lost the X server's keys"),
                                );
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

    #[cfg(test)]
    mod tests {
        use std::time::{Duration, Instant};

        use super::*;
        use crate::shortcuts::{Keybind, Modifiers};

        /// Waits up to two seconds for the key to be `held` or not.
        fn wait_held(held: bool) {
            let until = Instant::now() + Duration::from_secs(2);
            while yapfast_core::push_to_talk::held() != held {
                assert!(Instant::now() < until, "the key wasn't held: {held}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn key(scancode: u32, modifiers: Modifiers) -> Keybind {
            Keybind {
                scancode,
                name: "V".into(),
                trigger: Some("v".into()),
                modifiers,
            }
        }

        /// Pressing the key in another client holds it. Needs `DISPLAY` on a
        /// private X server, such as `Xvfb :57`, and `xdotool`, which types
        /// through XTEST as a keyboard would.
        #[test]
        #[ignore = "needs DISPLAY on a private X server and xdotool"]
        fn x11_hears_the_key_whatever_has_the_focus() {
            // evdev's V
            let bindings = [(Shortcut::PushToTalk, key(47, Modifiers::NONE))];
            let x11 = X11::start(&bindings, Arc::new(|_, _| {})).unwrap();
            let xdotool = |action| {
                let status = std::process::Command::new("xdotool")
                    .args([action, "v"])
                    .status()
                    .unwrap();
                assert!(status.success());
            };
            xdotool("keydown");
            wait_held(true);
            xdotool("keyup");
            wait_held(false);
            // Another key leaves it alone.
            std::process::Command::new("xdotool")
                .args(["key", "b"])
                .status()
                .unwrap();
            std::thread::sleep(Duration::from_millis(100));
            assert!(!yapfast_core::push_to_talk::held());
            drop(x11);
        }

        /// A shortcut with modifiers goes down only with exactly them held.
        #[test]
        #[ignore = "needs DISPLAY on a private X server and xdotool"]
        fn x11_hears_a_key_with_its_modifiers_only() {
            let bindings = [(Shortcut::PushToTalk, key(47, Modifiers::CTRL))];
            let x11 = X11::start(&bindings, Arc::new(|_, _| {})).unwrap();
            let xdotool = |args: &[&str]| {
                assert!(
                    std::process::Command::new("xdotool")
                        .args(args)
                        .status()
                        .unwrap()
                        .success()
                );
            };
            xdotool(&["key", "v"]);
            std::thread::sleep(Duration::from_millis(100));
            assert!(!yapfast_core::push_to_talk::held(), "V alone");
            xdotool(&["keydown", "ctrl", "keydown", "v"]);
            wait_held(true);
            xdotool(&["keyup", "v", "keyup", "ctrl"]);
            wait_held(false);
            drop(x11);
        }

        /// A stand-in for xdg-desktop-portal's GlobalShortcuts: it answers
        /// each request with Response on the request's path, binds each
        /// shortcut to what was preferred, and then presses the first.
        struct FakePortal;

        #[zbus::interface(name = "org.freedesktop.portal.GlobalShortcuts")]
        impl FakePortal {
            async fn create_session(
                &self,
                options: HashMap<String, OwnedValue>,
                #[zbus(header)] header: zbus::message::Header<'_>,
                #[zbus(connection)] connection: &Connection,
            ) -> OwnedObjectPath {
                let request = request_path(&header, &options);
                let session = format!("{PATH}/session/test/1");
                let results = HashMap::from([("session_handle", Value::from(session))]);
                respond(connection, &request, results).await;
                request
            }

            async fn bind_shortcuts(
                &self,
                session: OwnedObjectPath,
                shortcuts: Vec<(String, HashMap<String, OwnedValue>)>,
                _parent_window: String,
                options: HashMap<String, OwnedValue>,
                #[zbus(header)] header: zbus::message::Header<'_>,
                #[zbus(connection)] connection: &Connection,
            ) -> OwnedObjectPath {
                let request = request_path(&header, &options);
                let bound: Vec<_> = shortcuts
                    .iter()
                    .map(|(id, details)| {
                        let Some(Value::Str(trigger)) =
                            details.get("preferred_trigger").map(|v| &**v)
                        else {
                            panic!("no preferred trigger");
                        };
                        (
                            id.clone(),
                            HashMap::from([(
                                "trigger_description".to_owned(),
                                Value::from(trigger.to_uppercase()),
                            )]),
                        )
                    })
                    .collect();
                respond(
                    connection,
                    &request,
                    HashMap::from([("shortcuts", Value::from(bound))]),
                )
                .await;
                let connection = connection.clone();
                let (session, id) = (session.clone(), shortcuts[0].0.clone());
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(100));
                    zbus::block_on(press(&connection, &session, &id, "Activated"));
                });
                request
            }
        }

        fn request_path(
            header: &zbus::message::Header<'_>,
            options: &HashMap<String, OwnedValue>,
        ) -> OwnedObjectPath {
            let sender = header
                .sender()
                .unwrap()
                .trim_start_matches(':')
                .replace('.', "_");
            let Some(Value::Str(token)) = options.get("handle_token").map(|v| &**v) else {
                panic!("no handle token");
            };
            OwnedObjectPath::try_from(format!("{PATH}/request/{sender}/{token}")).unwrap()
        }

        async fn respond(
            connection: &Connection,
            request: &OwnedObjectPath,
            results: HashMap<&str, Value<'_>>,
        ) {
            connection
                .emit_signal(
                    None::<()>,
                    request,
                    "org.freedesktop.portal.Request",
                    "Response",
                    &(0u32, results),
                )
                .await
                .unwrap();
        }

        async fn press(connection: &Connection, session: &OwnedObjectPath, id: &str, signal: &str) {
            let options = HashMap::<&str, Value>::new();
            connection
                .emit_signal(
                    None::<()>,
                    PATH,
                    SHORTCUTS,
                    signal,
                    &(session, id, 0u64, options),
                )
                .await
                .unwrap();
        }

        /// The portal's handshake for push to talk and an action, with the
        /// modifiers in the shortcut offered, and push to talk going down and
        /// up. Needs a private session bus, as `dbus-run-session` gives.
        #[test]
        #[ignore = "needs a private session bus"]
        fn the_portal_binds_the_shortcuts_and_holds_them() {
            let server = zbus::blocking::connection::Builder::session()
                .unwrap()
                .name(PORTAL)
                .unwrap()
                .serve_at(PATH, FakePortal)
                .unwrap()
                .build()
                .unwrap();
            let told = Arc::new(std::sync::Mutex::new(Vec::new()));
            let heard = told.clone();
            let report: Report =
                Arc::new(move |shortcut, reach| heard.lock().unwrap().push((shortcut, reach)));
            let bindings = vec![
                (Shortcut::PushToTalk, key(47, Modifiers::NONE)),
                (
                    Shortcut::ToggleMute,
                    key(50, Modifiers::CTRL | Modifiers::ALT),
                ),
            ];
            let runtime = BackgroundRuntime::new().unwrap();
            let _task = runtime.spawn(async move { portal(&bindings, &report).await });
            let until = Instant::now() + Duration::from_secs(5);
            while !yapfast_core::push_to_talk::held() {
                assert!(
                    Instant::now() < until,
                    "the shortcut wasn't held: {:?}",
                    told.lock().unwrap()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(
                *told.lock().unwrap(),
                [
                    (Shortcut::PushToTalk, Reach::Everywhere(Some("V".into()))),
                    (
                        Shortcut::ToggleMute,
                        Reach::Everywhere(Some("CTRL+ALT+V".into()))
                    ),
                ]
            );
            let session = OwnedObjectPath::try_from(format!("{PATH}/session/test/1")).unwrap();
            zbus::block_on(press(
                server.inner(),
                &session,
                Shortcut::PushToTalk.id(),
                "Deactivated",
            ));
            wait_held(false);
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::{
        ptr,
        sync::{Mutex, mpsc},
    };

    use windows_sys::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG,
            PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN,
            WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
        },
    };

    use super::{Binding, Matcher, Reach, Report, deliver, lock, report_all};

    /// The shortcuts the hook watches.
    static MATCHER: Mutex<Option<Matcher>> = Mutex::new(None);

    /// A low-level keyboard hook on a thread of its own, which sees every
    /// key, its release included, and passes each on. Removed when dropped.
    pub struct Hook {
        thread: u32,
    }

    impl Hook {
        pub fn start(bindings: &[Binding], report: Report) -> Option<Self> {
            *lock(&MATCHER) = Some(Matcher::new(bindings));
            let (started, thread) = mpsc::sync_channel(1);
            let spawned = std::thread::Builder::new()
                .name("yapfast-shortcuts".into())
                .spawn(move || {
                    // SAFETY: the hook procedure below only reads the event
                    // Windows hands it; the message loop runs on this thread,
                    // which the hook needs, until WM_QUIT.
                    unsafe {
                        let hook = SetWindowsHookExW(
                            WH_KEYBOARD_LL,
                            Some(proc),
                            GetModuleHandleW(ptr::null()),
                            0,
                        );
                        if hook.is_null() {
                            let _ = started.send(None);
                            return;
                        }
                        let _ = started.send(Some(GetCurrentThreadId()));
                        let mut message: MSG = std::mem::zeroed();
                        while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {}
                        UnhookWindowsHookEx(hook);
                    }
                });
            let thread = spawned.ok().and_then(|_| thread.recv().ok().flatten());
            match thread {
                Some(thread) => {
                    report_all(&report, bindings, &Reach::Everywhere(None));
                    Some(Self { thread })
                }
                None => {
                    report_all(
                        &report,
                        bindings,
                        &Reach::Focused("Windows didn't let Yapfast watch keys in other apps"),
                    );
                    None
                }
            }
        }
    }

    impl Drop for Hook {
        fn drop(&mut self) {
            *lock(&MATCHER) = None;
            // SAFETY: posting to a thread id is harmless if it has ended.
            unsafe {
                PostThreadMessageW(self.thread, WM_QUIT, 0, 0);
            }
        }
    }

    /// Notes the shortcuts' keys going down or up, and passes every key on.
    unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            // SAFETY: for HC_ACTION, lparam points at the key's details.
            let key = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
            let extended = if key.flags & LLKHF_EXTENDED != 0 {
                0xE000
            } else {
                0
            };
            let pressed = match wparam as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => Some(true),
                WM_KEYUP | WM_SYSKEYUP => Some(false),
                _ => None,
            };
            if let Some(pressed) = pressed {
                let events = lock(&MATCHER)
                    .as_mut()
                    .map(|matcher| matcher.key(key.scanCode | extended, pressed));
                deliver(events.unwrap_or_default());
            }
        }
        // SAFETY: the next hook gets the event as it came.
        unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
    }

    #[cfg(test)]
    mod tests {
        use std::{
            sync::Arc,
            time::{Duration, Instant},
        };

        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
            SendInput,
        };

        use super::*;
        use crate::shortcuts::{Keybind, Modifiers, Shortcut};

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

        fn wait_held(held: bool) {
            let until = Instant::now() + Duration::from_secs(2);
            while yapfast_core::push_to_talk::held() != held {
                assert!(Instant::now() < until, "the key wasn't held: {held}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        /// The hook hears the key typed anywhere, down and up. Run it alone,
        /// on a desktop, or under Wine with an X server.
        #[test]
        #[ignore = "types into the desktop"]
        fn the_hook_hears_the_key_go_down_and_up() {
            let key = Keybind {
                scancode: 0x2F, // V
                name: "V".into(),
                trigger: None,
                modifiers: Modifiers::NONE,
            };
            let hook =
                Hook::start(&[(Shortcut::PushToTalk, key)], Arc::new(|_, _| {})).expect("no hook");
            send(0x2F, false);
            wait_held(true);
            send(0x2F, true);
            wait_held(false);
            send(0x30, false); // B
            send(0x30, true);
            std::thread::sleep(Duration::from_millis(100));
            assert!(!yapfast_core::push_to_talk::held());
            drop(hook);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: u32 = 47;
    const M: u32 = 50;
    const LEFT_CTRL: u32 = 29;
    const RIGHT_CTRL: u32 = 97;
    const LEFT_ALT: u32 = 56;
    const LEFT_SHIFT: u32 = 42;

    #[cfg(not(windows))]
    fn key(scancode: u32, modifiers: Modifiers) -> Keybind {
        Keybind {
            scancode,
            name: "K".into(),
            trigger: None,
            modifiers,
        }
    }

    #[cfg(not(windows))]
    fn matcher() -> Matcher {
        Matcher::new(&[
            (Shortcut::PushToTalk, key(V, Modifiers::NONE)),
            (
                Shortcut::ToggleMute,
                key(M, Modifiers::CTRL | Modifiers::ALT),
            ),
        ])
    }

    #[test]
    #[cfg(not(windows))]
    fn a_key_alone_goes_down_and_up_whatever_is_held() {
        let mut matcher = matcher();
        assert_eq!(matcher.key(V, true), [(Shortcut::PushToTalk, true)]);
        assert!(matcher.key(V, true).is_empty(), "a repeat changes nothing");
        assert_eq!(matcher.key(V, false), [(Shortcut::PushToTalk, false)]);
        matcher.key(LEFT_SHIFT, true);
        assert_eq!(matcher.key(V, true), [(Shortcut::PushToTalk, true)]);
    }

    #[test]
    #[cfg(not(windows))]
    fn a_shortcut_with_modifiers_wants_exactly_them() {
        let mut matcher = matcher();
        assert!(matcher.key(M, true).is_empty(), "no modifiers");
        matcher.key(M, false);
        matcher.key(LEFT_CTRL, true);
        assert!(matcher.key(M, true).is_empty(), "one of two");
        matcher.key(M, false);
        matcher.key(LEFT_ALT, true);
        assert_eq!(matcher.key(M, true), [(Shortcut::ToggleMute, true)]);
        assert_eq!(matcher.key(M, false), [(Shortcut::ToggleMute, false)]);
        matcher.key(LEFT_SHIFT, true);
        assert!(matcher.key(M, true).is_empty(), "one too many");
    }

    #[test]
    #[cfg(not(windows))]
    fn letting_go_of_a_modifier_ends_the_shortcut_unless_its_twin_is_held() {
        let mut matcher = matcher();
        matcher.key(LEFT_CTRL, true);
        matcher.key(RIGHT_CTRL, true);
        matcher.key(LEFT_ALT, true);
        assert_eq!(matcher.key(M, true), [(Shortcut::ToggleMute, true)]);
        assert!(
            matcher.key(LEFT_CTRL, false).is_empty(),
            "the right Ctrl is held"
        );
        assert!(matcher.key(RIGHT_CTRL, false).len() == 1);
        assert!(matcher.key(M, false).is_empty(), "already up");
        assert!(matcher.release_all().is_empty());
    }

    #[test]
    #[cfg(not(windows))]
    fn losing_the_keyboard_lets_everything_go() {
        let mut matcher = matcher();
        matcher.key(V, true);
        assert_eq!(matcher.release_all(), [(Shortcut::PushToTalk, false)]);
        assert!(matcher.key(V, false).is_empty());
    }

    #[test]
    fn shortcuts_are_known_by_the_names_the_portal_has() {
        for shortcut in [Shortcut::PushToTalk].into_iter().chain(Shortcut::ACTIONS) {
            assert_eq!(Shortcut::from_id(shortcut.id()), Some(shortcut));
        }
        assert_eq!(Shortcut::from_id("no-such-shortcut"), None);
    }

    #[test]
    fn a_label_puts_the_modifiers_before_the_key() {
        let key = Keybind {
            scancode: 50,
            name: "M".into(),
            trigger: Some("m".into()),
            modifiers: Modifiers::CTRL | Modifiers::SHIFT | Modifiers::SUPER,
        };
        assert_eq!(key.label(), "Ctrl+Shift+Super+M");
        #[cfg(target_os = "linux")]
        assert_eq!(key.portal_trigger().as_deref(), Some("CTRL+SHIFT+LOGO+m"));
    }

    #[test]
    fn a_saved_key_without_modifiers_still_loads() {
        let key: Keybind =
            serde_json::from_str(r#"{"scancode":47,"name":"V","trigger":"v"}"#).unwrap();
        assert_eq!(key.modifiers, Modifiers::NONE);
        let saved = serde_json::to_value(&key).unwrap();
        assert!(saved.get("modifiers").is_none());
        let with: Keybind =
            serde_json::from_str(r#"{"scancode":50,"name":"M","modifiers":3}"#).unwrap();
        assert_eq!(with.modifiers, Modifiers::CTRL | Modifiers::ALT);
    }
}
