//! What the window asks of the desktop beyond iced: the tray icon,
//! notifications, a later start of Yapfast handing over, starting at sign-in,
//! Discord's links, copied pictures, updates, the window's icon and files
//! dragged over it on Wayland. The root crate's desktop modules do each
//! (`src/desktop.rs`), and tell the window what they hear through a
//! [`Tell`], from threads of their own. Without them, as in the tests, each
//! does nothing.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use yapfast_core::{
    chat::Alert,
    keybinds::{Action, Status, Watched},
    runtime::BackgroundRuntime,
};

/// What the tray's icon or its menu asks for, or says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayChoice {
    Open,
    Disconnect,
    Quit,
    /// A tray came or went, which decides what closing the window does.
    Available,
}

/// What the desktop said, for the window to act on.
#[derive(Clone, Debug)]
pub enum Heard {
    Tray(TrayChoice),
    /// A notification was clicked: its conversation, or [`ABOUT`].
    Opened(String),
    /// Yapfast was started again on this folder: the desktop's activation
    /// token for that start, and the link it brought.
    Launched {
        token: Option<String>,
        link: Option<String>,
    },
    /// The look at the releases, or the update, went a step further.
    Updates,
    /// Files dragged over the window, where winit doesn't follow them.
    Drag(Drag),
}

/// A drag of files over the window, as the root crate's `drop.rs` follows it.
#[derive(Clone, Debug)]
pub enum Drag {
    Entered,
    /// Where in the window it is, in the desktop's logical pixels.
    Moved(f64, f64),
    Left,
    Dropped(Vec<std::path::PathBuf>),
}

/// Hands what the desktop heard to the window's event loop.
pub type Tell = Arc<dyn Fn(Heard) + Send + Sync>;

/// Where the pointer is over the window, asked of the platform while files
/// hover over it where winit says nothing of where (X11 and Windows): in
/// the desktop's logical pixels, as [`Drag::Moved`].
pub trait PointerAt: Send + Sync {
    fn at(&self) -> Option<(f64, f64)>;
}

/// The "conversation" of the notification of a newer release: opening it
/// shows About.
pub const ABOUT: &str = "yapfast:about";

/// The tray's icon while it's there.
pub trait Tray {
    /// A tray shows the icon now, so the window can close into it.
    fn available(&self) -> bool;
    /// The unread dot, and Disconnect in its menu during a call.
    fn show(&self, unread: bool, call: bool);
}

/// Shows desktop notifications.
pub trait Notifier {
    /// `text` says what the message says, rather than only who wrote.
    fn show(&self, alert: Alert, text: bool);
}

/// What About shows of the releases.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdatesShown {
    pub status: UpdateStatus,
    /// The release offered or skipped.
    pub latest: String,
    pub error: String,
    pub notes: String,
    /// A package manager installed this copy, which updates it.
    pub manager: String,
    pub can_install: bool,
    /// What Update and restart will do, or why this copy can't.
    pub install_note: String,
    pub install_error: String,
    /// Downloading or finishing an update: how far, in words and as a
    /// fraction (below 0 while it can't say), and whether it can stop.
    pub installing: bool,
    pub install_text: String,
    pub install_progress: f32,
    pub can_cancel: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    Current,
    Available,
    Skipped,
    Failed,
    Source,
}

/// The choices about updates the layout keeps.
#[derive(Clone, Debug, Default)]
pub struct UpdateChoices {
    pub nightlies: bool,
    pub skipped: Option<String>,
}

/// The release the last look found worth telling of: its tag and its page.
#[derive(Clone, Debug)]
pub struct Offered {
    pub tag: String,
    pub page: String,
    /// Skipped by the person: About still links it, but offers nothing.
    pub skipped: bool,
}

/// Looks at the releases and installs one.
pub trait Updates {
    /// Asks GitHub for the releases, unless a look is under way; tells
    /// [`Heard::Updates`] when it's done.
    fn look(&self);
    fn shown(&self, choices: &UpdateChoices) -> UpdatesShown;
    /// An update is downloading or ready, which About shows the progress
    /// of: what [`Updates::shown`] says as `installing`, without working
    /// out the rest.
    fn installing(&self) -> bool;
    /// The newest release the choices let through.
    fn offered(&self, choices: &UpdateChoices) -> Option<Offered>;
    /// Downloads the offered release and checks it; tells [`Heard::Updates`]
    /// as it goes, and once it's [`Updates::ready`].
    fn install(&self, choices: &UpdateChoices);
    fn cancel(&self);
    /// The update is ready for Yapfast to end and finish it.
    fn ready(&self) -> bool;
}

/// The desktop's parts, which the root crate fills in.
pub trait Desktop: Send + Sync {
    /// The window's app ID on Linux, which names its desktop entry.
    fn app_id(&self) -> &'static str {
        ""
    }
    /// Starts what talks back to the window: later starts of Yapfast on this
    /// folder handing over, and updates' results.
    fn start(&self, _tell: Tell) {}
    /// Puts Yapfast's icon in the tray; none where there's no tray.
    fn tray(&self, _tell: Tell) -> Option<Box<dyn Tray>> {
        None
    }
    /// The activation token of the tray click that's opening the window.
    fn tray_token(&self) -> Option<String> {
        None
    }
    fn notifier(&self, _root: &Path, _tell: Tell) -> Option<Box<dyn Notifier>> {
        None
    }
    /// Brings a shown window forward with a later start's token.
    fn activate(&self, _window: &dyn iced::window::Window, _token: &str) {}
    /// Gives a window just made what iced can't: its icon on Windows; and
    /// on Wayland follows files dragged over it.
    fn decorate(&self, _window: &dyn iced::window::Window, _tell: Tell) {}
    /// Asks where the pointer is over `window`, whose physical pixels are
    /// `scale` of its logical ones; none where the platform says itself
    /// (Wayland) or won't.
    fn pointer(
        &self,
        _window: &dyn iced::window::Window,
        _scale: f64,
    ) -> Option<Box<dyn PointerAt>> {
        None
    }
    /// The scale of the display `window` is on, where the window's own may
    /// not be it: Windows, where a compatibility setting, HIGHDPIAWARE,
    /// holds the window to the primary display's density on every display
    /// and sends no change of scale, while each monitor still names its
    /// own. None elsewhere, as a Wayland monitor names only a whole scale
    /// and the window has its fractional one.
    fn monitor_scale(&self, _window: &dyn iced::window::Window) -> Option<f32> {
        None
    }
    /// Where `window`'s frame is on the desktop, its top left corner in
    /// the desktop's pixels, while it's neither maximized, minimized nor
    /// full screen. None where the place isn't kept: Wayland doesn't say
    /// it, and on X11 a monitor gone since could leave the next window off
    /// screen, so only Windows, which tells where the monitors are, does.
    fn window_place(&self, _window: &dyn iced::window::Window) -> Option<[i32; 2]> {
        None
    }
    /// Puts a window not yet shown at `at`, as [`Desktop::window_place`]
    /// gave it, unless its title bar would show on no monitor then, as
    /// after one was unplugged; and shrinks it to the work area of the
    /// monitor it's on if it no longer fits there.
    fn put_window(&self, _window: &dyn iced::window::Window, _at: [i32; 2]) {}
    /// The window's icon where the window carries one (X11), as RGBA.
    fn window_icon(&self) -> Option<(Vec<u8>, u32, u32)> {
        None
    }
    /// Whether Yapfast starts when the person signs in; none where it can't.
    fn autostart(&self) -> Option<bool> {
        None
    }
    fn set_autostart(&self, _on: bool) -> Result<(), String> {
        Err("this build can't start at sign-in".into())
    }
    /// Whether `discord:` links open here; none where they can't.
    fn discord_links(&self, _root: &Path) -> Option<bool> {
        None
    }
    fn set_discord_links(&self, _root: &Path, _on: bool) -> Result<(), String> {
        Err("this build can't open Discord links".into())
    }
    /// Puts a picture, as it was downloaded, on the clipboard.
    fn copy_picture(&self, _bytes: Vec<u8>) {}
    /// Gives back memory freed with the window.
    fn release_memory(&self) {}
    fn updates(&self) -> Option<&dyn Updates> {
        None
    }
    /// What reads the files a paste attaches: files copied in a file
    /// manager, or a picture saved to a file; it gives none for text. It
    /// blocks while the copying app sends its data, so it runs off the
    /// window's thread. None where nothing reads them.
    fn paste_reader(&self) -> Option<PasteReader> {
        None
    }
    /// Watches keybinds (push to talk's key and the Keybinds page's) in
    /// every app it can, until what it returns is dropped. It tells
    /// `toggled` of a toggling one's press and `told` where they work each
    /// time that changes, both from the watcher's own threads.
    fn watch_keybinds(
        &self,
        _watched: Vec<Watched>,
        _runtime: &BackgroundRuntime,
        _toggled: Toggled,
        _told: Box<dyn Fn(Status) + Send + Sync>,
    ) -> Option<Box<dyn std::any::Any>> {
        None
    }
    /// Rests keybinds while one is recorded, and lets them work again.
    fn pause_keybinds(&self, _paused: bool) {}
    /// One of the window's own keys went down or up, by its scan code, or
    /// the window lost the focus (none), which lets the keys go: these run
    /// keybinds while nothing else hears the key, in other apps or here.
    fn window_key(&self, _scancode: Option<u32>, _pressed: bool) {}
}

/// See [`Desktop::paste_reader`].
pub type PasteReader = Box<dyn FnOnce() -> Option<Vec<PathBuf>> + Send>;

/// Hears a toggling keybind's press, from any thread.
pub type Toggled = Arc<dyn Fn(Action) + Send + Sync>;

/// What a build without the root crate's modules has: nothing.
pub struct Nothing;

impl Desktop for Nothing {}

/// The root crate's desktop, set once as the window opens.
static DESKTOP: std::sync::OnceLock<Arc<dyn Desktop>> = std::sync::OnceLock::new();

pub(crate) fn install(desktop: Arc<dyn Desktop>) {
    DESKTOP.set(desktop).ok();
}

/// The desktop's parts, or [`Nothing`].
pub(crate) fn desktop() -> &'static dyn Desktop {
    match DESKTOP.get() {
        Some(desktop) => desktop.as_ref(),
        None => &Nothing,
    }
}
