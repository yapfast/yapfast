//! Keybinds, as Discord's: keys held together that do something in any app,
//! such as push to talk or toggling mute. One [`Watcher`] watches them all:
//! through the GlobalShortcuts portal on Wayland, XInput 2's raw key events on
//! X11, and low-level keyboard and mouse hooks on Windows. Where none of these
//! works, they work while the window has the focus, from the key events each
//! client hands [`window_key`], and settings say so. Both clients use it; each
//! turns its toolkit's key events into scan codes, as winit gives them.
//!
//! Holding keybinds (push to talk, push to mute) set
//! [`yapfast_core::push_to_talk`]'s atomics from the thread that hears the
//! key, so the microphone never waits on the window; the others go to the
//! window's event loop through [`Toggled`]. Nothing here polls: a thread
//! blocks on the X server or in the hooks' message loop, or the portal's task
//! waits on the session bus.

mod matcher;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;

#[cfg(not(any(target_os = "linux", windows)))]
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, MutexGuard, PoisonError,
    atomic::{AtomicBool, Ordering},
};

pub use yapfast_core::keybinds::*;
use yapfast_core::runtime::BackgroundRuntime;

use matcher::Matcher;

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

type Report = Arc<dyn Fn(Status) + Send + Sync>;

/// Keybinds from the window's own key events, while nothing else hears
/// them, and the scan codes push-to-talk keybinds hold. A client may hand
/// its keys on from a thread other than the window's, as iced's event
/// filter runs on its executor.
static FOCUSED: Mutex<Option<(Matcher, Sink)>> = Mutex::new(None);
static TALK_CODES: Mutex<Vec<u32>> = Mutex::new(Vec::new());
/// The window's own key events stand in for global keys.
static FOCUSED_ON: AtomicBool = AtomicBool::new(false);

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The window's own key going down or up, by its scan code, or the focus
/// going (none), which lets everything go as its keys' release won't come:
/// runs keybinds while no global keys work.
pub fn window_key(scancode: Option<u32>, down: bool) {
    if !FOCUSED_ON.load(Ordering::Relaxed) {
        return;
    }
    let mut focused = locked(&FOCUSED);
    let Some((matcher, sink)) = focused.as_mut() else {
        return;
    };
    let out = &mut |action, down| sink.act(action, down);
    match scancode {
        Some(code) => matcher.key(code, down, out),
        None => matcher.release_all(out),
    }
}

/// Whether the key at `scancode` is one a push-to-talk keybind holds, which
/// shouldn't also type into the composer.
pub fn is_talk_key(scancode: u32) -> bool {
    locked(&TALK_CODES).contains(&scancode)
}

/// The scan codes of the keys push-to-talk keybinds among `watched` hold.
pub fn talk_codes(watched: &[Watched]) -> impl Iterator<Item = u32> + '_ {
    watched
        .iter()
        .filter(|watched| watched.action == Action::PushToTalk)
        .flat_map(|watched| watched.keys.keys().iter().map(|key| key.scancode))
}

/// Watches keybinds in every app it can, until dropped. One at a time: drop
/// the old one before starting another.
pub struct Watcher {
    watched: Vec<Watched>,
    status: Arc<Mutex<Status>>,
    #[cfg(target_os = "linux")]
    global: Option<linux::Global>,
    #[cfg(windows)]
    global: Option<windows::Hook>,
}

impl Watcher {
    /// Starts watching `watched`. `toggled` hears toggling keybinds and
    /// `told` where they work whenever that changes, both from any thread.
    /// The window's own key events stand in where nothing else hears keys
    /// ([`window_key`]).
    pub fn start(
        watched: Vec<Watched>,
        runtime: &BackgroundRuntime,
        toggled: Toggled,
        told: impl Fn(Status) + Send + Sync + 'static,
    ) -> Self {
        let sink = Sink { toggled };
        *locked(&FOCUSED) = Some((Matcher::new(&watched), sink.clone()));
        *locked(&TALK_CODES) = talk_codes(&watched).collect();
        let status = Arc::new(Mutex::new(Status::default()));
        let shared = status.clone();
        let report: Report = Arc::new(move |now: Status| {
            FOCUSED_ON.store(matches!(now.reach, Reach::Focused(_)), Ordering::Relaxed);
            *locked(&shared) = now.clone();
            told(now);
        });
        tracing::info!(
            target: crate::LOG,
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
            watched,
            status,
            #[cfg(any(target_os = "linux", windows))]
            global,
        }
    }

    pub fn watched(&self) -> &[Watched] {
        &self.watched
    }

    pub fn status(&self) -> Status {
        locked(&self.status).clone()
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // What hears keys stops first, so nothing holds a key after this.
        #[cfg(any(target_os = "linux", windows))]
        drop(self.global.take());
        FOCUSED_ON.store(false, Ordering::Relaxed);
        let taken = locked(&FOCUSED).take();
        if let Some((mut matcher, sink)) = taken {
            matcher.release_all(&mut |action, down| sink.act(action, down));
        }
        locked(&TALK_CODES).clear();
        yapfast_core::push_to_talk::hold(false);
        yapfast_core::push_to_talk::hold_mute(false);
    }
}
