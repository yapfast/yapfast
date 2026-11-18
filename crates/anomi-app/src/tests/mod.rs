//! The window driven through `iced_test`'s headless simulator, on the real
//! core: each test builds the window's view, clicks and types into it as a
//! person would, and hands the messages that came out to `App::update`, as
//! iced's runtime does. The tasks `update` returns (focus, timers, the
//! linking's progress) don't run here, and the simulator keeps a widget's
//! state only while it lives, so a field is clicked in the same
//! simulator it is typed into. Tests choose no language: text is the
//! code's English. One module per area.

mod editor;
mod links;
mod notify;
mod sign_in;
mod timeline;

use std::path::{Path, PathBuf};

use iced::{Settings, Size, window};
use iced_test::Simulator;
use yapfast_core::application::{Action, MfaPrompt};
use yapfast_core::chat::MfaMethod;

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, fonts, typography};

use crate::app::App;
use crate::message::Message;

/// The window's size in the tests, the golden pictures' (1400x860).
pub(crate) const SIZE: Size = Size::new(1400.0, 860.0);

/// A data folder of a test's own, gone with it.
pub(crate) struct Folder(PathBuf);

impl Folder {
    pub(crate) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("anomi-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An app on a data folder of its own, which goes after the app wrote its
/// layout there.
pub(crate) struct Window {
    pub(crate) app: App,
    _folder: Folder,
}

/// The window on a data folder of its own, shown and in front: the
/// preview, or else the sign-in screen of an Anomi with no account yet.
pub(crate) fn start(name: &str, preview: bool) -> Window {
    let folder = Folder::new(name);
    let mut app = App::for_tests(folder.path());
    if preview {
        drop(app.dispatch(Action::Preview));
    }
    app.motion.front = true;
    app.refresh();
    Window {
        app,
        _folder: folder,
    }
}

/// A prompt Discord's hold of a request asks, at `method` of `methods`.
pub(crate) fn mfa_prompt(id: u64, methods: Vec<MfaMethod>, method: Option<MfaMethod>) -> MfaPrompt {
    MfaPrompt {
        id,
        methods,
        method,
        code: String::new(),
        phone: None,
        busy: false,
        error: None,
        warned: false,
    }
}

/// The simulator's settings: the window's fonts, its text face.
fn settings() -> Settings {
    fonts::install();
    Settings {
        font: typography::UI,
        ..Settings::default()
    }
}

/// Builds the window's view, lets `act` work on it, and applies what came
/// out, as iced would.
pub(crate) fn interact(
    app: &mut App,
    act: impl FnOnce(&mut Simulator<'_, Message, Theme, Renderer>),
) {
    let messages: Vec<Message> = {
        let view = app.view(window::Id::unique()).boxed();
        let mut ui = Simulator::with_size(settings(), SIZE, view);
        act(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages {
        drop(app.update(message));
    }
}

/// Whether the window shows `text` anywhere.
pub(crate) fn shows(app: &App, text: &str) -> bool {
    let view = app.view(window::Id::unique()).boxed();
    let mut ui = Simulator::with_size(settings(), SIZE, view);
    ui.find(text).is_ok()
}

/// A widget by its id, as the screens name their buttons and fields.
pub(crate) fn id(
    name: &'static str,
) -> impl iced_test::Selector<Output = iced_test::selector::Target> {
    iced_test::selector::id(iced::widget::Id::new(name))
}
