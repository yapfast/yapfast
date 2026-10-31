//! One Yapfast for each data folder, unless Settings → Advanced allows more:
//! starting it again brings the running one's window forward and ends. The
//! first start holds `instance.lock` in the folder, which the system lets go
//! of when the process ends, and listens for later starts: on a Unix socket
//! beside the lock, which carries the desktop's activation token, or on
//! Windows a named event. See docs/tray.md.

use std::{fs::File, path::Path, time::Duration};

const LOCK: &str = "instance.lock";
#[cfg(unix)]
const SOCKET: &str = "instance.socket";

/// How long a later start keeps trying to reach the first, which may still
/// be starting, or restarting with another renderer.
const PATIENCE: Duration = Duration::from_secs(5);
const RETRY: Duration = Duration::from_millis(100);

/// What this start of Yapfast goes on to do.
pub enum Start {
    /// Run, as the first on the folder or one more beside it.
    Run(Instance),
    /// Another Yapfast runs on the folder and was asked to show its window,
    /// unless this start was for the tray only: nothing is left to do.
    Handed,
}

/// What a later start asks the running Yapfast to show: the desktop's
/// activation token, and a Discord link when the desktop handed it one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    pub token: Option<String>,
    pub link: Option<String>,
}

/// A running Yapfast's hold on its folder, while it's the first there.
#[derive(Default)]
pub struct Instance {
    _lock: Option<File>,
    listener: Option<Listener>,
}

/// Starts Yapfast on `root`. With another running there and `alone`, hands
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
            tracing::warn!(%error, "could not tell whether Yapfast already runs");
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
            tracing::info!("another Yapfast runs on this folder; starting one more");
            Start::Run(Instance::default())
        }
        Err(std::fs::TryLockError::WouldBlock) if background && launch.link.is_none() => {
            tracing::info!("Yapfast already runs; nothing to start in the tray");
            Start::Handed
        }
        Err(std::fs::TryLockError::WouldBlock) => {
            if hand(root, &launch) {
                tracing::info!("Yapfast already runs; showed its window");
                Start::Handed
            } else {
                tracing::warn!("Yapfast already runs but didn't answer; starting another");
                Start::Run(Instance::default())
            }
        }
        Err(std::fs::TryLockError::Error(error)) => {
            tracing::warn!(%error, "could not tell whether Yapfast already runs");
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

/// Asks the first Yapfast to show its window, waiting a little for it to listen.
fn hand(root: &Path, launch: &Launch) -> bool {
    let started = std::time::Instant::now();
    loop {
        match ask(root, launch) {
            Ok(()) => return true,
            Err(error) if started.elapsed() >= PATIENCE => {
                tracing::warn!(%error, "could not reach the running Yapfast");
                return false;
            }
            Err(_) => std::thread::sleep(RETRY),
        }
    }
}

#[cfg(unix)]
fn ask(root: &Path, launch: &Launch) -> std::io::Result<()> {
    use std::io::Write;
    let mut stream = std::os::unix::net::UnixStream::connect(root.join(SOCKET))?;
    writeln!(stream, "{}", launch_line(launch))
}

/// A later start as one socket line: `open`, then its token and link,
/// with `-` for either it hasn't got. Neither has whitespace.
#[cfg(unix)]
fn launch_line(launch: &Launch) -> String {
    format!(
        "open {} {}",
        launch.token.as_deref().unwrap_or("-"),
        launch.link.as_deref().unwrap_or("-")
    )
}

/// Reads a socket line back, including the older `open [token]` shape.
#[cfg(unix)]
fn launch_from_line(line: &str) -> Option<Launch> {
    let mut words = line.split_whitespace();
    if words.next() != Some("open") {
        return None;
    }
    let field = |word: Option<&str>| match word {
        None | Some("-") => None,
        Some(word) => Some(word.to_owned()),
    };
    let token = field(words.next());
    let link = field(words.next());
    if words.next().is_some() {
        return None;
    }
    Some(Launch { token, link })
}

#[cfg(unix)]
struct Listener(Option<std::os::unix::net::UnixListener>);

#[cfg(unix)]
impl Listener {
    fn bind(root: &Path) -> Option<Self> {
        let path = root.join(SOCKET);
        // Holding the lock, this start is the only one: a socket there is a
        // gone Yapfast's.
        let _ = std::fs::remove_file(&path);
        match std::os::unix::net::UnixListener::bind(&path) {
            Ok(listener) => Some(Self(Some(listener))),
            Err(error) => {
                tracing::warn!(%error, "starting Yapfast again can't show this window");
                None
            }
        }
    }

    fn listen(&mut self, opened: impl Fn(Launch) + Send + 'static) {
        let Some(listener) = self.0.take() else {
            return;
        };
        let thread = std::thread::Builder::new()
            .name("yapfast-instance".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                use std::io::Read;
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut request = String::new();
                    // An activation token is short; this caps a stranger's line.
                    if stream.take(4096).read_to_string(&mut request).is_err() {
                        continue;
                    }
                    if let Some(launch) = launch_from_line(&request) {
                        opened(launch);
                    }
                }
            });
        if let Err(error) = thread {
            tracing::warn!(%error, "starting Yapfast again can't show this window");
        }
    }
}

#[cfg(windows)]
mod windows {
    //! A named auto-reset event for the folder: a later start sets it, and the
    //! first Yapfast's thread, waiting on it, shows the window. A second,
    //! unnamed event stops the thread before a restart starts the next process.

    use std::path::Path;

    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        System::Threading::{
            CreateEventW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent,
            WaitForMultipleObjects,
        },
        UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow},
    };

    /// The event's name: `Local\yapfast-` and an FNV-1a hash of the folder's
    /// absolute path, as Windows compares paths, without case.
    fn name(root: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        let root = std::path::absolute(root).unwrap_or_else(|_| root.to_owned());
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for unit in root.as_os_str().encode_wide() {
            let unit = char::from_u32(unit.into()).map_or(unit, |c| c.to_ascii_lowercase() as u16);
            for byte in unit.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
            }
        }
        format!("Local\\yapfast-{hash:016x}")
            .encode_utf16()
            .chain([0])
            .collect()
    }

    pub(super) fn ask(root: &Path, launch: &super::Launch) -> std::io::Result<()> {
        // SAFETY: the name is NUL-terminated; the handle is closed once set.
        unsafe {
            let event = OpenEventW(EVENT_MODIFY_STATE, 0, name(root).as_ptr());
            if event.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            if let Some(link) = launch.link.as_deref()
                && let Err(error) = super::queue_link(root, link)
            {
                CloseHandle(event);
                return Err(error);
            }
            // The started process may bring a window forward; it passes that on.
            AllowSetForegroundWindow(ASFW_ANY);
            let set = SetEvent(event);
            CloseHandle(event);
            if set == 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    /// The two events, as addresses a thread can take.
    pub(super) struct Listener {
        event: usize,
        stop: usize,
        root: std::path::PathBuf,
        listening: bool,
    }

    impl Listener {
        pub(super) fn bind(root: &Path) -> Option<Self> {
            super::clear_link_queue(root);
            // SAFETY: the name is NUL-terminated; null attributes are the defaults.
            let (event, stop) = unsafe {
                (
                    CreateEventW(std::ptr::null(), 0, 0, name(root).as_ptr()),
                    CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()),
                )
            };
            if event.is_null() || stop.is_null() {
                tracing::warn!(
                    error = %std::io::Error::last_os_error(),
                    "starting Yapfast again can't show this window"
                );
                // SAFETY: each is a handle this made, or null.
                unsafe {
                    if !event.is_null() {
                        CloseHandle(event);
                    }
                    if !stop.is_null() {
                        CloseHandle(stop);
                    }
                }
                return None;
            }
            Some(Self {
                event: event as usize,
                stop: stop as usize,
                root: root.to_owned(),
                listening: false,
            })
        }

        pub(super) fn listen(&mut self, opened: impl Fn(super::Launch) + Send + 'static) {
            let (event, stop) = (self.event, self.stop);
            let root = self.root.clone();
            let thread = std::thread::Builder::new()
                .name("yapfast-instance".into())
                .stack_size(64 * 1024)
                .spawn(move || {
                    let handles = [event as HANDLE, stop as HANDLE];
                    // SAFETY: both handles stay open until this thread closes them.
                    while unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) }
                        == WAIT_OBJECT_0
                    {
                        let links = super::drain_link_queue(&root);
                        if links.is_empty() {
                            opened(super::Launch::default());
                        } else {
                            for link in links {
                                opened(super::Launch {
                                    token: None,
                                    link: Some(link),
                                });
                            }
                        }
                    }
                    // SAFETY: Drop only sets `stop` once this thread runs.
                    unsafe {
                        CloseHandle(handles[0]);
                        CloseHandle(handles[1]);
                    }
                });
            match thread {
                Ok(_) => self.listening = true,
                Err(error) => {
                    tracing::warn!(%error, "starting Yapfast again can't show this window");
                }
            }
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            // SAFETY: the handles are open: the thread closes them only after `stop`.
            unsafe {
                if self.listening {
                    SetEvent(self.stop as HANDLE);
                } else {
                    CloseHandle(self.event as HANDLE);
                    CloseHandle(self.stop as HANDLE);
                }
            }
        }
    }
}

#[cfg(windows)]
use windows::{Listener, ask};

/// The folder under a data folder holding links handed to its running
/// Yapfast on Windows, whose named event carries no words of its own.
#[cfg(any(windows, test))]
const LINK_QUEUE: &str = "instance-links";

/// Leaves a link for the running Yapfast on `root` to take when its event
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

/// Drops links left for a Yapfast that has gone; the new first start on a
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

/// Gives the window Slint makes the token a later start brought, if one waits.
pub fn attributes(
    attributes: slint::winit_030::winit::window::WindowAttributes,
) -> slint::winit_030::winit::window::WindowAttributes {
    #[cfg(target_os = "linux")]
    if let Some(token) = TOKEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
    {
        use slint::winit_030::winit::{
            platform::startup_notify::WindowAttributesExtStartupNotify, window::ActivationToken,
        };
        return attributes.with_activation_token(ActivationToken::from_raw(token));
    }
    attributes
}

/// Brings a shown window forward with the token a later start brought, on
/// Wayland, where winit can't give one to a window it already made.
#[cfg(target_os = "linux")]
pub fn activate(window: &slint::winit_030::winit::window::Window, token: &str) {
    use slint::winit_030::winit::raw_window_handle::{
        HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
    };
    use wayland_client::{
        Connection, Dispatch, Proxy, QueueHandle,
        backend::{Backend, ObjectId},
        globals::{GlobalListContents, registry_queue_init},
        protocol::{wl_registry::WlRegistry, wl_surface::WlSurface},
    };
    use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;

    struct State;
    impl Dispatch<WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &WlRegistry,
            _: <WlRegistry as Proxy>::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }
    impl Dispatch<XdgActivationV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &XdgActivationV1,
            _: <XdgActivationV1 as Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    let (Ok(display), Ok(handle)) = (window.display_handle(), window.window_handle()) else {
        return;
    };
    let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(surface)) =
        (display.as_raw(), handle.as_raw())
    else {
        return;
    };
    // SAFETY: winit's display and surface live while its window does, which
    // outlives this call.
    let connection = Connection::from_backend(unsafe {
        Backend::from_foreign_display(display.display.as_ptr().cast())
    });
    let surface =
        unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.surface.as_ptr().cast()) }
            .and_then(|id| WlSurface::from_id(&connection, id));
    let result = surface
        .map_err(|error| error.to_string())
        .and_then(|surface| {
            let (globals, queue) =
                registry_queue_init::<State>(&connection).map_err(|error| error.to_string())?;
            // A compositor without the protocol leaves focus to winit's request.
            let activation: XdgActivationV1 = match globals.bind(&queue.handle(), 1..=1, ()) {
                Ok(activation) => activation,
                Err(wayland_client::globals::BindError::NotPresent) => return Ok(()),
                Err(error) => return Err(error.to_string()),
            };
            activation.activate(token.to_owned(), &surface);
            activation.destroy();
            connection.flush().map_err(|error| error.to_string())
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not bring the window forward");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn launch(token: Option<&str>, link: Option<&str>) -> Launch {
        Launch {
            token: token.map(str::to_owned),
            link: link.map(str::to_owned),
        }
    }

    fn opened(instance: &mut Instance) -> std::sync::mpsc::Receiver<Launch> {
        let (sender, receiver) = std::sync::mpsc::channel();
        instance.listen(move |launch| {
            let _ = sender.send(launch);
        });
        receiver
    }

    #[test]
    fn launch_lines_carry_tokens_and_links() {
        let with_link = launch(
            Some("token-1"),
            Some("https://discord.com/channels/10/11/12"),
        );
        assert_eq!(
            launch_line(&with_link),
            "open token-1 https://discord.com/channels/10/11/12"
        );
        assert_eq!(launch_from_line(&launch_line(&with_link)), Some(with_link));
        assert_eq!(
            launch_from_line("open token-1"),
            Some(launch(Some("token-1"), None))
        );
        assert_eq!(
            launch_from_line("open - yapfast://channels/@me/20"),
            Some(launch(None, Some("yapfast://channels/@me/20")))
        );
        assert_eq!(launch_from_line("open"), Some(Launch::default()));
        // A start writes a link with a space as one word, which arrives whole.
        #[cfg(feature = "discord")]
        {
            let link =
                yapfast_core::discord::discord_link("https://discord.com/channels/10/11?a b");
            let spaced = launch(None, link.as_deref());
            assert_eq!(launch_from_line(&launch_line(&spaced)), Some(spaced));
        }
        assert_eq!(launch_from_line("open one two three"), None);
        assert_eq!(launch_from_line("close token-1"), None);
    }

    #[test]
    fn queued_links_are_taken_in_order_and_cleared() {
        let root = tempfile::tempdir().unwrap();
        queue_link(root.path(), "https://discord.com/channels/10/11").unwrap();
        queue_link(root.path(), "https://discord.com/users/99").unwrap();
        assert_eq!(
            drain_link_queue(root.path()),
            [
                "https://discord.com/channels/10/11",
                "https://discord.com/users/99"
            ]
        );
        assert!(drain_link_queue(root.path()).is_empty());
        queue_link(root.path(), "https://discord.gg/stale").unwrap();
        clear_link_queue(root.path());
        assert!(drain_link_queue(root.path()).is_empty());
    }

    #[test]
    fn a_later_start_shows_the_first_window() {
        let root = tempfile::tempdir().unwrap();
        let Start::Run(mut first) = claim_with(root.path(), true, false, Launch::default()) else {
            panic!("the first start runs");
        };
        let requests = opened(&mut first);
        assert!(matches!(
            claim_with(
                root.path(),
                true,
                false,
                launch(Some("token-1"), Some("https://discord.com/channels/10/11"))
            ),
            Start::Handed
        ));
        let wait = Duration::from_secs(5);
        assert_eq!(
            requests.recv_timeout(wait).unwrap(),
            launch(Some("token-1"), Some("https://discord.com/channels/10/11"))
        );
        assert!(matches!(
            claim_with(root.path(), true, false, Launch::default()),
            Start::Handed
        ));
        assert_eq!(requests.recv_timeout(wait).unwrap(), Launch::default());
        // Starting at sign-in leaves the running one as it is.
        assert!(matches!(
            claim_with(root.path(), true, true, Launch::default()),
            Start::Handed
        ));
        // Allowed, another runs beside the first, without listening.
        let Start::Run(another) = claim_with(root.path(), false, false, Launch::default()) else {
            panic!("allowed, a later start runs");
        };
        assert!(another._lock.is_none() && another.listener.is_none());
        assert!(requests.recv_timeout(Duration::from_millis(200)).is_err());
        // Once the first ends, the next start is the first.
        drop(first);
        assert!(matches!(
            claim_with(root.path(), true, false, Launch::default()),
            Start::Run(Instance {
                _lock: Some(_),
                listener: Some(_)
            })
        ));
    }
}
