//! One Anomi for each data folder, unless Settings → Advanced allows more:
//! starting it again brings the running one's window forward and ends. The
//! first start holds `instance.lock` in the folder, which the system lets go
//! of when the process ends, and listens for later starts: on a Unix socket
//! beside the lock, which carries the desktop's activation token and a link,
//! or on Windows a named event, with links queued in files. Both clients
//! share the lock and the socket, so either hands a start to the other. See
//! docs/tray.md.

use std::{fs::File, path::Path, time::Duration};

const LOCK: &str = "instance.lock";
#[cfg(unix)]
const SOCKET: &str = "instance.socket";

#[cfg(target_os = "linux")]
mod activate;
#[cfg(target_os = "linux")]
pub use activate::{activate, wayland};

/// Elsewhere winit's own focus request brings a window forward.
#[cfg(not(target_os = "linux"))]
pub fn activate<W: ?Sized>(_window: &W, _token: &str) {}

/// No Wayland but on Linux.
#[cfg(not(target_os = "linux"))]
pub fn wayland<W: ?Sized>(_window: &W) -> bool {
    false
}
#[cfg(all(test, unix))]
mod tests;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix::{Listener, ask};
#[cfg(all(test, unix))]
use unix::{launch_from_line, launch_line};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::{Listener, ask};

/// How long a later start keeps trying to reach the first, which may still
/// be starting, or restarting with another renderer.
const PATIENCE: Duration = Duration::from_secs(5);
const RETRY: Duration = Duration::from_millis(100);

/// What this start of Anomi goes on to do.
pub enum Start {
    /// Run, as the first on the folder or one more beside it.
    Run(Instance),
    /// Another Anomi runs on the folder and was asked to show its window,
    /// unless this start was for the tray only: nothing is left to do.
    Handed,
}

/// What a later start asks the running Anomi to show: the desktop's
/// activation token, and a Discord link when the desktop handed it one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    pub token: Option<String>,
    pub link: Option<String>,
}

/// A running Anomi's hold on its folder, while it's the first there.
#[derive(Default)]
pub struct Instance {
    _lock: Option<File>,
    listener: Option<Listener>,
}

/// Starts Anomi on `root`. With another running there and `alone`, hands
/// this start to it, which shows its window unless `background`.
pub fn claim(root: &Path, alone: bool, background: bool, link: Option<String>) -> Start {
    claim_with(
        root,
        alone,
        background,
        Launch {
            token: activation_token(),
            link,
        },
    )
}

fn claim_with(root: &Path, alone: bool, background: bool, launch: Launch) -> Start {
    let lock = std::fs::create_dir_all(root).and_then(|()| {
        File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join(LOCK))
    });
    let lock = match lock {
        Ok(lock) => lock,
        Err(error) => {
            tracing::warn!(target: crate::LOG, %error, "could not tell whether Anomi already runs");
            return Start::Run(Instance::default());
        }
    };
    match lock.try_lock() {
        Ok(()) => {
            let listener = Listener::bind(root);
            Start::Run(Instance {
                _lock: Some(lock),
                listener,
            })
        }
        Err(std::fs::TryLockError::WouldBlock) if !alone => {
            tracing::info!(target: crate::LOG, "another Anomi runs on this folder; starting one more");
            Start::Run(Instance::default())
        }
        Err(std::fs::TryLockError::WouldBlock) if background && launch.link.is_none() => {
            tracing::info!(target: crate::LOG, "Anomi already runs; nothing to start in the tray");
            Start::Handed
        }
        Err(std::fs::TryLockError::WouldBlock) => {
            if hand(root, &launch) {
                tracing::info!(target: crate::LOG, "Anomi already runs; showed its window");
                Start::Handed
            } else {
                tracing::warn!(target: crate::LOG, "Anomi already runs but didn't answer; starting another");
                Start::Run(Instance::default())
            }
        }
        Err(std::fs::TryLockError::Error(error)) => {
            tracing::warn!(target: crate::LOG, %error, "could not tell whether Anomi already runs");
            Start::Run(Instance::default())
        }
    }
}

impl Instance {
    /// Calls `opened` on a thread of its own for each later start that asks
    /// for the window, with the activation token the desktop gave that start.
    pub fn listen(&mut self, opened: impl Fn(Launch) + Send + 'static) {
        if let Some(listener) = self.listener.as_mut() {
            listener.listen(opened);
        }
    }
}

/// The token the desktop launched this start with, which lets the window
/// shown take the focus: Wayland's xdg-activation or X11's startup
/// notification.
fn activation_token() -> Option<String> {
    #[cfg(target_os = "linux")]
    for name in ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"] {
        if let Ok(token) = std::env::var(name)
            && !token.is_empty()
            && !token.contains(char::is_whitespace)
        {
            return Some(token);
        }
    }
    None
}

/// Asks the first Anomi to show its window, waiting a little for it to listen.
fn hand(root: &Path, launch: &Launch) -> bool {
    let started = std::time::Instant::now();
    loop {
        match ask(root, launch) {
            Ok(()) => return true,
            Err(error) if started.elapsed() >= PATIENCE => {
                tracing::warn!(target: crate::LOG, %error, "could not reach the running Anomi");
                return false;
            }
            Err(_) => std::thread::sleep(RETRY),
        }
    }
}

/// The folder under a data folder holding links handed to its running
/// Anomi on Windows, whose named event carries no words of its own.
#[cfg(any(windows, test))]
const LINK_QUEUE: &str = "instance-links";

/// Leaves a link for the running Anomi on `root` to take when its event
/// is set. The file lands whole under its final name, so the running one
/// never reads a half-written link.
#[cfg(any(windows, test))]
fn queue_link(root: &Path, link: &str) -> std::io::Result<()> {
    let folder = root.join(LINK_QUEUE);
    std::fs::create_dir_all(&folder)?;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let next = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let path = folder.join(format!(
        "{stamp:020}-{}-{next:020}.link",
        std::process::id()
    ));
    let waiting = path.with_extension("tmp");
    std::fs::write(&waiting, link)?;
    std::fs::rename(&waiting, &path)
}

/// Takes the links waiting on `root`, oldest first.
#[cfg(any(windows, test))]
fn drain_link_queue(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root.join(LINK_QUEUE)) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "link")
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let link = std::fs::read_to_string(&path).ok()?;
            let _ = std::fs::remove_file(&path);
            let link = link.trim();
            (!link.is_empty() && link.len() <= 4096).then(|| link.to_owned())
        })
        .collect()
}

/// Drops links left for a Anomi that has gone; the new first start on a
/// folder opens no stale link when a later start shows its window.
#[cfg(any(windows, test))]
fn clear_link_queue(root: &Path) {
    let _ = std::fs::remove_dir_all(root.join(LINK_QUEUE));
}

/// The token a later start brought, for the window made next.
#[cfg(target_os = "linux")]
static TOKEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The window made next takes the focus with `token`.
pub fn give_next_window(token: Option<String>) {
    #[cfg(target_os = "linux")]
    {
        *TOKEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = token;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = token;
}

/// The token for the window made next, once: each toolkit gives it the
/// window it makes.
pub fn take_next_token() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        TOKEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
