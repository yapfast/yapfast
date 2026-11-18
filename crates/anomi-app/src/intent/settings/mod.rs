//! The person's settings' intents (task I19): each page's switches kept in
//! the layout as the Slint app keeps them (`on_preferences_changed`,
//! `src/ui/callbacks.rs:2303`) and handed to the core or the program where
//! the Slint app hands them, at once; the themes, the theme editor, the
//! language and the fonts (`src/ui/appearance.rs`, `language.rs`); the voice
//! gate's meter and the microphone test while Voice & video shows in front.
//!
//! Settings opens through [`open`], which fills it, and closes through
//! [`close`] (its Close button, Escape, the window going): a theme being
//! edited goes back to the one chosen, a keybind being recorded stops, the
//! meter and the test stop, linking started there stops, and once it played
//! its way out ([`gone`]) its pages hold nothing.

mod appearance;
pub(crate) mod camera;
mod editor;
mod frame;
#[cfg(feature = "discord")]
pub(crate) mod gate;
mod prefs;
mod start;
#[cfg(test)]
mod tests;
mod voice;

use std::path::PathBuf;

use iced::Task;

use anomi_vm::settings::{self, frame::Page};
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::{Message, Once};
use crate::platform::keybinds::record;
use crate::project::Reach;

pub use appearance::choose_theme;
pub use start::start;

/// What Settings' work off the window's thread, or a wait of its own,
/// brought back.
#[derive(Clone, Debug)]
pub enum Due {
    /// The build was copied a while ago, by the copy's number.
    Copied(u64),
    /// Font files were chosen, and copied in.
    FontsChosen(Vec<PathBuf>),
    FontsCopied(Once<Vec<appearance::Imported>>),
    /// The windows drew the new font: their background moves, so every
    /// pixel is drawn again.
    Repainted,
    /// A theme file was chosen, read, or written.
    ThemeChosen(PathBuf),
    ThemeRead(Once<Result<anomi_theme::custom::Imported, anomi_theme::custom::ImportError>>),
    ThemeWritten(String, Result<(), String>),
    /// What the voice gate's meter heard that the page says in words.
    #[cfg(feature = "discord")]
    Gate(gate::Heard),
}

/// The person's settings' intents.
pub fn handle(app: &mut App, intent: settings::Intent) -> Task<Message> {
    use settings::Intent as I;
    match intent {
        I::Frame(intent) => frame::frame(app, intent),
        I::Account(intent) => frame::account(app, intent),
        I::Voice(intent) => voice::handle(app, intent),
        I::Messages(intent) => prefs::messages(app, intent),
        I::Notifications(intent) => prefs::notifications(app, intent),
        I::Sounds(intent) => prefs::sounds(app, intent),
        I::Appearance(intent) => appearance::handle(app, intent),
        I::ThemeEditor(intent) => editor::handle(app, intent),
        I::Discord(intent) => prefs::discord(app, intent),
        I::Games(intent) => prefs::games(app, intent),
        I::Keybinds(intent) => keybinds(app, intent),
        I::Advanced(intent) => prefs::advanced(app, intent),
        I::About(intent) => prefs::about(app, intent),
        intent => {
            tracing::debug!(?intent, "not the person's settings'");
            Task::none()
        }
    }
}

/// What came back, handled by the part that asked.
pub fn due(app: &mut App, due: Due) -> Task<Message> {
    match due {
        Due::Copied(copy) => frame::copied_gone(app, copy),
        Due::FontsChosen(paths) => appearance::import_fonts(app, paths),
        Due::FontsCopied(imported) => {
            appearance::add_fonts(app, imported.take().unwrap_or_default())
        }
        Due::Repainted => {
            let kept = &mut app.shown.settings.kept;
            kept.repaint = kept.repaint.next();
            Task::none()
        }
        Due::ThemeChosen(path) => appearance::read_theme(path),
        Due::ThemeRead(read) => match read.take() {
            Some(read) => appearance::import_theme(app, read),
            None => Task::none(),
        },
        Due::ThemeWritten(name, written) => {
            editor::exported(app, &name, written);
            Task::none()
        }
        #[cfg(feature = "discord")]
        Due::Gate(heard) => {
            gate::heard(app, heard);
            Task::none()
        }
    }
}

/// Opens Settings on `page`, filled: the cog and the account menu's User
/// Settings, a device menu's Voice Settings, a profile's privacy link.
pub fn open(app: &mut App, page: Page) -> Task<Message> {
    app.vm.settings.frame.page = page;
    app.vm.shell.open(Layer::Settings);
    app.shown.settings.kept.tray = app.desktop.tray_shows();
    entered(app, page)
}

/// A page shows: Voice & video lists the devices again, as Slint's did as
/// it opened (`ui/settings.slint:640`), and it and Keybinds show the
/// keybinds as they are.
fn entered(app: &mut App, page: Page) -> Task<Message> {
    app.show(Reach::SETTINGS);
    match page {
        Page::Voice => {
            record::show(app);
            app.dispatch(yapfast_core::application::Action::Voice(
                yapfast_core::voice::Command::Devices,
            ))
        }
        Page::Keybinds => {
            record::show(app);
            Task::none()
        }
        _ => Task::none(),
    }
}

/// Settings closes: its Close button, Escape, the window going.
pub fn close(app: &mut App) -> Task<Message> {
    let page = app.vm.settings.frame.page;
    left(app, page);
    let task = crate::intent::sign_in::settings_closed(app);
    app.vm.shell.close(Layer::Settings);
    task
}

/// Settings played its way out: its pages hold nothing until it opens.
pub fn gone(app: &mut App) {
    crate::project::settings_gone(&mut app.vm, &mut app.shown);
}

/// A page is left, for another or as Settings closes: what it started ends
/// there, and its NEW tags have been seen.
fn left(app: &mut App, page: Page) {
    if app.desktop.keybinds.recording.is_some() {
        record::record(app, None);
    }
    let kept = &mut app.shown.settings.kept;
    if page == Page::Appearance {
        kept.note = None;
        kept.refused.clear();
        kept.changed();
        editor::cancel(app);
    }
    app.vm.settings.voice.microphone_testing = false;
    app.vm.settings.voice.entrance_open = false;
    if page == Page::About {
        let seen = &app.layout.seen_new;
        let mut after = seen.clone();
        if yapfast_core::new_tags::leave(&mut after, yapfast_core::new_tags::Page::About) {
            app.layout.edit(|saved| saved.seen_new = after);
        }
    }
}

/// After each update: the camera's test, the voice gate's meter and the
/// microphone test run while Voice & video shows in front, and stop
/// otherwise.
pub fn after(app: &mut App) -> Task<Message> {
    camera::follow(app);
    #[cfg(feature = "discord")]
    return gate::follow(app);
    #[cfg(not(feature = "discord"))]
    {
        let _ = app;
        Task::none()
    }
}

/// The Keybinds page's intents, on the keybinds' own (I18b).
fn keybinds(app: &mut App, intent: settings::keybinds::Intent) -> Task<Message> {
    use settings::keybinds::Intent as I;
    match intent {
        I::Add => record::add(app),
        I::Remove(row) => record::remove(app, row),
        I::Action(row, action) => record::choose(app, row, action),
        I::Record(row, on) => record::record(app, on.then_some(record::Target::Row(row))),
        intent => tracing::debug!(?intent, "not the keybinds'"),
    }
    Task::none()
}
