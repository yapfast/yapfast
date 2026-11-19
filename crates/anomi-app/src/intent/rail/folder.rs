//! A rail folder's menu chosen, and its settings
//! (`project::menus::folder`, `anomi_vm::rail::folder`): the folder moved
//! whole to the top or the bottom of the rail, through the core's
//! `Action::ArrangeServers` as a drag's drop; every folder closed, which is
//! the window's own (`layout.json`); the settings opened with the name's
//! field taking the keys. Done gives the name and colour to the core
//! (`Action::SetFolder`), which shows them at once and writes them with the
//! server list; Escape, the X and a press beside the dialog close it
//! applying nothing, and the colour picker under it closes on its own.

use iced::Task;
use yapfast_core::application::{Action, FOLDER_NAME_LIMIT};

use anomi_vm::rail::arrange;
use anomi_vm::rail::folder::{FolderSettings, Intent};
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;
use crate::project::menus::folder::{BOTTOM, CLOSE_ALL, SETTINGS, TOP};

/// A row of a folder's menu, by its id, about the folder `target`; `None`
/// for a row that isn't one of these (Mark Folder As Read is the core's
/// choice, carried out as a server's).
pub(crate) fn choose(app: &mut App, target: &str, id: &str) -> Option<Task<Message>> {
    match id {
        TOP | BOTTOM => Some(to_end(app, target, id == TOP)),
        SETTINGS => Some(open(app, target)),
        CLOSE_ALL => Some(close_all(app)),
        _ => None,
    }
}

/// The folder whole at the top of the rail, or at its bottom, as Discord's
/// Move… does; nothing where it is already.
fn to_end(app: &mut App, folder: &str, top: bool) -> Task<Message> {
    let groups = arrange::groups(&app.vm.rail);
    match arrange::to_end(&groups, folder, top) {
        Some(groups) => super::arrange_servers(app, groups),
        None => Task::none(),
    }
}

/// Every folder closes, the rail showing so at once.
fn close_all(app: &mut App) -> Task<Message> {
    app.layout.edit(|saved| saved.open_folders.clear());
    app.refresh();
    Task::none()
}

/// The folder's settings open on it as it is, the name's field taking the
/// keys.
fn open(app: &mut App, folder: &str) -> Task<Message> {
    let Some(settings) = crate::project::rail::folder_settings(&app.core.state, folder) else {
        return Task::none();
    };
    app.vm.rail.settings = settings;
    app.vm.shell.open(Layer::FolderSettings);
    anomi_widgets::field::focus(anomi_screens::rail::folder::NAME)
}

/// The dialog's intents.
pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let settings = &mut app.vm.rail.settings;
    match intent {
        Intent::Name(text) => settings.rename(&text, FOLDER_NAME_LIMIT),
        Intent::Colour(colour) => settings.choose(colour),
        Intent::Custom(at) => {
            settings.open_custom();
            app.vm.shell.open_at(Layer::FolderColour, at);
        }
        Intent::Pick(change) => settings.picked(change),
        Intent::Done => {
            let action = applied(&app.vm.rail.settings);
            close(app);
            return app.dispatch(action);
        }
        Intent::Close => close(app),
        _ => {}
    }
    Task::none()
}

/// What Done asks of the core: the folder named as typed, an empty name
/// taking its own away, and coloured as chosen, `None` the default.
pub(crate) fn applied(settings: &FolderSettings) -> Action {
    Action::SetFolder {
        folder: settings.id.clone(),
        name: settings.name.clone(),
        color: settings.colour,
    }
}

/// The dialog closes, and the picker with it, applying nothing.
pub(crate) fn close(app: &mut App) {
    app.vm.shell.close(Layer::FolderColour);
    app.vm.shell.close(Layer::FolderSettings);
}

/// The dialog played its way out: its view-model goes, unless it opened
/// again meanwhile.
pub(crate) fn gone(app: &mut App) {
    if !app.vm.shell.is_open(Layer::FolderSettings) {
        app.vm.rail.settings = FolderSettings::default();
    }
}
