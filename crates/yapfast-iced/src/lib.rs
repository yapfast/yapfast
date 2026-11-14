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
//! costs no message and no rebuild of the view.
//!
//! Where things are: [`app`] holds the window's state, its messages and the
//! view's layers; [`sync`] takes in what changed in the core's state; each
//! part of the window has its module ([`rail`], [`sidebar`], [`header`],
//! [`timeline`], [`composer`], [`members`], [`profile`], [`picker`],
//! [`menus`], [`call`]); [`widgets`], [`field`], [`select`] and [`video`]
//! are the client's own widgets; [`fonts`], [`pictures`] and [`paint`]
//! what it draws with.

#[cfg(all(feature = "cpu", feature = "gpu"))]
compile_error!("yapfast-iced draws with one renderer: `cpu` or `gpu`, not both");
#[cfg(not(any(feature = "cpu", feature = "gpu")))]
compile_error!("yapfast-iced needs a renderer: `cpu` or `gpu`");

mod account;
mod active_now;
mod app;
mod bot_select;
mod browse;
mod browser;
mod call;
mod cards;
mod channel_settings;
mod client;
mod commands;
mod composer;
mod control;
mod controls;
mod dialogs;
mod edit;
mod field;
mod fonts;
mod form;
mod forward;
mod friends;
mod header;
mod layout;
mod looks;
mod markdown;
mod media;
mod members;
mod menus;
mod message;
mod mfa;
mod paint;
mod people;
mod person;
mod picker;
mod pictures;
mod places;
mod playback;
mod polls;
mod popover;
mod profile;
mod rail;
mod reactions;
mod reorder;
mod select;
mod server_dialogs;
mod server_settings;
mod settings;
mod sheet;
mod sidebar;
mod signin;
mod status;
mod style;
mod suggest;
mod switcher;
mod sync;
mod theme;
mod tile;
mod timeline;
mod typing;
mod uploads;
mod video;
mod viewer;
mod widgets;

#[cfg(test)]
mod tests;

use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};

use iced::{Size, Task};
use yapfast_core::{application::Action, runtime::BackgroundRuntime};

use crate::{app::App, client::Client};
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
    /// Watches the push-to-talk key the layout keeps (as the Slint client
    /// wrote it) in every app it can, until what it returns is dropped.
    pub push_to_talk: Option<TalkKey>,
    /// Which build this is, as Settings shows it.
    pub build: settings::Build,
}

/// See [`Options::push_to_talk`].
/// It's given the key and what to tell where the key works (what Settings
/// says under it), each time that changes.
pub type TalkKey = fn(
    &serde_json::Value,
    &BackgroundRuntime,
    Box<dyn Fn(String) + Send + Sync>,
) -> Option<Box<dyn std::any::Any>>;

/// The root crate's [`Options::paste`], set once as the window opens.
static PASTE: OnceLock<fn() -> Option<Vec<PathBuf>>> = OnceLock::new();

/// The root crate's [`Options::push_to_talk`], set once as the window
/// opens.
static TALK_KEY: OnceLock<TalkKey> = OnceLock::new();

/// What watches the push-to-talk key, where the build has one.
pub(crate) fn talk_key_watcher() -> Option<TalkKey> {
    TALK_KEY.get().copied()
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

/// Opens the window and runs until it closes.
pub fn run(runtime: BackgroundRuntime, root: PathBuf, options: Options) -> anyhow::Result<()> {
    if options.gpu != cfg!(feature = "gpu") {
        tracing::warn!(
            gpu = options.gpu,
            "this iced build draws with its one renderer whatever --renderer asks"
        );
    }
    if let Some(paste) = options.paste {
        let _ = PASTE.set(paste);
    }
    // Push to talk's key is watched by the window (`settings/voice.rs`),
    // which Settings changes.
    if let Some(watch) = options.push_to_talk {
        let _ = TALK_KEY.set(watch);
    }
    let _ = BUILD.set(options.build);
    pictures::install();
    video::install();
    fonts::install(options.twemoji);
    let runtime = Arc::new(runtime);
    let _ = EXECUTOR.set(runtime.block_on(async { tokio::runtime::Handle::current() }));
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
    // iced calls `boot` once; the client goes into the window then.
    let client = RefCell::new(Some(client));
    let boot = move || {
        let client = client
            .borrow_mut()
            .take()
            .expect("iced boots the window once");
        App::new(client, size)
    };
    tracing::info!("starting the iced window");
    iced::application(boot, App::update, App::view)
        .executor::<Exec>()
        .subscription(App::subscription)
        .title("Yapfast")
        .default_font(fonts::REGULAR)
        .window_size(Size::new(size.0 as f32, size.1 as f32))
        .style(App::window_style)
        .theme(App::theme)
        .run()
        .map_err(|error| anyhow::anyhow!("the iced window failed: {error}"))
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

/// `message` after `delay`, as a task on the backend's runtime.
pub(crate) fn after<Message: Send + 'static>(delay: Duration, message: Message) -> Task<Message> {
    // Made when first polled, so it needs the runtime only then.
    Task::perform(async move { tokio::time::sleep(delay).await }, move |()| {
        message
    })
}
