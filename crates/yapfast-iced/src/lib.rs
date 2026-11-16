//! Yapfast's front end in iced 0.14, beside the Slint one until it replaces
//! it (`.local/iced-rewrite/plan.md`, docs/iced-client.md). The root
//! crate's `iced` feature builds it into the same executable, and `main.rs`
//! hands it the backend's runtime and the data directory.
//!
//! It drives `yapfast-core` as the Slint client does, through `Action`s,
//! `AppState` and the backend's events ([`client`]), and draws with iced's
//! tiny-skia renderer, patched in `vendor/iced_tiny_skia` (or wgpu, with the
//! `gpu` feature). iced builds the whole view again after every message and
//! lays it out, so what costs is kept out of the view: the lists are
//! virtualized by hand (`widgets::Rows` holds only the rows near each
//! viewport, keyed so their widget state follows them), pictures are the
//! core's own buffers in the renderer's format ([`pictures`]), and moving
//! pictures advance inside their widget on the window's redraws, so a frame
//! costs no message and no rebuild of the view. Meters, clocks and video
//! frames are read as they're drawn and only redraw the window
//! ([`widgets::Gauge`], [`widgets::LiveText`], [`video`]).
//!
//! Where things are: [`app`] holds the window's state, its messages and the
//! view's layers; [`sync`] takes in what changed in the core's state; each
//! part of the window has its module ([`rail`], [`sidebar`], [`header`],
//! [`timeline`], [`composer`], [`members`], [`profile`], [`picker`],
//! [`menus`], [`call`]); [`widgets`], [`field`], [`select`] and [`video`]
//! are the client's own widgets; [`fonts`], [`pictures`] and [`paint`]
//! what it draws with.

// A task dropped is work that never runs (a sync's pictures, a scroll):
// every task goes back to the runtime.
#![cfg_attr(not(test), warn(clippy::let_underscore_must_use))]

#[cfg(all(feature = "cpu", feature = "gpu"))]
compile_error!("yapfast-iced draws with one renderer: `cpu` or `gpu`, not both");
#[cfg(not(any(feature = "cpu", feature = "gpu")))]
compile_error!("yapfast-iced needs a renderer: `cpu` or `gpu`");

mod account;
mod active_now;
mod animated;
mod app;
mod arrange;
mod away;
mod banners;
mod bot_select;
mod browse;
mod browse_channels;
mod browser;
mod call;
use yapfast_iced_kit::canvas;
mod cards;
mod channel_settings;
mod client;
mod commands;
mod composer;
mod control;
use yapfast_iced_kit::controls;
mod conversation;
mod ctx;
mod desktop;
mod slot;
use yapfast_iced_kit::dialog;
mod dialogs;
mod edit;
use yapfast_iced_kit::elide;
use yapfast_iced_kit::field;
use yapfast_iced_kit::fonts;
use yapfast_iced_kit::form;
mod forum;
mod forward;
mod found;
use yapfast_iced_kit::frames;
mod friends;
mod header;
use yapfast_iced_kit::icons;
mod image_viewer;
mod info_cards;
mod layout;
mod link_dialogs;
mod links;
use yapfast_iced_kit::look;
mod looks;
mod markdown;
mod media;
mod members;
mod menus;
mod message;
mod mfa;
mod modal;
mod paint;
mod people;
mod person;
mod picker;
mod pictures;
use yapfast_iced_kit::piece;
mod places;
mod playback;
mod polls;
use yapfast_iced_kit::popover;
mod popovers;
mod profile;
mod rail;
mod reactions;
use yapfast_iced_kit::reorder;
mod search;
mod select;
mod server_dialogs;
mod server_settings;
mod settings;
use yapfast_iced_kit::sheet;
mod shell;
mod sidebar;
mod signin;
mod status;
use yapfast_iced_kit::style;
mod keybinds;
mod suggest;
mod switcher;
mod sync;
use yapfast_iced_kit::theme;
mod themes;
mod thread;
use yapfast_iced_kit::tile;
mod timeline;
mod typing;
mod updates;
mod uploads;
mod video;
use yapfast_iced_kit::viewer;
use yapfast_iced_kit::widgets;
mod window_bar;

#[cfg(test)]
mod tests;

use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use iced::Task;
use yapfast_core::{application::Action, runtime::BackgroundRuntime};

use crate::{app::App, client::Client};
pub use desktop::{
    Desktop, Drag, Heard, Notifier, Offered, PointerAt, Tell, Tray, TrayChoice, UpdateChoices,
    UpdateStatus, Updates, UpdatesShown,
};
/// The window the desktop's parts act on, by its handles.
pub use iced::window::Window;
pub use settings::Build;

/// How the window starts.
pub struct Options {
    /// The core's sample conversations, without connecting.
    pub preview: bool,
    /// `--renderer gpu`: this build's renderer is fixed when it's built, so
    /// a renderer it lacks is only noted.
    pub gpu: bool,
    /// The Twemoji font the root crate's `build.rs` packs.
    pub twemoji: &'static [u8],
    /// Reads what a paste in the composer attaches: files, or a picture
    /// saved to a file; none for text. Blocks, so it runs off the window's
    /// thread.
    pub paste: Option<fn() -> Option<Vec<PathBuf>>>,
    /// Watches keybinds (push to talk's key and the Keybinds page's) in
    /// every app it can, until what it returns is dropped; and rests them
    /// while one is recorded.
    pub keybinds: Option<WatchKeys>,
    pub pause_keybinds: Option<fn(bool)>,
    /// The window's own keys, while no global key works: a key's scan
    /// code going down or up, and none as the window loses the focus,
    /// which lets the key go.
    pub window_talk: Option<fn(Option<u32>, bool)>,
    /// Which build this is, as Settings shows it.
    pub build: settings::Build,
    /// `--background`: start in the tray with a saved session, as starting
    /// at sign-in does.
    pub background: bool,
    /// A Discord link handed to this start, as one word.
    pub open_link: Option<String>,
    /// The tray, notifications, links and the rest the root crate's
    /// desktop modules do.
    pub desktop: Arc<dyn Desktop>,
}

/// How the window ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Closed,
    /// An update was downloaded and checked; `main` finishes it.
    Update,
}

/// How the window ended, set as it ends.
static EXIT: std::sync::Mutex<Exit> = std::sync::Mutex::new(Exit::Closed);

pub(crate) fn exit_for(exit: Exit) {
    *EXIT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = exit;
}

/// See [`Options::keybinds`]. It's given the keybinds to watch, what to
/// tell of a toggling one's press, and what to tell of where they work
/// each time that changes; both are told from the watcher's own threads.
pub type WatchKeys = fn(
    Vec<yapfast_core::keybinds::Watched>,
    &BackgroundRuntime,
    Toggled,
    Box<dyn Fn(yapfast_core::keybinds::Status) + Send + Sync>,
) -> Option<Box<dyn std::any::Any>>;

/// Hears a toggling keybind's press, from any thread.
pub type Toggled = Arc<dyn Fn(yapfast_core::keybinds::Action) + Send + Sync>;

/// The root crate's [`Options::paste`], set once as the window opens.
static PASTE: OnceLock<fn() -> Option<Vec<PathBuf>>> = OnceLock::new();

/// The root crate's [`Options::keybinds`] and [`Options::pause_keybinds`],
/// set once as the window opens.
static WATCH_KEYS: OnceLock<WatchKeys> = OnceLock::new();
static PAUSE_KEYS: OnceLock<fn(bool)> = OnceLock::new();

/// The root crate's [`Options::window_talk`], set once as the window opens.
pub(crate) static WINDOW_TALK: OnceLock<fn(Option<u32>, bool)> = OnceLock::new();

/// What watches keybinds, where the build has one.
pub(crate) fn keybinds_watcher() -> Option<WatchKeys> {
    WATCH_KEYS.get().copied()
}

/// Rests keybinds, or lets them work again, where the build watches them.
pub(crate) fn pause_keybinds(paused: bool) {
    if let Some(pause) = PAUSE_KEYS.get() {
        pause(paused);
    }
}

/// Which build this is, set once as the window opens.
static BUILD: OnceLock<settings::Build> = OnceLock::new();

pub(crate) fn build() -> settings::Build {
    BUILD.get().cloned().unwrap_or_default()
}

/// What reads a paste's files, where the build has one.
pub(crate) fn paste_reader() -> Option<fn() -> Option<Vec<PathBuf>>> {
    PASTE.get().copied()
}

/// The window's size when the private test display doesn't set one.
const SIZE: (u32, u32) = (1120, 760);

/// Opens the window, or starts in the tray, and runs until Yapfast quits.
pub fn run(
    runtime: Arc<BackgroundRuntime>,
    root: PathBuf,
    options: Options,
) -> anyhow::Result<Exit> {
    if options.gpu != cfg!(feature = "gpu") {
        tracing::warn!(
            gpu = options.gpu,
            "this iced build draws with its one renderer whatever --renderer asks"
        );
    }
    if let Some(paste) = options.paste {
        PASTE.set(paste).ok();
    }
    // Keybinds are watched by the window (`keybinds.rs`), which Settings
    // changes.
    if let Some(watch) = options.keybinds {
        WATCH_KEYS.set(watch).ok();
    }
    if let Some(pause) = options.pause_keybinds {
        PAUSE_KEYS.set(pause).ok();
    }
    if let Some(window_talk) = options.window_talk {
        WINDOW_TALK.set(window_talk).ok();
    }
    BUILD.set(options.build).ok();
    desktop::install(options.desktop);
    pictures::install();
    video::install();
    frames::install();
    fonts::install(options.twemoji);
    EXECUTOR
        .set(runtime.block_on(async { tokio::runtime::Handle::current() }))
        .ok();
    // As the Slint client starts: the preview, a saved session, or the
    // sign-in screen.
    let restore = !options.preview && yapfast_core::chat::can_restore(&root);
    let mut client = Client::new(runtime, root);
    if options.preview {
        client.dispatch(Action::Preview);
    } else if restore {
        client.dispatch(Action::Restore);
    }
    let size = test_size().unwrap_or(SIZE);
    // iced calls `boot` once; the client goes into the window then. It runs
    // as a daemon, which goes on without a window: closing to the tray drops
    // the window, its renderer and its widgets, as the Slint client's does.
    let start = shell::Start {
        background: options.background && !options.preview && restore,
        link: options.open_link.filter(|_| !options.preview),
    };
    let client = RefCell::new(Some((client, start)));
    let boot = move || {
        let (client, start) = client
            .borrow_mut()
            .take()
            .expect("iced boots the window once");
        let (mut app, task) = App::new(client, size);
        let shell = app.start_shell(start);
        (app, Task::batch([task, shell]))
    };
    tracing::info!(background = options.background, "starting the iced window");
    let ran = iced::daemon(boot, App::update, App::view_of)
        .executor::<Exec>()
        .subscription(App::subscription)
        .title(App::title)
        .default_font(fonts::REGULAR)
        .style(App::window_style)
        .theme(|app: &App, _| app.theme())
        .scale_factor(App::scale_factor)
        .run()
        .map_err(|error| anyhow::anyhow!("the iced window failed: {error}"));
    // What the window changed last is written before Yapfast ends.
    layout::flush_all();
    ran?;
    Ok(*EXIT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner))
}

/// The size the private test display asks for, as the Slint client takes it.
fn test_size() -> Option<(u32, u32)> {
    if std::env::var("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY").as_deref() != Ok("1") {
        return None;
    }
    let size = std::env::var("YAPFAST_UI_TEST_SIZE").ok()?;
    let (width, height) = size.split_once('x')?;
    let size = (width.parse().ok()?, height.parse().ok()?);
    (size.0 >= 800 && size.1 >= 560).then_some(size)
}

/// The backend's runtime, which iced's futures run on.
static EXECUTOR: OnceLock<tokio::runtime::Handle> = OnceLock::new();

/// Runs iced's futures (the event stream, timers, the video and control
/// tasks) on the backend's Tokio runtime rather than on threads of iced's
/// own. iced wants one of its executor features on even so (`tokio`); its
/// runtime is never made.
struct Exec(tokio::runtime::Handle);

impl iced::Executor for Exec {
    fn new() -> Result<Self, iced::futures::io::Error> {
        EXECUTOR
            .get()
            .cloned()
            .map(Self)
            .ok_or_else(|| iced::futures::io::Error::other("the backend's runtime isn't set"))
    }

    fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        self.0.spawn(future);
    }

    fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        self.0.block_on(future)
    }

    fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = self.0.enter();
        f()
    }
}

pub(crate) use yapfast_iced_kit::timer::after;
