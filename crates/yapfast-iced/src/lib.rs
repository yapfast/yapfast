//! Yapfast's front end, in iced 0.14 (docs/client/README.md). The root
//! crate's `desktop` feature builds it into the executable, and `main.rs`
//! hands it the backend's runtime and the data directory.
//!
//! It drives `yapfast-core` through `Action`s, `AppState` and the backend's
//! events ([`client`]), and draws with iced's tiny-skia renderer, patched in
//! `vendor/iced_tiny_skia` (or wgpu, with the `gpu` feature). iced builds
//! the whole view again after every message and lays it out, so what costs
//! is kept out of the view: the lists are virtualized by hand
//! (`widgets::Rows` holds only the rows near each viewport, keyed so their
//! widget state follows them), pictures are the core's own buffers in the
//! renderer's format ([`pictures`]), and moving pictures advance inside
//! their widget on the window's redraws, so a frame costs no message and no
//! rebuild of the view. Meters, clocks and video frames are read as they're
//! drawn and only redraw the window ([`widgets::Gauge`],
//! [`widgets::LiveText`], [`video`]).
//!
//! Where things are: [`app`] holds the window's state, its messages and the
//! view's layers; [`sync`] takes in what changed in the core's state; each
//! part of the window has its module ([`rail`], [`sidebar`], [`header`],
//! [`timeline`], [`composer`], [`members`], [`profile`], [`picker`],
//! [`menus`], [`call`]); [`widgets`], [`field`], [`piece`] and [`video`]
//! are the client's own widgets; [`fonts`], [`pictures`] and [`paint`]
//! what it draws with.

// A task dropped is work that never runs (a sync's pictures, a scroll):
// every task goes back to the runtime.
#![cfg_attr(not(test), warn(clippy::let_underscore_must_use))]

mod account;
mod active_now;
mod animated;
mod app;
mod arrange;
mod away;
mod banners;
mod bot_select;
mod browse_channels;
mod browser;
mod call;
use yapfast_iced_kit::canvas;
mod channel_settings;
mod client;
mod commands;
mod components;
mod composer;
mod control_file;
use yapfast_iced_kit::controls;
mod conversation;
mod ctx;
mod desktop;
mod slot;
use yapfast_iced_kit::dialog;
mod dialogs;
mod edit;
// build.rs packs Twemoji with it; here its test runs.
#[cfg(test)]
mod emoji_font_tables;
use yapfast_iced_kit::elide;
use yapfast_iced_kit::field;
use yapfast_iced_kit::fonts;
use yapfast_iced_kit::form;
mod forum;
mod forward;
mod found;
mod frame;
use yapfast_iced_kit::frames;
mod friends;
mod graphics;
mod header;
use yapfast_iced_kit::icons;
mod image_viewer;
mod info_cards;
mod layout;
mod link_dialogs;
mod links;
use yapfast_iced_kit::look;
mod bot_modal;
mod looks;
mod markdown;
mod media;
mod members;
mod menus;
mod message;
mod mfa;
mod paint;
mod people;
mod person_menu;
mod picker;
mod pictures;
use yapfast_iced_kit::piece;
mod place_menus;
mod playback;
mod polls;
use yapfast_iced_kit::popover;
use yapfast_iced_kit::renderer;
mod header_popovers;
mod profile;
mod rail;
mod reactions;
use yapfast_iced_kit::reorder;
mod search;
mod search_filters;
mod search_popout;
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
mod tag_card;
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
    Desktop, Drag, Heard, Notifier, Offered, PasteReader, PointerAt, Tell, Toggled, Tray,
    TrayChoice, UpdateChoices, UpdateStatus, Updates, UpdatesShown,
};
/// The window the desktop's parts act on, by its handles.
pub use iced::window::Window;
pub use settings::Build;

/// How the window starts.
pub struct Options {
    /// The core's sample conversations, without connecting.
    pub preview: bool,
    /// `--renderer`: the GPU (true) or the CPU for this start, whatever the
    /// GPU setting says; none follows the setting.
    pub renderer: Option<bool>,
    /// Why the GPU failed to start, when this start replaces one whose GPU
    /// failed (`Exit::UseSoftware`): it draws on the CPU, says so, and
    /// turns the GPU setting off.
    pub gpu_failure: Option<String>,
    /// Which build this is, as Settings shows it.
    pub build: settings::Build,
    /// `--background`: start in the tray with a saved session, as starting
    /// at sign-in does.
    pub background: bool,
    /// A Discord link handed to this start, as one word.
    pub open_link: Option<String>,
    /// The tray, notifications, links, pastes, keybinds and the rest the
    /// root crate's desktop modules do.
    pub desktop: Arc<dyn Desktop>,
}

/// How the window ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    Closed,
    /// An update was downloaded and checked; `main` finishes it.
    Update,
    /// Settings' Restart Yapfast, for the GPU setting: `main` starts
    /// Yapfast again, which follows the setting.
    Restart,
    /// The GPU failed to start, for this reason: `main` starts Yapfast
    /// again on the CPU, which tells of it.
    UseSoftware(String),
}

/// How the window ended, set as it ends.
static EXIT: std::sync::Mutex<Exit> = std::sync::Mutex::new(Exit::Closed);

pub(crate) fn exit_for(exit: Exit) {
    *EXIT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = exit;
}

/// Which build this is, set once as the window opens.
static BUILD: OnceLock<settings::Build> = OnceLock::new();

pub(crate) fn build() -> settings::Build {
    BUILD.get().cloned().unwrap_or_default()
}

/// Twemoji's pictures as a font, which `build.rs` packs.
pub(crate) const TWEMOJI: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/twemoji.ttf"));

/// The window's size when the private test display doesn't set one.
const SIZE: (u32, u32) = (1120, 760);

/// Opens the window, or starts in the tray, and runs until Yapfast quits.
pub fn run(
    runtime: Arc<BackgroundRuntime>,
    root: PathBuf,
    options: Options,
) -> anyhow::Result<Exit> {
    // The renderer first: each picture is made in its format.
    let plan = graphics::plan(&root, options.renderer, options.gpu_failure);
    renderer::choose(plan.gpu);
    BUILD.set(options.build).ok();
    desktop::install(options.desktop);
    pictures::install();
    video::install();
    frames::install();
    fonts::install(TWEMOJI);
    EXECUTOR
        .set(runtime.block_on(async { tokio::runtime::Handle::current() }))
        .ok();
    // It starts on the preview, a saved session, or the sign-in screen.
    let restore = !options.preview && yapfast_core::chat::can_restore(&root);
    let mut client = Client::new(runtime, root);
    if options.preview {
        client.dispatch(Action::Preview);
    } else if restore {
        client.dispatch(Action::Restore);
    }
    let test_size = test_size();
    let size = test_size.unwrap_or(SIZE);
    // iced calls `boot` once; the client goes into the window then. It runs
    // as a daemon, which goes on without a window: closing to the tray drops
    // the window, its renderer and its widgets.
    let start = shell::Start {
        // A link handed to the start is for someone to see: the window opens.
        background: options.background
            && !options.preview
            && restore
            && options.open_link.is_none(),
        link: options.open_link.filter(|_| !options.preview),
        gpu_failed: plan.failed,
        fixed_size: test_size.is_some(),
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
    tracing::info!(
        background = options.background,
        gpu = plan.gpu,
        "starting the iced window"
    );
    let ran = iced::daemon(boot, App::update, App::view_of)
        .executor::<Exec>()
        .subscription(App::subscription)
        .title(App::title)
        .default_font(fonts::REGULAR)
        .style(App::window_style)
        .theme(|app: &App, _| app.theme())
        .scale_factor(App::scale_factor)
        .run();
    // What the window changed last is written before Yapfast ends.
    layout::flush_all();
    match ran {
        Ok(()) => {}
        // Only the GPU may start when it's chosen (`renderer::choose`): the
        // next start draws on the CPU.
        Err(iced::Error::GraphicsCreationFailed(error)) if plan.gpu => {
            let reason = error.to_string();
            tracing::error!(%reason, "the GPU failed to start; starting again on the CPU");
            return Ok(Exit::UseSoftware(reason));
        }
        Err(error) => anyhow::bail!("the iced window failed: {error}"),
    }
    Ok(EXIT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone())
}

/// The size the private test display asks for.
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
