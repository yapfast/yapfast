//! Keybinds, as Discord's: keys held together that do something in any app,
//! such as push to talk or toggling mute. One [`Watcher`] watches them all:
//! through the GlobalShortcuts portal on Wayland, XInput 2's raw key events on
//! X11, and low-level keyboard and mouse hooks on Windows. Where none of these
//! works, they work while Yapfast's window has the focus, and settings say so.
//!
//! Holding keybinds (push to talk, push to mute) set
//! [`yapfast_core::push_to_talk`]'s atomics from the thread that hears the
//! key, so the microphone never waits on the window; the others go to the
//! window's event loop through [`Toggled`].
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub use yapfast_core::keybinds::*;
use yapfast_core::runtime::BackgroundRuntime;

/// Hears toggling keybinds, from any thread; see [`Action`].
pub type Toggled = Arc<dyn Fn(Action) + Send + Sync>;

/// Does what a keybind says as its keys go down or come up.
#[derive(Clone)]
struct Sink {
    toggled: Toggled,
}

impl Sink {
    fn act(&self, action: Action, down: bool) {
        match action {
            Action::Unassigned => {}
            Action::PushToTalk => yapfast_core::push_to_talk::hold(down),
            Action::PushToMute => yapfast_core::push_to_talk::hold_mute(down),
            toggles if down => (self.toggled)(toggles),
            _ => {}
        }
    }
}

/// While settings record a keybind, keybinds do nothing new.
static PAUSED: AtomicBool = AtomicBool::new(false);

/// Keybinds stop and start doing things, as recording starts and stops.
/// Keybinds held already keep on until their keys come up.
pub fn pause(paused: bool) {
    PAUSED.store(paused, Ordering::Relaxed);
}

/// Which keybinds hold, from keys going down and up. It allocates only when
/// made: each event looks through the few keys bound.
struct Matcher {
    /// The scan codes bound, both sides of a modifier in a combination.
    codes: Vec<u32>,
    held: Vec<bool>,
    bindings: Vec<Compiled>,
    /// How many keybinds hold each holding action; see [`Action::hold_slot`].
    holding: [u8; 2],
}

struct Compiled {
    action: Action,
    /// For each key, its index in `codes`, and its other side's for a
    /// modifier in a combination (the same index otherwise).
    keys: Vec<[usize; 2]>,
    active: bool,
    /// Completed by the key going down now.
    fresh: bool,
}

impl Compiled {
    fn uses(&self, index: usize) -> bool {
        self.keys.iter().any(|key| key.contains(&index))
    }

    fn satisfied(&self, held: &[bool]) -> bool {
        self.keys
            .iter()
            .all(|&[one, other]| held[one] || held[other])
    }

    /// Holds every key `other` holds, and more.
    fn covers(&self, other: &Compiled) -> bool {
        self.keys.len() > other.keys.len() && other.keys.iter().all(|key| self.keys.contains(key))
    }
}

type Out<'a> = &'a mut dyn FnMut(Action, bool);

impl Matcher {
    fn new(watched: &[Watched]) -> Self {
        let mut codes = Vec::new();
        let mut index = |code: u32| match codes.iter().position(|&c| c == code) {
            Some(index) => index,
            None => {
                codes.push(code);
                codes.len() - 1
            }
        };
        let bindings = watched
            .iter()
            .map(|watched| {
                let combination = watched.keys.keys().len() > 1;
                let keys = watched
                    .keys
                    .keys()
                    .iter()
                    .map(|key| match modifier(key.scancode).filter(|_| combination) {
                        Some((left, right, _)) => [index(left), index(right)],
                        None => {
                            let one = index(key.scancode);
                            [one, one]
                        }
                    })
                    .collect();
                Compiled {
                    action: watched.action,
                    keys,
                    active: false,
                    fresh: false,
                }
            })
            .collect();
        let held = vec![false; codes.len()];
        Self {
            codes,
            held,
            bindings,
            holding: [0; 2],
        }
    }

    fn position(&self, code: u32) -> Option<usize> {
        self.codes.iter().position(|&c| c == code)
    }

    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "Windows' keyboard hook calls it")
    )]
    fn bound(&self, code: u32) -> bool {
        self.position(code).is_some()
    }

    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "Windows' keyboard hook calls it")
    )]
    fn is_held(&self, code: u32) -> bool {
        self.position(code).is_some_and(|index| self.held[index])
    }

    /// Some keybind holds.
    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "Windows' keyboard hook calls it")
    )]
    fn holding(&self) -> bool {
        self.holding.iter().any(|&count| count > 0)
    }

    /// A key went down or came up. A key going down again before coming up
    /// is a repeat and changes nothing. Among the keybinds a key completes,
    /// one that toggles gives way to one that holds all its keys and more,
    /// as Discord's do.
    fn key(&mut self, code: u32, down: bool, out: Out<'_>) {
        let Some(index) = self.position(code) else {
            return;
        };
        if self.held[index] == down {
            return;
        }
        self.held[index] = down;
        if !down {
            for binding in 0..self.bindings.len() {
                let compiled = &self.bindings[binding];
                if compiled.active && compiled.uses(index) && !compiled.satisfied(&self.held) {
                    self.deactivate(binding, out);
                }
            }
            return;
        }
        if PAUSED.load(Ordering::Relaxed) {
            return;
        }
        let mut any = false;
        for compiled in &mut self.bindings {
            compiled.fresh =
                !compiled.active && compiled.uses(index) && compiled.satisfied(&self.held);
            any |= compiled.fresh;
        }
        if !any {
            return;
        }
        for binding in 0..self.bindings.len() {
            let compiled = &self.bindings[binding];
            if !compiled.fresh {
                continue;
            }
            let shadowed = !compiled.action.holds()
                && self
                    .bindings
                    .iter()
                    .any(|other| other.fresh && other.covers(compiled));
            self.activate(binding, !shadowed, out);
        }
    }

    /// The desktop says a keybind went down or came up, as the portal does,
    /// having matched its keys itself.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn set(&mut self, binding: usize, down: bool, out: Out<'_>) {
        let Some(compiled) = self.bindings.get(binding) else {
            return;
        };
        if down && !compiled.active && !PAUSED.load(Ordering::Relaxed) {
            self.activate(binding, true, out);
        } else if !down && compiled.active {
            self.deactivate(binding, out);
        }
    }

    fn activate(&mut self, binding: usize, fire: bool, out: Out<'_>) {
        let compiled = &mut self.bindings[binding];
        compiled.active = true;
        match compiled.action.hold_slot() {
            Some(slot) => {
                self.holding[slot] += 1;
                if self.holding[slot] == 1 {
                    out(compiled.action, true);
                }
            }
            None if fire => out(compiled.action, true),
            None => {}
        }
    }

    fn deactivate(&mut self, binding: usize, out: Out<'_>) {
        let compiled = &mut self.bindings[binding];
        compiled.active = false;
        if let Some(slot) = compiled.action.hold_slot() {
            self.holding[slot] -= 1;
            if self.holding[slot] == 0 {
                out(compiled.action, false);
            }
        }
    }

    /// Every key comes up, as when the keys stop coming.
    fn release_all(&mut self, out: Out<'_>) {
        self.held.fill(false);
        for binding in 0..self.bindings.len() {
            if self.bindings[binding].active {
                self.deactivate(binding, out);
            }
        }
    }

    /// Lets go of keys held here that `is_down` says are up, as a key-up
    /// that never came; all but `except`.
    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "Windows' keyboard hook calls it")
    )]
    fn resync(&mut self, except: Option<u32>, is_down: impl Fn(u32) -> bool, out: Out<'_>) {
        for index in 0..self.codes.len() {
            let code = self.codes[index];
            if self.held[index] && Some(code) != except && !is_down(code) {
                self.key(code, false, out);
            }
        }
    }
}

type Report = Arc<dyn Fn(Status) + Send + Sync>;

/// Keybinds from the window's own key events, while nothing else hears
/// them. The window hands its keys on from the thread its event filter
/// runs on, so it isn't the window's.
static FOCUSED: Mutex<Option<(Matcher, Sink)>> = Mutex::new(None);

fn focused() -> std::sync::MutexGuard<'static, Option<(Matcher, Sink)>> {
    FOCUSED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The window's own key events stand in for global keys.
static FOCUSED_ON: AtomicBool = AtomicBool::new(false);

/// The window's own key going down or up, by its scan code, or the focus
/// going (none), as the window hands them on: runs keybinds while no
/// global keys work. On Windows the hooks' thread takes them, for as long
/// as the hooks hear nothing with Yapfast focused.
pub fn window_key(scancode: Option<u32>, pressed: bool) {
    #[cfg(windows)]
    if windows::window_key(scancode, pressed) {
        return;
    }
    if !FOCUSED_ON.load(Ordering::Relaxed) {
        return;
    }
    let mut focused = focused();
    let Some((matcher, sink)) = focused.as_mut() else {
        return;
    };
    let out = &mut |action, down| sink.act(action, down);
    match scancode {
        Some(code) => matcher.key(code, pressed, out),
        None => matcher.release_all(out),
    }
}

/// Watches keybinds in every app it can, until dropped.
pub struct Watcher {
    #[cfg(target_os = "linux")]
    global: Option<linux::Global>,
    #[cfg(windows)]
    global: Option<windows::Hook>,
}

impl Watcher {
    /// Starts watching `watched`. `toggled` hears toggling keybinds and
    /// `told` where they work whenever that changes, both from any thread.
    /// The window's own key events stand in where nothing else hears keys
    /// (`window_key`).
    pub fn start(
        watched: Vec<Watched>,
        runtime: &BackgroundRuntime,
        toggled: Toggled,
        told: impl Fn(Status) + Send + Sync + 'static,
    ) -> Self {
        let sink = Sink { toggled };
        *focused() = Some((Matcher::new(&watched), sink.clone()));
        let report: Report = Arc::new(move |now: Status| {
            FOCUSED_ON.store(matches!(now.reach, Reach::Focused(_)), Ordering::Relaxed);
            told(now);
        });
        tracing::info!(
            keybinds = watched.len(),
            keys = watched
                .iter()
                .map(|watched| format!("{:?} {}", watched.action, watched.keys.name()))
                .collect::<Vec<_>>()
                .join(", "),
            "watching keybinds"
        );
        #[cfg(target_os = "linux")]
        let global = linux::Global::start(&watched, runtime, report, sink);
        #[cfg(windows)]
        let global = {
            let _ = runtime;
            windows::Hook::start(Matcher::new(&watched), sink, report)
        };
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (runtime, sink);
            report(Status {
                reach: Reach::Focused("Yapfast can't watch keys in other apps on this system yet"),
                given: HashMap::new(),
            });
        }
        Self {
            #[cfg(any(target_os = "linux", windows))]
            global,
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // What hears keys stops first, so nothing holds a key after this.
        #[cfg(any(target_os = "linux", windows))]
        drop(self.global.take());
        FOCUSED_ON.store(false, Ordering::Relaxed);
        let taken = focused().take();
        if let Some((mut matcher, sink)) = taken {
            matcher.release_all(&mut |action, down| sink.act(action, down));
        }
        yapfast_core::push_to_talk::hold(false);
        yapfast_core::push_to_talk::hold_mute(false);
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

    use super::{Matcher, Reach, Report, Sink, Status, Watched};

    /// Global keybinds on Wayland or X11; stop when dropped.
    pub enum Global {
        Portal { _session: BackgroundTask<()> },
        X11 { _events: X11 },
    }

    /// A shortcut to bind through the portal: its id, what settings call
    /// it, and the keys Yapfast would like for it.
    pub(super) struct Wanted {
        pub id: String,
        pub description: &'static str,
        pub trigger: Option<String>,
    }

    impl Global {
        /// Wayland's desktops grab keys through the portal; X11 lets any
        /// client hear every key.
        pub fn start(
            watched: &[Watched],
            runtime: &BackgroundRuntime,
            report: Report,
            sink: Sink,
        ) -> Option<Self> {
            let wayland =
                std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
            let matcher = Matcher::new(watched);
            if wayland {
                report(Status::default());
                let wanted = watched
                    .iter()
                    .map(|watched| Wanted {
                        id: watched.id.clone(),
                        description: watched.action.label(),
                        trigger: watched.keys.trigger(),
                    })
                    .collect();
                return Some(Global::Portal {
                    _session: runtime.spawn(async move {
                        let mut matcher = matcher;
                        let error = portal(wanted, &mut matcher, &sink, &report).await;
                        matcher.release_all(&mut |action, down| sink.act(action, down));
                        if let Err(error) = error {
                            tracing::info!(
                                error = format!("{error:#}"),
                                "no global keybinds through the portal"
                            );
                            report(Status {
                                reach: Reach::Focused(
                                    "Your desktop doesn't offer Yapfast global shortcuts",
                                ),
                                given: HashMap::new(),
                            });
                        }
                    }),
                });
            }
            match X11::start(matcher, sink, report.clone()) {
                Ok(x11) => {
                    report(Status {
                        reach: Reach::Everywhere,
                        given: HashMap::new(),
                    });
                    Some(Global::X11 { _events: x11 })
                }
                Err(error) => {
                    tracing::info!(error = format!("{error:#}"), "no global keybinds on X11");
                    report(Status {
                        reach: Reach::Focused("Yapfast couldn't watch keys in other apps"),
                        given: HashMap::new(),
                    });
                    None
                }
            }
        }
    }

    pub(super) const PORTAL: &str = "org.freedesktop.portal.Desktop";
    pub(super) const PATH: &str = "/org/freedesktop/portal/desktop";
    pub(super) const SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";

    /// Binds the keybinds through the GlobalShortcuts portal and runs them
    /// as the desktop says, until the task is dropped, which ends the
    /// session with the connection.
    pub(super) async fn portal(
        wanted: Vec<Wanted>,
        matcher: &mut Matcher,
        sink: &Sink,
        report: &Report,
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
                &(crate::APP_ID, HashMap::<&str, Value<'_>>::new()),
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

        let shortcuts: Vec<(&str, HashMap<&str, Value<'_>>)> = wanted
            .iter()
            .map(|wanted| {
                let mut details = HashMap::from([("description", Value::from(wanted.description))]);
                if let Some(trigger) = &wanted.trigger {
                    details.insert("preferred_trigger", Value::from(trigger.as_str()));
                }
                (wanted.id.as_str(), details)
            })
            .collect();
        let options = HashMap::from([("handle_token", Value::from(format!("{token}_bind")))]);
        connection
            .call_method(
                Some(PORTAL),
                PATH,
                Some(SHORTCUTS),
                "BindShortcuts",
                &(&session, shortcuts, "", options),
            )
            .await?;
        let results = response(&mut signals, &request("bind")).await?;
        report(Status {
            reach: Reach::Desktop,
            given: described(&results),
        });

        while let Some(message) = signals.next().await {
            let message = message?;
            let header = message.header();
            let held = match header.member().map(|member| member.as_str()) {
                Some("Activated") => true,
                Some("Deactivated") => false,
                Some("ShortcutsChanged") => {
                    if let Ok((path, shortcuts)) = message
                        .body()
                        .deserialize::<(ObjectPath<'_>, Vec<(String, HashMap<String, OwnedValue>)>)>()
                        && path == *session
                    {
                        report(Status {
                            reach: Reach::Desktop,
                            given: given(&shortcuts),
                        });
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
                    .deserialize::<(ObjectPath<'_>, &str, u64, HashMap<String, OwnedValue>)>()
                && path == *session
                && let Some(binding) = wanted.iter().position(|wanted| wanted.id == id)
            {
                matcher.set(binding, held, &mut |action, down| sink.act(action, down));
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
                1 => Err(anyhow!("the shortcuts were declined")),
                _ => Err(anyhow!("the portal failed ({code})")),
            };
        }
        Err(anyhow!("the session bus closed"))
    }

    /// The shortcuts the desktop bound, in its words.
    fn described(results: &HashMap<String, OwnedValue>) -> HashMap<String, String> {
        results
            .get("shortcuts")
            .and_then(|value| {
                Vec::<(String, HashMap<String, OwnedValue>)>::try_from(value.try_clone().ok()?).ok()
            })
            .map(|shortcuts| given(&shortcuts))
            .unwrap_or_default()
    }

    /// Each shortcut's trigger, in the desktop's words; empty for one it
    /// bound to no keys yet.
    fn given(shortcuts: &[(String, HashMap<String, OwnedValue>)]) -> HashMap<String, String> {
        shortcuts
            .iter()
            .map(|(id, details)| {
                let text = match details.get("trigger_description").map(|value| &**value) {
                    Some(Value::Str(text)) => text.to_string(),
                    _ => String::new(),
                };
                (id.clone(), text)
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
        pub(super) fn start(mut matcher: Matcher, sink: Sink, report: Report) -> Result<Self> {
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
            std::thread::Builder::new()
                .name("yapfast-keybinds".into())
                .spawn(move || {
                    let out = &mut |action, down| sink.act(action, down);
                    while !stopped.load(Ordering::Relaxed) {
                        // X11 keycodes are evdev's plus 8.
                        match waiting.wait_for_event() {
                            Ok(Event::XinputRawKeyPress(event)) => {
                                matcher.key(event.detail.saturating_sub(8), true, out);
                            }
                            Ok(Event::XinputRawKeyRelease(event)) => {
                                matcher.key(event.detail.saturating_sub(8), false, out);
                            }
                            Ok(_) => {}
                            Err(error) => {
                                tracing::warn!(%error, "lost the X server's keys");
                                report(Status {
                                    reach: Reach::Focused("Yapfast lost the X server's keys"),
                                    given: HashMap::new(),
                                });
                                break;
                            }
                        }
                    }
                    matcher.release_all(out);
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
        use std::{
            sync::Mutex,
            time::{Duration, Instant},
        };

        use super::{
            super::{Action, Key, Shortcut, Toggled},
            *,
        };

        /// Waits up to two seconds for push to talk to be `held` or not.
        fn wait_held(held: bool) {
            let until = Instant::now() + Duration::from_secs(2);
            while yapfast_core::push_to_talk::held() != held {
                assert!(Instant::now() < until, "the key wasn't held: {held}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn watched(id: &str, action: Action, keys: &[(u32, &str)]) -> Watched {
            Watched {
                id: id.into(),
                action,
                keys: Shortcut::new(
                    keys.iter()
                        .map(|&(scancode, trigger)| Key {
                            scancode,
                            name: trigger.to_uppercase(),
                            trigger: Some(trigger.into()),
                        })
                        .collect(),
                )
                .unwrap(),
            }
        }

        /// A sink that keeps the toggles it hears.
        fn sink() -> (Sink, Arc<Mutex<Vec<Action>>>) {
            let toggles = Arc::new(Mutex::new(Vec::new()));
            let heard = toggles.clone();
            let toggled: Toggled = Arc::new(move |action| heard.lock().unwrap().push(action));
            (Sink { toggled }, toggles)
        }

        /// Pressing the keys in another client runs the keybinds. Needs
        /// `DISPLAY` on a private X server, such as `Xvfb :57`, and `xdotool`,
        /// which types through XTEST as a keyboard would.
        #[test]
        #[ignore = "needs DISPLAY on a private X server and xdotool"]
        fn x11_hears_the_keys_whatever_has_the_focus() {
            let (sink, toggles) = sink();
            let matcher = Matcher::new(&[
                watched("talk", Action::PushToTalk, &[(47, "v")]), // evdev's V
                watched("mute", Action::ToggleMute, &[(29, "Control_L"), (50, "m")]),
            ]);
            let x11 = X11::start(matcher, sink, Arc::new(|_| {})).unwrap();
            let xdotool = |args: &[&str]| {
                let status = std::process::Command::new("xdotool")
                    .args(args)
                    .status()
                    .unwrap();
                assert!(status.success());
            };
            xdotool(&["keydown", "v"]);
            wait_held(true);
            xdotool(&["keyup", "v"]);
            wait_held(false);
            // Another key leaves it alone.
            xdotool(&["key", "b"]);
            std::thread::sleep(Duration::from_millis(100));
            assert!(!yapfast_core::push_to_talk::held());
            // The right Ctrl holds the combination as well as the left.
            xdotool(&["key", "Control_R+m"]);
            xdotool(&["key", "m"]);
            let until = Instant::now() + Duration::from_secs(2);
            while toggles.lock().unwrap().is_empty() {
                assert!(Instant::now() < until, "Ctrl+M didn't toggle");
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(*toggles.lock().unwrap(), [Action::ToggleMute], "M alone");
            drop(x11);
        }

        /// A stand-in for xdg-desktop-portal's GlobalShortcuts: it answers
        /// each request with Response on the request's path, binds each
        /// shortcut to what was preferred, and then presses the first.
        struct FakePortal {
            bound: Arc<Mutex<Vec<(String, String)>>>,
        }

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
                let mut bound = Vec::new();
                for (id, details) in &shortcuts {
                    let Some(Value::Str(trigger)) = details.get("preferred_trigger").map(|v| &**v)
                    else {
                        panic!("no preferred trigger for {id}");
                    };
                    self.bound
                        .lock()
                        .unwrap()
                        .push((id.clone(), trigger.to_string()));
                    bound.push((
                        id.clone(),
                        HashMap::from([(
                            "trigger_description".to_owned(),
                            Value::from(trigger.to_uppercase()),
                        )]),
                    ));
                }
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
            let options = HashMap::<&str, Value<'_>>::new();
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

        /// The portal's handshake, each keybind's shortcut, and push to talk
        /// and a toggle going down and up. Needs a private session bus, as
        /// `dbus-run-session` gives.
        #[test]
        #[ignore = "needs a private session bus"]
        fn the_portal_binds_the_keybinds_and_runs_them() {
            let bound = Arc::new(Mutex::new(Vec::new()));
            let server = zbus::blocking::connection::Builder::session()
                .unwrap()
                .name(PORTAL)
                .unwrap()
                .serve_at(
                    PATH,
                    FakePortal {
                        bound: bound.clone(),
                    },
                )
                .unwrap()
                .build()
                .unwrap();
            let keybinds = [
                watched(
                    super::super::TALK_SHORTCUT,
                    Action::PushToTalk,
                    &[(47, "v")],
                ),
                watched(
                    "keybind-3",
                    Action::ToggleDeafen,
                    &[(29, "Control_L"), (42, "Shift_L"), (32, "d")],
                ),
            ];
            let told = Arc::new(Mutex::new(Vec::new()));
            let heard = told.clone();
            let report: Report = Arc::new(move |status| heard.lock().unwrap().push(status));
            let (sink, toggles) = sink();
            let wanted = keybinds
                .iter()
                .map(|watched| Wanted {
                    id: watched.id.clone(),
                    description: watched.action.label(),
                    trigger: watched.keys.trigger(),
                })
                .collect();
            let mut matcher = Matcher::new(&keybinds);
            let runtime = BackgroundRuntime::new().unwrap();
            let _task =
                runtime.spawn(async move { portal(wanted, &mut matcher, &sink, &report).await });
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
                *bound.lock().unwrap(),
                [
                    ("push-to-talk".to_owned(), "v".to_owned()),
                    ("keybind-3".to_owned(), "CTRL+SHIFT+d".to_owned())
                ]
            );
            let status = told.lock().unwrap().last().cloned().unwrap();
            assert_eq!(status.reach, Reach::Desktop);
            assert!(status.talk_note().contains("shortcut V."));
            assert_eq!(
                status.given_note("keybind-3").as_deref(),
                Some("Your desktop gave it the shortcut CTRL+SHIFT+D.")
            );
            let session = OwnedObjectPath::try_from(format!("{PATH}/session/test/1")).unwrap();
            zbus::block_on(press(
                server.inner(),
                &session,
                super::super::TALK_SHORTCUT,
                "Deactivated",
            ));
            wait_held(false);
            zbus::block_on(press(server.inner(), &session, "keybind-3", "Activated"));
            zbus::block_on(press(server.inner(), &session, "keybind-3", "Deactivated"));
            let until = Instant::now() + Duration::from_secs(2);
            while toggles.lock().unwrap().is_empty() {
                assert!(Instant::now() < until, "the toggle wasn't heard");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(*toggles.lock().unwrap(), [Action::ToggleDeafen]);
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::{
        cell::RefCell,
        collections::HashMap,
        ptr,
        sync::{
            atomic::{AtomicBool, AtomicU32, Ordering},
            mpsc,
        },
    };

    use windows_sys::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{GetCurrentProcessId, GetCurrentThreadId},
        },
        UI::{
            Input::{
                GetRegisteredRawInputDevices,
                KeyboardAndMouse::{
                    GetAsyncKeyState, GetKeyboardLayout, MAPVK_VK_TO_VSC_EX, MAPVK_VSC_TO_VK_EX,
                    MapVirtualKeyExW, MapVirtualKeyW, VK_XBUTTON1, VK_XBUTTON2,
                },
                RAWINPUTDEVICE, RIDEV_REMOVE, RegisterRawInputDevices,
            },
            WindowsAndMessaging::{
                CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
                HC_ACTION, KBDLLHOOKSTRUCT, KillTimer, LLKHF_EXTENDED, LLKHF_INJECTED, MSG,
                MSLLHOOKSTRUCT, PostThreadMessageW, SetTimer, SetWindowsHookExW,
                UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN, WM_KEYUP,
                WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WM_XBUTTONDOWN, WM_XBUTTONUP,
                XBUTTON1, XBUTTON2,
            },
        },
    };
    // The window's own winit, the same version.
    use winit::{keyboard::PhysicalKey, platform::scancode::PhysicalKeyExtScancode};

    use super::{MOUSE_4, MOUSE_5, Matcher, Reach, Report, Sink, Status};

    /// How often a held keybind checks that its keys are still down, in ms.
    const CHECK_EVERY: u32 = 250;
    /// Key events logged at debug level, to tell why a key isn't heard.
    const LOGGED: u32 = 32;
    /// The window's own key, posted to the hook thread: its scan code, and
    /// whether it went down.
    const WINDOW_KEY: u32 = WM_APP + 1;
    /// The usage page of keyboards and mice, and the usage of each.
    const GENERIC: u16 = 0x01;
    const MOUSE: u16 = 0x02;
    const KEYBOARD: u16 = 0x06;

    /// The hook thread, to post the window's keys to; 0 while there's none.
    static THREAD: AtomicU32 = AtomicU32::new(0);
    /// The keyboard hook, and the mouse hook, heard a bound key while
    /// Yapfast's window had the focus, so the window's own keys aren't
    /// needed; see [`window_event`]. It's so of every hook after, as when
    /// a keybind that changes what's watched starts new ones mid-press.
    static HEARS_FOCUSED: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

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
        /// The window's own keys stood in, which is logged once.
        stood_in: bool,
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
            drop_raw_input();
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
                        stood_in: false,
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
                            match message.message {
                                WM_TIMER => check_held(),
                                WINDOW_KEY => {
                                    window_event(message.wParam as u32, message.lParam != 0);
                                }
                                _ => {}
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
                    THREAD.store(thread, Ordering::Relaxed);
                    tracing::info!(keys, buttons, "keybind hooks installed");
                    report(Status {
                        reach: Reach::Everywhere,
                        given: HashMap::new(),
                    });
                    Some(Self { thread })
                }
                None => {
                    tracing::warn!("Windows didn't install the keybind hooks");
                    report(Status {
                        reach: Reach::Focused(
                            "Windows didn't let Yapfast watch keys in other apps",
                        ),
                        given: HashMap::new(),
                    });
                    None
                }
            }
        }
    }

    impl Drop for Hook {
        fn drop(&mut self) {
            let _ = THREAD.compare_exchange(self.thread, 0, Ordering::Relaxed, Ordering::Relaxed);
            // SAFETY: posting to a thread id is harmless if it has ended.
            unsafe {
                PostThreadMessageW(self.thread, WM_QUIT, 0, 0);
            }
        }
    }

    /// Lets go of the raw keyboard and mouse input winit registers the
    /// process for as its event loop starts, for device events nothing in
    /// Yapfast reads. While a window of a process registered so has the
    /// focus, Windows calls none of the process's low-level hooks, so
    /// keybinds worked in every app but Yapfast itself (issue #249).
    fn drop_raw_input() {
        let mut registered = [RAWINPUTDEVICE::default(); 16];
        let mut count = registered.len() as u32;
        let size = size_of::<RAWINPUTDEVICE>() as u32;
        // SAFETY: `registered` holds `count` devices of `size` bytes each.
        let found =
            unsafe { GetRegisteredRawInputDevices(registered.as_mut_ptr(), &mut count, size) };
        // More than fit, or an error: both names are let go unseen.
        let all = [MOUSE, KEYBOARD];
        let usages: Vec<u16> = match found {
            u32::MAX => all.to_vec(),
            found => registered[..found as usize]
                .iter()
                .filter(|device| device.usUsagePage == GENERIC && all.contains(&device.usUsage))
                .map(|device| device.usUsage)
                .collect(),
        };
        if usages.is_empty() {
            return;
        }
        let devices: Vec<RAWINPUTDEVICE> = usages
            .iter()
            .map(|&usage| RAWINPUTDEVICE {
                usUsagePage: GENERIC,
                usUsage: usage,
                dwFlags: RIDEV_REMOVE,
                hwndTarget: ptr::null_mut(),
            })
            .collect();
        // SAFETY: `devices` holds that many devices of `size` bytes each.
        let dropped =
            unsafe { RegisterRawInputDevices(devices.as_ptr(), devices.len() as u32, size) } != 0;
        tracing::info!(
            ?usages,
            dropped,
            "raw input let go, so the keybind hooks hear keys with Yapfast focused"
        );
    }

    /// The window's own key goes to the hook thread, if there is one: true
    /// if the hooks run keybinds, whether or not they need this key.
    pub(super) fn window_key(scancode: Option<u32>, pressed: bool) -> bool {
        let thread = THREAD.load(Ordering::Relaxed);
        if thread == 0 {
            return false;
        }
        // The focus going needs no word: the hooks hear keys in other apps,
        // and a held keybind checks its keys by itself.
        if let Some(code) = scancode
            && !HEARS_FOCUSED[usize::from(matches!(code, MOUSE_4 | MOUSE_5))]
                .load(Ordering::Relaxed)
        {
            // SAFETY: posting to a thread id is harmless if it has ended.
            unsafe {
                PostThreadMessageW(thread, WINDOW_KEY, code as WPARAM, LPARAM::from(pressed));
            }
        }
        true
    }

    /// Whether a window of Yapfast's has the focus.
    fn focused_here() -> bool {
        let mut process = 0;
        // SAFETY: these only read which window is in front and whose it is.
        unsafe {
            GetWindowThreadProcessId(GetForegroundWindow(), &mut process);
            process == GetCurrentProcessId()
        }
    }

    /// A hook heard a bound key, the keyboard's or the mouse's: with
    /// Yapfast focused, that hook needs no help from the window's keys.
    fn heard(mouse: bool) {
        let hears = &HEARS_FOCUSED[usize::from(mouse)];
        if !hears.load(Ordering::Relaxed) && focused_here() {
            hears.store(true, Ordering::Relaxed);
            tracing::debug!(mouse, "the keybind hook hears keys with Yapfast focused");
        }
    }

    /// The window's own key went down or came up, on the hook thread. It
    /// counts only until the hook has heard a key with Yapfast focused,
    /// which Windows hands the hook first: so each key counts once, and
    /// keybinds work in Yapfast's window even where its hook hears nothing
    /// there.
    fn window_event(code: u32, down: bool) {
        let mouse = matches!(code, MOUSE_4 | MOUSE_5);
        if HEARS_FOCUSED[usize::from(mouse)].load(Ordering::Relaxed) {
            return;
        }
        let stands_in = HOOKED.with_borrow_mut(|hooked| {
            let Some(hooked) = hooked.as_mut() else {
                return false;
            };
            if !hooked.matcher.bound(code) {
                return false;
            }
            if !hooked.stood_in {
                hooked.stood_in = true;
                tracing::info!(
                    "the keybind hook heard nothing with Yapfast focused; the window's keys stand in"
                );
            }
            true
        });
        if !stands_in {
            return;
        }
        // The virtual key Windows keeps the state of, as the hook's has.
        let virtual_key = (!mouse)
            // SAFETY: MapVirtualKeyW only reads its arguments.
            .then(|| unsafe { MapVirtualKeyW(code, MAPVK_VSC_TO_VK_EX) })
            .filter(|&virtual_key| virtual_key != 0);
        event(code, down, virtual_key);
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
            if hooked.logged >= LOGGED || !suspect || !tracing::enabled!(tracing::Level::DEBUG) {
                return;
            }
            hooked.logged += 1;
            tracing::debug!(
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
    pub(super) unsafe extern "system" fn proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
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
                    heard(false);
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
                heard(true);
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
            stood_in: false,
        }));
    }

    #[cfg(test)]
    mod tests {
        use std::{
            sync::{Arc, Mutex},
            time::{Duration, Instant},
        };

        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
            KEYEVENTF_SCANCODE, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
        };

        use super::{
            super::{Action, Key, Shortcut, Toggled, Watched},
            *,
        };

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

        /// The window's own keys run keybinds on the hook's thread until
        /// the hook has heard a key with Yapfast focused; issue #249.
        #[test]
        fn the_windows_own_keys_stand_in_until_the_hook_hears_them_focused() {
            let _talk = TALK_STATE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let (sink, toggles) = sink();
            hooked_here(
                Matcher::new(&[
                    watched(Action::PushToTalk, &[0x2F]), // V
                    watched(Action::ToggleMute, &[0x32]), // M
                    watched(Action::PushToMute, &[MOUSE_4]),
                ]),
                sink,
            );
            yapfast_core::push_to_talk::hold(false);
            window_event(0x30, true); // B, bound to nothing
            assert!(!yapfast_core::push_to_talk::held());
            window_event(0x2F, true);
            assert!(yapfast_core::push_to_talk::held());
            window_event(0x2F, false);
            assert!(!yapfast_core::push_to_talk::held());
            window_event(0x32, true);
            window_event(0x32, false);
            assert_eq!(*toggles.lock().unwrap(), [Action::ToggleMute]);
            // The hook hears a key with no window of Yapfast's in front,
            // as in a test: the window's keys would still count.
            key(WM_KEYDOWN, 0x32, 0x4D, 0);
            key(WM_KEYUP, 0x32, 0x4D, 0);
            assert_eq!(toggles.lock().unwrap().len(), 2);
            let hears = |mouse: usize| HEARS_FOCUSED[mouse].load(Ordering::Relaxed);
            assert!(!hears(0) && !hears(1));
            // Once it heard one with Yapfast in front, they don't.
            HEARS_FOCUSED[0].store(true, Ordering::Relaxed);
            window_event(0x2F, true);
            assert!(!yapfast_core::push_to_talk::held(), "the hook hears keys");
            window_event(0x32, true);
            window_event(0x32, false);
            assert_eq!(toggles.lock().unwrap().len(), 2);
            // The mouse hook is another: its buttons still stand in.
            window_event(MOUSE_4, true);
            HOOKED.with_borrow(|hooked| {
                assert!(hooked.as_ref().unwrap().matcher.is_held(MOUSE_4));
            });
            window_event(MOUSE_4, false);
            HEARS_FOCUSED[1].store(true, Ordering::Relaxed);
            window_event(MOUSE_4, true);
            HOOKED.with_borrow(|hooked| {
                assert!(!hooked.as_ref().unwrap().matcher.is_held(MOUSE_4));
            });
            yapfast_core::push_to_talk::hold_mute(false);
            for hears in &HEARS_FOCUSED {
                hears.store(false, Ordering::Relaxed);
            }
            HOOKED.take();
        }

        /// Raw input registered as winit's is let go, from another thread
        /// too: it's the process's.
        #[test]
        fn raw_input_registered_as_winits_is_let_go() {
            fn registered() -> Vec<u16> {
                let mut devices = [RAWINPUTDEVICE::default(); 16];
                let mut count = devices.len() as u32;
                // SAFETY: `devices` holds `count` devices of that size.
                let found = unsafe {
                    GetRegisteredRawInputDevices(
                        devices.as_mut_ptr(),
                        &mut count,
                        size_of::<RAWINPUTDEVICE>() as u32,
                    )
                };
                assert_ne!(found, u32::MAX);
                devices[..found as usize]
                    .iter()
                    .filter(|device| device.usUsagePage == GENERIC)
                    .map(|device| device.usUsage)
                    .collect()
            }
            let devices = [MOUSE, KEYBOARD].map(|usage| RAWINPUTDEVICE {
                usUsagePage: GENERIC,
                usUsage: usage,
                dwFlags: 0,
                hwndTarget: ptr::null_mut(),
            });
            // SAFETY: two devices of the size Windows expects.
            let done = unsafe {
                RegisterRawInputDevices(devices.as_ptr(), 2, size_of::<RAWINPUTDEVICE>() as u32)
            };
            assert_ne!(done, 0);
            let mut before = registered();
            before.sort_unstable();
            assert_eq!(before, [MOUSE, KEYBOARD]);
            std::thread::spawn(drop_raw_input).join().unwrap();
            assert!(registered().is_empty());
            // Nothing registered is nothing to do.
            drop_raw_input();
            assert!(registered().is_empty());
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
            // The window's own keys reach the same thread, as when Windows
            // keeps a focused Yapfast's keys from its hook.
            assert!(window_key(Some(0x2F), true));
            wait_held(true);
            assert!(window_key(None, false));
            assert!(window_key(Some(0x2F), false));
            wait_held(false);
            drop(hook);
            assert!(!window_key(Some(0x2F), true), "no hook");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(scancode: u32, name: &str) -> Key {
        Key {
            scancode,
            name: name.into(),
            trigger: Some(name.to_lowercase()),
        }
    }

    fn watched(action: Action, keys: &[u32]) -> Watched {
        Watched {
            id: String::new(),
            action,
            keys: Shortcut::new(
                keys.iter()
                    .map(|&code| key(code, &format!("{code}")))
                    .collect(),
            )
            .unwrap(),
        }
    }

    /// Runs `events` (code, down) through keybinds and returns what they did.
    fn run(watched: &[Watched], events: &[(u32, bool)]) -> Vec<(Action, bool)> {
        let mut matcher = Matcher::new(watched);
        let mut done = Vec::new();
        for &(code, down) in events {
            matcher.key(code, down, &mut |action, down| done.push((action, down)));
        }
        done
    }

    const V: u32 = 47;
    const M: u32 = 50;
    const D: u32 = 32;
    #[cfg(target_os = "linux")]
    const LEFT_CTRL: u32 = 29;
    #[cfg(target_os = "linux")]
    const RIGHT_CTRL: u32 = 97;
    #[cfg(target_os = "linux")]
    const SHIFT: u32 = 42;
    #[cfg(windows)]
    const LEFT_CTRL: u32 = 0x1D;
    #[cfg(windows)]
    const RIGHT_CTRL: u32 = 0xE01D;
    #[cfg(windows)]
    const SHIFT: u32 = 0x2A;

    #[test]
    fn a_key_holds_while_down_and_repeats_change_nothing() {
        let done = run(
            &[watched(Action::PushToTalk, &[V])],
            &[
                (V, true),
                (V, true),
                (M, true),
                (M, false),
                (V, false),
                (V, false),
            ],
        );
        assert_eq!(
            done,
            [(Action::PushToTalk, true), (Action::PushToTalk, false)]
        );
    }

    #[test]
    fn a_toggle_happens_once_a_press() {
        let done = run(
            &[watched(Action::ToggleMute, &[M])],
            &[(M, true), (M, true), (M, false), (M, true)],
        );
        assert_eq!(
            done,
            [(Action::ToggleMute, true), (Action::ToggleMute, true)]
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn a_combination_needs_its_keys_held_together_and_either_ctrl() {
        let keybinds = [watched(Action::ToggleMute, &[LEFT_CTRL, M])];
        assert_eq!(run(&keybinds, &[(M, true), (M, false)]), []);
        assert_eq!(
            run(&keybinds, &[(M, true), (LEFT_CTRL, true)]),
            [(Action::ToggleMute, true)],
            "in either order"
        );
        assert_eq!(
            run(&keybinds, &[(RIGHT_CTRL, true), (M, true)]),
            [(Action::ToggleMute, true)]
        );
        // A key alone is its side only.
        let right = [watched(Action::PushToTalk, &[RIGHT_CTRL])];
        assert_eq!(run(&right, &[(LEFT_CTRL, true)]), []);
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn push_to_talk_holds_while_other_keys_go_down_too() {
        // Talking while moving in a game: other keys don't stop it, and a
        // combination holds while its keys do.
        let done = run(
            &[watched(Action::PushToTalk, &[LEFT_CTRL, V])],
            &[
                (LEFT_CTRL, true),
                (V, true),
                (M, true),
                (D, true),
                (M, false),
                (LEFT_CTRL, false),
                (V, false),
            ],
        );
        assert_eq!(
            done,
            [(Action::PushToTalk, true), (Action::PushToTalk, false)]
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn a_shorter_toggle_gives_way_to_a_longer_one_its_keys_complete() {
        let keybinds = [
            watched(Action::ToggleMute, &[LEFT_CTRL, D]),
            watched(Action::ToggleDeafen, &[LEFT_CTRL, SHIFT, D]),
            watched(Action::PushToTalk, &[D]),
        ];
        assert_eq!(
            run(&keybinds, &[(LEFT_CTRL, true), (SHIFT, true), (D, true)]),
            [(Action::ToggleDeafen, true), (Action::PushToTalk, true)],
            "a holding keybind still holds"
        );
        assert_eq!(
            run(&keybinds, &[(LEFT_CTRL, true), (D, true)]),
            [(Action::ToggleMute, true), (Action::PushToTalk, true)]
        );
    }

    #[test]
    fn two_push_to_talk_keybinds_hold_until_both_are_up() {
        let done = run(
            &[
                watched(Action::PushToTalk, &[V]),
                watched(Action::PushToTalk, &[M]),
            ],
            &[(V, true), (M, true), (V, false), (M, false)],
        );
        assert_eq!(
            done,
            [(Action::PushToTalk, true), (Action::PushToTalk, false)]
        );
    }

    #[test]
    fn letting_everything_go_ends_what_holds() {
        let mut matcher = Matcher::new(&[
            watched(Action::PushToTalk, &[V]),
            watched(Action::PushToMute, &[M]),
        ]);
        let mut done = Vec::new();
        let mut out = |action, down| done.push((action, down));
        matcher.key(V, true, &mut out);
        matcher.key(M, true, &mut out);
        matcher.release_all(&mut out);
        matcher.key(V, true, &mut out);
        assert_eq!(
            done,
            [
                (Action::PushToTalk, true),
                (Action::PushToMute, true),
                (Action::PushToTalk, false),
                (Action::PushToMute, false),
                (Action::PushToTalk, true),
            ]
        );
    }

    #[test]
    fn a_key_up_that_never_came_is_found() {
        let mut matcher = Matcher::new(&[watched(Action::PushToTalk, &[V])]);
        let mut done = Vec::new();
        let mut out = |action, down| done.push((action, down));
        matcher.key(V, true, &mut out);
        matcher.resync(None, |_| true, &mut out);
        matcher.resync(Some(V), |_| false, &mut out);
        assert!(matcher.holding());
        matcher.resync(None, |_| false, &mut out);
        assert!(!matcher.holding());
        assert_eq!(
            done,
            [(Action::PushToTalk, true), (Action::PushToTalk, false)]
        );
    }

    #[test]
    fn the_desktop_runs_keybinds_by_name() {
        let mut matcher = Matcher::new(&[
            watched(Action::PushToTalk, &[V]),
            watched(Action::ToggleDeafen, &[D]),
        ]);
        let mut done = Vec::new();
        let mut out = |action, down| done.push((action, down));
        matcher.set(0, true, &mut out);
        matcher.set(0, true, &mut out);
        matcher.set(1, true, &mut out);
        matcher.set(1, false, &mut out);
        matcher.set(0, false, &mut out);
        matcher.set(7, true, &mut out);
        assert_eq!(
            done,
            [
                (Action::PushToTalk, true),
                (Action::ToggleDeafen, true),
                (Action::PushToTalk, false),
            ]
        );
    }

    #[test]
    fn the_windows_own_keys_run_keybinds_only_while_no_global_key_works() {
        let toggles = Arc::new(Mutex::new(Vec::new()));
        let heard = toggles.clone();
        let toggled: Toggled = Arc::new(move |action| heard.lock().unwrap().push(action));
        *focused() = Some((
            Matcher::new(&[watched(Action::ToggleMute, &[M])]),
            Sink { toggled },
        ));
        window_key(Some(M), true);
        window_key(Some(M), false);
        assert!(toggles.lock().unwrap().is_empty(), "a global key works");
        FOCUSED_ON.store(true, Ordering::Relaxed);
        window_key(Some(V), true);
        window_key(Some(V), false);
        assert!(toggles.lock().unwrap().is_empty(), "another key");
        window_key(Some(M), true);
        window_key(None, false);
        window_key(Some(M), true);
        assert_eq!(
            *toggles.lock().unwrap(),
            [Action::ToggleMute, Action::ToggleMute],
            "the focus going let the key go"
        );
        FOCUSED_ON.store(false, Ordering::Relaxed);
        focused().take();
    }
}
