//! The push-to-talk key, held in any app: through the GlobalShortcuts portal
//! on Wayland, XInput 2's raw key events on X11, and a low-level keyboard or
//! mouse hook on Windows. Where none of these works, the key works while
//! Yapfast's window has the focus, and settings say so. Each tells
//! [`yapfast_core::push_to_talk::hold`] when the key goes down or comes up.
use std::{
    collections::BTreeSet,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};

use serde::{Deserialize, Serialize};
use slint::winit_030::winit::event::MouseButton;
use slint::winit_030::winit::{
    event::{ElementState, KeyEvent, WindowEvent},
    keyboard::{Key, KeyLocation, NamedKey, PhysicalKey},
    platform::scancode::PhysicalKeyExtScancode,
};
use yapfast_core::runtime::BackgroundRuntime;

/// Mouse buttons share the saved scan-code field, tagged above Windows' 16-bit
/// scan codes so existing keyboard bindings keep their meaning.
const MOUSE_4: u32 = 0xFFFF_0004;
const MOUSE_5: u32 = 0xFFFF_0005;

/// A key recorded in settings.
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
    #[serde(default)]
    pub modifiers: u8,
}

impl Keybind {
    /// The key a key press names, unless it has no scan code.
    pub fn recorded(event: &KeyEvent) -> Option<Self> {
        let scancode = event.physical_key.to_scancode()?;
        Some(
            Self {
                scancode,
                name: name(event),
                trigger: trigger(event),
                modifiers: 0,
            }
            .with_modifiers(),
        )
    }

    /// Mouse 4 or 5 pressed in the window.
    pub fn recorded_mouse(event: &WindowEvent) -> Option<Self> {
        let WindowEvent::MouseInput {
            state: ElementState::Pressed,
            button,
            ..
        } = event
        else {
            return None;
        };
        let (scancode, name) = match button {
            MouseButton::Back => (MOUSE_4, "Mouse 4"),
            MouseButton::Forward => (MOUSE_5, "Mouse 5"),
            _ => return None,
        };
        Some(
            Self {
                scancode,
                name: name.into(),
                trigger: None,
                modifiers: 0,
            }
            .with_modifiers(),
        )
    }

    fn with_modifiers(mut self) -> Self {
        self.modifiers = MODIFIERS.load(Ordering::Relaxed) & !modifier(self.scancode);
        for (flag, label, trigger) in [
            (SUPER, "Win", "LOGO"),
            (ALT, "Alt", "ALT"),
            (SHIFT, "Shift", "SHIFT"),
            (CTRL, "Ctrl", "CTRL"),
        ] {
            if self.modifiers & flag != 0 {
                self.name = format!("{label}+{}", self.name);
                self.trigger = self.trigger.map(|key| format!("{trigger}+{key}"));
            }
        }
        self
    }
}

const CTRL: u8 = 1;
const SHIFT: u8 = 2;
const ALT: u8 = 4;
const SUPER: u8 = 8;
static MODIFIERS: AtomicU8 = AtomicU8::new(0);

pub fn is_modifier(event: &KeyEvent) -> bool {
    event
        .physical_key
        .to_scancode()
        .is_some_and(|key| modifier(key) != 0)
}

fn modifier(key: u32) -> u8 {
    #[cfg(windows)]
    let keys = [
        (CTRL, [0x1D, 0xE01D]),
        (SHIFT, [0x2A, 0x36]),
        (ALT, [0x38, 0xE038]),
        (SUPER, [0xE05B, 0xE05C]),
    ];
    #[cfg(not(windows))]
    let keys = [
        (CTRL, [29, 97]),
        (SHIFT, [42, 54]),
        (ALT, [56, 100]),
        (SUPER, [125, 126]),
    ];
    keys.into_iter()
        .find_map(|(flag, keys)| keys.contains(&key).then_some(flag))
        .unwrap_or(0)
}

type Input = Arc<dyn Fn(u32, bool) -> bool + Send + Sync>;

fn input_handler(key: &Keybind, handler: Handler) -> Input {
    let key = key.clone();
    let state = Mutex::new((BTreeSet::new(), false));
    Arc::new(move |code, down| {
        if code != NONE && code != key.scancode && modifier(code) == 0 {
            return false;
        }
        let mut state = state.lock().unwrap_or_else(|p| p.into_inner());
        if code == NONE {
            state.0.clear();
            if state.1 {
                state.1 = false;
                handler(false);
            }
            return false;
        }
        if down {
            state.0.insert(code);
        } else {
            state.0.remove(&code);
        }
        let modifiers =
            state.0.iter().fold(0, |flags, key| flags | modifier(*key)) & !modifier(key.scancode);
        let held = state.0.contains(&key.scancode) && modifiers == key.modifiers;
        let consumed = code == key.scancode && (held || state.1);
        if state.1 != held {
            state.1 = held;
            handler(held);
        }
        consumed
    })
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

/// Where the key works.
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
    pub fn shortcut_note(&self) -> String {
        match self {
            Self::Asking => "Asking your desktop to enable this shortcut...".into(),
            Self::Everywhere(None) => {
                "Works while another app is in front, including games.".into()
            }
            Self::Everywhere(Some(key)) if key.is_empty() => {
                "Approve or assign this shortcut in your desktop's global shortcut settings.".into()
            }
            Self::Everywhere(Some(key)) => {
                format!("Global shortcut assigned by your desktop: {key}")
            }
            Self::Focused(why) => {
                format!("{why}. This shortcut works only while Yapfast is in front.")
            }
        }
    }
    /// What settings say under the key.
    pub fn note(&self) -> String {
        match self {
            Reach::Asking => "Asking your desktop to let Yapfast use the key in every app...".into(),
            Reach::Everywhere(None) => "Hold the key in any app to talk.".into(),
            Reach::Everywhere(Some(shortcut)) if shortcut.is_empty() => {
                "Your desktop hasn't allowed this shortcut yet. Approve it in your system's global shortcuts settings.".into()
            }
            Reach::Everywhere(Some(shortcut)) => format!(
                "Your desktop gave push to talk the shortcut {shortcut}. Hold it in any app to talk; change it in your system's global shortcuts settings."
            ),
            Reach::Focused(why) => format!(
                "{why}, so push to talk works only while Yapfast's window is in front."
            ),
        }
    }
}

const NONE: u32 = u32::MAX;
#[cfg(test)]
static FOCUSED_KEY: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(NONE);

type Handler = Arc<dyn Fn(bool) + Send + Sync>;
static RECORDING: AtomicBool = AtomicBool::new(false);

pub fn recording(on: bool) {
    RECORDING.store(on, Ordering::Relaxed);
    if on {
        yapfast_core::push_to_talk::hold(false);
        yapfast_core::push_to_talk::push_mute(false);
        for binding in WINDOW_BINDINGS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
        {
            (binding.input)(NONE, false);
        }
    }
}
struct WindowBinding {
    id: &'static str,
    focused: Arc<AtomicBool>,
    input: Input,
}
static WINDOW_BINDINGS: LazyLock<Mutex<Vec<WindowBinding>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

fn event_key(event: &WindowEvent) -> Option<(u32, bool)> {
    match event {
        WindowEvent::KeyboardInput { event, .. } => event
            .physical_key
            .to_scancode()
            .map(|key| (key, event.state == ElementState::Pressed)),
        WindowEvent::MouseInput { state, button, .. } => {
            let key = match button {
                MouseButton::Back => MOUSE_4,
                MouseButton::Forward => MOUSE_5,
                _ => return None,
            };
            Some((key, *state == ElementState::Pressed))
        }
        _ => None,
    }
}

/// Holds the key from the window's own events while no global key works.
/// Losing the focus lets it go, as the key's release won't come.
pub fn window_event(event: &WindowEvent) -> bool {
    if let WindowEvent::ModifiersChanged(modifiers) = event {
        let modifiers = modifiers.state();
        MODIFIERS.store(
            (u8::from(modifiers.control_key()) * CTRL)
                | (u8::from(modifiers.shift_key()) * SHIFT)
                | (u8::from(modifiers.alt_key()) * ALT)
                | (u8::from(modifiers.super_key()) * SUPER),
            Ordering::Relaxed,
        );
    }
    if matches!(event, WindowEvent::Focused(false)) {
        MODIFIERS.store(0, Ordering::Relaxed);
    }
    if RECORDING.load(Ordering::Relaxed) {
        return false;
    }
    let bindings = WINDOW_BINDINGS.lock().unwrap_or_else(|p| p.into_inner());
    let mut consumed = false;
    for binding in bindings.iter() {
        if let Some((key, held)) = event_key(event) {
            consumed |= (binding.input)(key, held);
        } else if matches!(event, WindowEvent::Focused(false))
            && binding.focused.load(Ordering::Relaxed)
        {
            (binding.input)(NONE, false);
        }
    }
    #[cfg(test)]
    {
        let key = FOCUSED_KEY.load(Ordering::Relaxed);
        if key != NONE {
            if let Some((code, held)) = event_key(event) {
                if key == code {
                    yapfast_core::push_to_talk::hold(held);
                }
            } else if matches!(event, WindowEvent::Focused(false)) {
                yapfast_core::push_to_talk::hold(false);
            }
        }
    }
    consumed
}

/// Watches the key in every app it can, until dropped.
pub struct Watcher {
    key: Keybind,
    id: &'static str,
    handler: Handler,
    active: Arc<Mutex<bool>>,
    reach: Arc<Mutex<Reach>>,
    #[cfg(target_os = "linux")]
    _global: Option<linux::Global>,
    #[cfg(windows)]
    _global: Option<windows::Hook>,
}

impl Watcher {
    /// Starts watching `key`; `told` hears where it works whenever that
    /// changes, from any thread.
    pub fn start(
        key: Keybind,
        runtime: &BackgroundRuntime,
        told: impl Fn(Reach) + Send + Sync + 'static,
    ) -> Self {
        Self::start_with(
            key,
            "push-to-talk",
            runtime,
            Arc::new(yapfast_core::push_to_talk::hold),
            told,
        )
    }

    pub fn start_with(
        key: Keybind,
        id: &'static str,
        runtime: &BackgroundRuntime,
        handler: Handler,
        told: impl Fn(Reach) + Send + Sync + 'static,
    ) -> Self {
        let active = Arc::new(Mutex::new(true));
        let enabled = active.clone();
        let reset = handler.clone();
        let callback = handler;
        let handler: Handler = Arc::new(move |held| {
            let enabled = enabled.lock().unwrap_or_else(|p| p.into_inner());
            if *enabled && (!held || !RECORDING.load(Ordering::Relaxed)) {
                callback(held);
            }
        });
        let input = input_handler(&key, handler.clone());
        let focused = Arc::new(AtomicBool::new(false));
        WINDOW_BINDINGS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(WindowBinding {
                id,
                focused: focused.clone(),
                input: input.clone(),
            });
        let reach = Arc::new(Mutex::new(Reach::Asking));
        let shared = reach.clone();
        let fallback = focused;
        let on_error = input.clone();
        let reporting = active.clone();
        let report = Arc::new(move |now: Reach| {
            let focused = matches!(now, Reach::Focused(_));
            if focused {
                on_error(NONE, false);
            }
            let active = reporting.lock().unwrap_or_else(|p| p.into_inner());
            if !*active {
                return;
            }
            fallback.store(focused, Ordering::Relaxed);
            *shared
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = now.clone();
            told(now);
        });
        #[cfg(target_os = "linux")]
        let global =
            linux::Global::start_with(&key, id, runtime, report, handler.clone(), input.clone());
        #[cfg(windows)]
        let global = {
            let _ = runtime;
            windows::Hook::start_with(key.scancode, report, input.clone())
        };
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = runtime;
            report(Reach::Focused(
                "Yapfast can't watch keys in other apps on this system yet",
            ));
        }
        Self {
            key,
            id,
            handler: reset,
            active,
            reach,
            #[cfg(any(target_os = "linux", windows))]
            _global: global,
        }
    }

    pub fn key(&self) -> &Keybind {
        &self.key
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
        *self.active.lock().unwrap_or_else(|p| p.into_inner()) = false;
        WINDOW_BINDINGS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|b| b.id != self.id);
        (self.handler)(false);
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

    use super::{Handler, Input, Keybind, Reach, Report};

    /// A global key on Wayland or X11; stops when dropped.
    pub enum Global {
        Portal { _session: BackgroundTask<()> },
        X11 { _events: X11 },
    }

    impl Global {
        /// Wayland's desktops grab keys through the portal; X11 lets any
        /// client hear every key.
        pub fn start_with(
            key: &Keybind,
            id: &'static str,
            runtime: &BackgroundRuntime,
            report: Report,
            handler: Handler,
            input: Input,
        ) -> Option<Self> {
            let wayland =
                std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
            if wayland {
                report(Reach::Asking);
                let trigger = key.trigger.clone();
                return Some(Global::Portal {
                    _session: runtime.spawn(async move {
                        if let Err(error) = portal_with(trigger, id, &report, handler).await {
                            tracing::info!(
                                error = format!("{error:#}"),
                                "no global push-to-talk key through the portal"
                            );
                            report(Reach::Focused(
                                "Your desktop doesn't offer Yapfast a global shortcut",
                            ));
                        }
                    }),
                });
            }
            match X11::start_with(report.clone(), input) {
                Ok(x11) => {
                    report(Reach::Everywhere(None));
                    Some(Global::X11 { _events: x11 })
                }
                Err(error) => {
                    tracing::info!(
                        error = format!("{error:#}"),
                        "no global push-to-talk key on X11"
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
    #[cfg(test)]
    const SHORTCUT: &str = "push-to-talk";

    /// Binds the key through the GlobalShortcuts portal and holds it as the
    /// desktop says, until the task is dropped, which ends the session with
    /// the connection.
    #[cfg(test)]
    async fn portal(trigger: Option<String>, report: &Report) -> Result<()> {
        portal_with(
            trigger,
            SHORTCUT,
            report,
            Arc::new(yapfast_core::push_to_talk::hold),
        )
        .await
    }

    async fn portal_with(
        trigger: Option<String>,
        id: &'static str,
        report: &Report,
        handler: Handler,
    ) -> Result<()> {
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

        let mut wanted = HashMap::from([(
            "description",
            Value::from(match id {
                "toggle-mute" => "Toggle mute",
                "push-to-mute" => "Push to mute",
                _ => "Push to talk",
            }),
        )]);
        if let Some(trigger) = &trigger {
            wanted.insert("preferred_trigger", Value::from(trigger.as_str()));
        }
        let options = HashMap::from([("handle_token", Value::from(format!("{token}_bind")))]);
        connection
            .call_method(
                Some(PORTAL),
                PATH,
                Some(SHORTCUTS),
                "BindShortcuts",
                &(&session, vec![(id, wanted)], "", options),
            )
            .await?;
        let results = response(&mut signals, &request("bind")).await?;
        report(Reach::Everywhere(Some(described_for(&results, id))));

        while let Some(message) = signals.next().await {
            let message = message?;
            let header = message.header();
            let held =
                match header.member().map(|member| member.as_str()) {
                    Some("Activated") => true,
                    Some("Deactivated") => false,
                    Some("ShortcutsChanged") => {
                        if let Ok((path, shortcuts)) = message
                        .body()
                        .deserialize::<(ObjectPath, Vec<(String, HashMap<String, OwnedValue>)>)>()
                        && path == *session
                    {
                        report(Reach::Everywhere(Some(trigger_description_for(&shortcuts, id))));
                    }
                        continue;
                    }
                    Some("Closed") if header.path().is_some_and(|path| *path == *session) => {
                        bail!("the desktop closed the shortcut's session");
                    }
                    _ => continue,
                };
            if let Ok((path, shortcut_id, ..)) =
                message
                    .body()
                    .deserialize::<(ObjectPath, &str, u64, HashMap<String, OwnedValue>)>()
                && path == *session
                && shortcut_id == id
            {
                handler(held);
            }
        }
        Err(anyhow!("the session bus closed"))
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
                1 => Err(anyhow!("the shortcut was declined")),
                _ => Err(anyhow!("the portal failed ({code})")),
            };
        }
        Err(anyhow!("the session bus closed"))
    }

    /// The shortcut the desktop bound, in its words; empty when it bound
    /// none yet.
    fn described_for(results: &HashMap<String, OwnedValue>, id: &str) -> String {
        results
            .get("shortcuts")
            .and_then(|value| {
                Vec::<(String, HashMap<String, OwnedValue>)>::try_from(value.try_clone().ok()?).ok()
            })
            .map(|shortcuts| trigger_description_for(&shortcuts, id))
            .unwrap_or_default()
    }

    fn trigger_description_for(
        shortcuts: &[(String, HashMap<String, OwnedValue>)],
        wanted: &str,
    ) -> String {
        shortcuts
            .iter()
            .find(|(id, _)| id == wanted)
            .and_then(|(_, details)| {
                match details.get("trigger_description").map(|value| &**value) {
                    Some(Value::Str(text)) => Some(text.to_string()),
                    _ => None,
                }
            })
            .unwrap_or_default()
    }

    /// XInput 2's raw key events from the root window, which come whatever
    /// window has the focus and leave the key to it. A thread waits on them.
    pub struct X11 {
        connection: Arc<RustConnection>,
        window: xproto::Window,
        stop: Arc<AtomicBool>,
    }

    impl X11 {
        #[cfg(test)]
        fn start(scancode: u32, report: Report) -> Result<Self> {
            Self::start_with(
                report,
                Arc::new(move |code, held| {
                    if code == scancode || code == super::NONE {
                        yapfast_core::push_to_talk::hold(held);
                    }
                    false
                }),
            )
        }
        fn start_with(report: Report, handler: Input) -> Result<Self> {
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
                        xinput::XIEventMask::RAW_KEY_PRESS
                            | xinput::XIEventMask::RAW_KEY_RELEASE
                            | xinput::XIEventMask::RAW_BUTTON_PRESS
                            | xinput::XIEventMask::RAW_BUTTON_RELEASE,
                    ],
                }],
            )?;
            connection.flush()?;
            let connection = Arc::new(connection);
            let stop = Arc::new(AtomicBool::new(false));
            let (waiting, stopped) = (connection.clone(), stop.clone());
            // Seed modifiers already held when a binding is installed.
            let keys = connection.query_keymap()?.reply()?.keys;
            for code in 8..256u32 {
                if super::modifier(code - 8) != 0
                    && keys[(code / 8) as usize] & (1 << (code % 8)) != 0
                {
                    handler(code - 8, true);
                }
            }
            std::thread::Builder::new()
                .name("yapfast-talk-key".into())
                .spawn(move || {
                    while !stopped.load(Ordering::Relaxed) {
                        match waiting.wait_for_event() {
                            Ok(Event::XinputRawKeyPress(event))
                                if event.flags & xinput::KeyEventFlags::KEY_REPEAT
                                    == xinput::KeyEventFlags::default() =>
                            {
                                handler(event.detail.saturating_sub(8), true);
                            }
                            Ok(Event::XinputRawKeyRelease(event)) => {
                                handler(event.detail.saturating_sub(8), false);
                            }
                            Ok(Event::XinputRawButtonPress(event)) => {
                                let key = match event.detail {
                                    8 => super::MOUSE_4,
                                    9 => super::MOUSE_5,
                                    _ => continue,
                                };
                                handler(key, true);
                            }
                            Ok(Event::XinputRawButtonRelease(event)) => {
                                let key = match event.detail {
                                    8 => super::MOUSE_4,
                                    9 => super::MOUSE_5,
                                    _ => continue,
                                };
                                handler(key, false);
                            }
                            Ok(_) => {}
                            Err(error) => {
                                tracing::warn!(%error, "lost the X server's keys");
                                handler(super::NONE, false);
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

    #[cfg(test)]
    mod tests {
        use std::time::{Duration, Instant};

        use super::*;

        /// Waits up to two seconds for the key to be `held` or not.
        fn wait_held(held: bool) {
            let until = Instant::now() + Duration::from_secs(2);
            while yapfast_core::push_to_talk::held() != held {
                assert!(Instant::now() < until, "the key wasn't held: {held}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        /// Pressing the key in another client holds it. Needs `DISPLAY` on a
        /// private X server, such as `Xvfb :57`, and `xdotool`, which types
        /// through XTEST as a keyboard would.
        #[test]
        #[ignore = "needs DISPLAY on a private X server and xdotool"]
        fn x11_hears_the_key_whatever_has_the_focus() {
            let x11 = X11::start(47, Arc::new(|_| {})).unwrap(); // evdev's V
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

        /// A stand-in for xdg-desktop-portal's GlobalShortcuts: it answers
        /// each request with Response on the request's path, binds the
        /// shortcut to what was preferred, and then presses it.
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
                let (id, details) = &shortcuts[0];
                let Some(Value::Str(trigger)) = details.get("preferred_trigger").map(|v| &**v)
                else {
                    panic!("no preferred trigger");
                };
                let bound = vec![(
                    id.clone(),
                    HashMap::from([(
                        "trigger_description".to_owned(),
                        Value::from(trigger.to_uppercase()),
                    )]),
                )];
                respond(
                    connection,
                    &request,
                    HashMap::from([("shortcuts", Value::from(bound))]),
                )
                .await;
                let connection = connection.clone();
                let (session, id) = (session.clone(), id.clone());
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

        /// The portal's handshake, and its shortcut going down and up. Needs
        /// a private session bus, as `dbus-run-session` gives.
        #[test]
        #[ignore = "needs a private session bus"]
        fn the_portal_binds_the_key_and_holds_it() {
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
            let report: Report = Arc::new(move |reach| heard.lock().unwrap().push(reach));
            let runtime = BackgroundRuntime::new().unwrap();
            let _task = runtime.spawn(async move { portal(Some("v".into()), &report).await });
            let until = Instant::now() + Duration::from_secs(5);
            while !yapfast_core::push_to_talk::held() {
                assert!(
                    Instant::now() < until,
                    "the shortcut wasn't held: {:?}",
                    told.lock().unwrap()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(*told.lock().unwrap(), [Reach::Everywhere(Some("V".into()))]);
            let session = OwnedObjectPath::try_from(format!("{PATH}/session/test/1")).unwrap();
            zbus::block_on(press(server.inner(), &session, SHORTCUT, "Deactivated"));
            wait_held(false);
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

    use super::{Input, Reach, Report};
    #[cfg(test)]
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    /// The key or tagged mouse button the hook watches.
    #[cfg(test)]
    static KEY: AtomicU32 = AtomicU32::new(super::NONE);
    thread_local! {
        static BINDING: std::cell::RefCell<Option<Input>> = const { std::cell::RefCell::new(None) };
    }
    fn input(key: u32, held: bool) {
        BINDING.with(|binding| {
            if let Some(handler) = &*binding.borrow() {
                handler(key, held);
            } else {
                #[cfg(test)]
                if key == KEY.load(Ordering::Relaxed) {
                    yapfast_core::push_to_talk::hold(held);
                }
            }
        });
    }

    /// A low-level keyboard or mouse hook on a thread of its own, which sees
    /// every input event, releases included, and passes each on. Removed when
    /// dropped.
    pub struct Hook {
        thread: u32,
    }

    impl Hook {
        #[cfg(test)]
        pub fn start(scancode: u32, report: Report) -> Option<Self> {
            Self::start_with(
                scancode,
                report,
                Arc::new(move |code, held| {
                    if code == scancode || code == super::NONE {
                        yapfast_core::push_to_talk::hold(held);
                    }
                    false
                }),
            )
        }
        pub fn start_with(scancode: u32, report: Report, handler: Input) -> Option<Self> {
            let mouse = matches!(scancode, super::MOUSE_4 | super::MOUSE_5);
            let (started, thread) = mpsc::sync_channel(1);
            let spawned = std::thread::Builder::new()
                .name("yapfast-talk-key".into())
                .spawn(move || {
                    BINDING.with(|binding| *binding.borrow_mut() = Some(handler));
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
                        let mouse_hook = if mouse {
                            SetWindowsHookExW(
                                WH_MOUSE_LL,
                                Some(mouse_proc),
                                GetModuleHandleW(ptr::null()),
                                0,
                            )
                        } else {
                            ptr::null_mut()
                        };
                        if hook.is_null() || (mouse && mouse_hook.is_null()) {
                            if !hook.is_null() {
                                UnhookWindowsHookEx(hook);
                            }
                            if !mouse_hook.is_null() {
                                UnhookWindowsHookEx(mouse_hook);
                            }
                            let _ = started.send(None);
                            return;
                        }
                        for (virtual_key, code) in [
                            (0xA2, 0x1D),
                            (0xA3, 0xE01D),
                            (0xA0, 0x2A),
                            (0xA1, 0x36),
                            (0xA4, 0x38),
                            (0xA5, 0xE038),
                            (0x5B, 0xE05B),
                            (0x5C, 0xE05C),
                        ] {
                            if windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(
                                virtual_key,
                            ) < 0
                            {
                                input(code, true);
                            }
                        }
                        let _ = started.send(Some(GetCurrentThreadId()));
                        let mut message: MSG = std::mem::zeroed();
                        while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {}
                        UnhookWindowsHookEx(hook);
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

    /// Notes the push-to-talk key going down or up, and passes every key on.
    unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            // SAFETY: for HC_ACTION, lparam points at the key's details.
            let key = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
            let extended = if key.flags & LLKHF_EXTENDED != 0 {
                0xE000
            } else {
                0
            };
            match wparam as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => input(key.scanCode | extended, true),
                WM_KEYUP | WM_SYSKEYUP => input(key.scanCode | extended, false),
                _ => {}
            }
        }
        // SAFETY: the next hook gets the event as it came.
        unsafe { CallNextHookEx(ptr::null_mut(), code, wparam, lparam) }
    }

    /// Notes the bound side button going down or up, and passes every mouse
    /// event on. XBUTTON1 is Mouse 4 (Back); XBUTTON2 is Mouse 5 (Forward).
    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            // SAFETY: for HC_ACTION, lparam points at the mouse event's details.
            let mouse = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
            let button = match (mouse.mouseData >> 16) as u16 {
                XBUTTON1 => super::MOUSE_4,
                XBUTTON2 => super::MOUSE_5,
                _ => super::NONE,
            };
            match wparam as u32 {
                WM_XBUTTONDOWN => input(button, true),
                WM_XBUTTONUP => input(button, false),
                _ => {}
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
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
            KEYEVENTF_SCANCODE, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
        };

        use super::*;

        /// The hook tests share the microphone's held state and the watched key.
        static TALK_STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

        #[test]
        fn the_window_records_mouse_4_and_5() {
            use slint::winit_030::winit::event::{
                DeviceId, ElementState, MouseButton, WindowEvent,
            };

            use crate::push_to_talk::Keybind;

            let event = |button, state| WindowEvent::MouseInput {
                device_id: DeviceId::dummy(),
                state,
                button,
            };
            for (button, name, scancode) in [
                (MouseButton::Back, "Mouse 4", super::super::MOUSE_4),
                (MouseButton::Forward, "Mouse 5", super::super::MOUSE_5),
            ] {
                let key = Keybind::recorded_mouse(&event(button, ElementState::Pressed))
                    .expect("a side button should be recordable");
                assert_eq!(key.scancode, scancode);
                assert_eq!(key.name, name);
                assert_eq!(key.trigger, None);
            }
            assert!(
                Keybind::recorded_mouse(&event(MouseButton::Left, ElementState::Pressed)).is_none(),
                "ordinary buttons keep their usual meaning"
            );
            assert!(
                Keybind::recorded_mouse(&event(MouseButton::Back, ElementState::Released))
                    .is_none(),
                "only a press records"
            );
        }

        #[test]
        fn the_focused_window_holds_a_mouse_binding() {
            use slint::winit_030::winit::event::{
                DeviceId, ElementState, MouseButton, WindowEvent,
            };

            let _talk = TALK_STATE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mouse = |button, state| WindowEvent::MouseInput {
                device_id: DeviceId::dummy(),
                state,
                button,
            };
            super::super::FOCUSED_KEY.store(super::super::MOUSE_4, Ordering::Relaxed);
            yapfast_core::push_to_talk::hold(false);
            crate::push_to_talk::window_event(&mouse(MouseButton::Forward, ElementState::Pressed));
            assert!(!yapfast_core::push_to_talk::held());
            crate::push_to_talk::window_event(&mouse(MouseButton::Back, ElementState::Pressed));
            assert!(yapfast_core::push_to_talk::held());
            crate::push_to_talk::window_event(&WindowEvent::Focused(false));
            assert!(!yapfast_core::push_to_talk::held());
            crate::push_to_talk::window_event(&mouse(MouseButton::Back, ElementState::Pressed));
            assert!(yapfast_core::push_to_talk::held());
            crate::push_to_talk::window_event(&mouse(MouseButton::Back, ElementState::Released));
            assert!(!yapfast_core::push_to_talk::held());
            super::super::FOCUSED_KEY.store(super::super::NONE, Ordering::Relaxed);
        }

        #[test]
        fn the_mouse_hook_hears_the_bound_side_button_go_down_and_up() {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                MSLLHOOKSTRUCT, WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2,
            };

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
            KEY.store(super::super::MOUSE_4, Ordering::Relaxed);
            event(WM_XBUTTONDOWN, XBUTTON2);
            assert!(!yapfast_core::push_to_talk::held());
            event(WM_XBUTTONDOWN, XBUTTON1);
            assert!(yapfast_core::push_to_talk::held());
            event(WM_XBUTTONUP, XBUTTON2);
            assert!(yapfast_core::push_to_talk::held());
            event(WM_XBUTTONUP, XBUTTON1);
            assert!(!yapfast_core::push_to_talk::held());

            KEY.store(super::super::MOUSE_5, Ordering::Relaxed);
            event(WM_XBUTTONDOWN, XBUTTON2);
            assert!(yapfast_core::push_to_talk::held());
            event(WM_XBUTTONUP, XBUTTON2);
            assert!(!yapfast_core::push_to_talk::held());
            KEY.store(super::super::NONE, Ordering::Relaxed);
        }

        /// The installed mouse hook hears Mouse 4 anywhere, down and up. Run it
        /// alone, on a desktop, or under Wine with an X server.
        #[test]
        #[ignore = "clicks the desktop's side buttons"]
        fn the_installed_mouse_hook_hears_mouse_4_go_down_and_up() {
            let _talk = TALK_STATE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let hook = Hook::start(super::super::MOUSE_4, Arc::new(|_| {})).expect("no hook");
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

        /// The hook hears the key typed anywhere, down and up. Run it alone,
        /// on a desktop, or under Wine with an X server.
        #[test]
        #[ignore = "types into the desktop"]
        fn the_hook_hears_the_key_go_down_and_up() {
            let _talk = TALK_STATE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let hook = Hook::start(0x2F, Arc::new(|_| {})).expect("no hook"); // V
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
