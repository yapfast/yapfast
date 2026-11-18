//! Menus' intents: the window's menu (a server's, a folder's, a DM's, a
//! channel's, a category's, a person's, the server header's dropdown and
//! its events, the bell's and a thread's More), opened where the Slint
//! window's `open-context-menu` opens it, with its rows built as it opens
//! (`prepare-menu`, `src/ui/callbacks.rs:1195`) and its choices carried
//! out (`choose-menu`, `:1222`); and the message menu ([`message`]).
//!
//! A closed menu costs nothing: its rows go once it played its way out.

mod choose;
pub(crate) mod message;
#[cfg(test)]
mod tests;

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::menu::{self, Menu, Spot};
use anomi_vm::shell::Layer;
use anomi_vm::voice::participant_menu::{self as card, ParticipantMenu};

use crate::app::App;
use crate::message::Message;
use crate::project::menus::subs::IN_VOICE;
use crate::project::menus::{self as rows, header_open};

/// The window's menu's intents.
pub fn handle(app: &mut App, intent: menu::Intent) -> Task<Message> {
    match intent {
        menu::Intent::Close => {
            close(app);
            Task::none()
        }
        // A menu opened since keeps its rows.
        menu::Intent::Gone => {
            if !app.vm.menu.open {
                app.vm.menu = Menu::default();
                app.shown.menus.target.clear();
                app.shown.menus.apps_due = false;
            }
            Task::none()
        }
        menu::Intent::Choose(choice) => choose::choose(app, choice),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The message menu's: `chat::Intent::Menu`.
pub fn message(app: &mut App, intent: anomi_vm::chat::Intent) -> Task<Message> {
    match intent {
        anomi_vm::chat::Intent::Menu(intent) => message::handle(app, intent),
        intent => {
            tracing::debug!(?intent, "not the message menu's");
            Task::none()
        }
    }
}

/// Opens the menu about `target` (a server, a folder, a conversation, a
/// person, or one of `project::menus::subs`' prefixed targets) at `spot`;
/// nothing when it offers nothing. A person's asks for the open room's
/// apps, which go in as they come.
pub(crate) fn open(app: &mut App, target: String, person: bool, spot: Spot) -> Task<Message> {
    let ask = if person {
        app.dispatch(Action::AskAppCommands)
    } else {
        Task::none()
    };
    let rows = rows::rows(&app.core, &app.layout, &target, person);
    if rows.is_empty() {
        return ask;
    }
    put(app, target, person, Menu::new(0, spot, rows));
    ask
}

/// Shows `menu`, whose rows carry their own intents, as the window's: the
/// composer's plus (I11).
pub(crate) fn show(app: &mut App, menu: Menu) {
    put(app, String::new(), false, menu);
}

/// Shows `menu` as the window's, about `target`.
fn put(app: &mut App, target: String, person: bool, mut menu: Menu) {
    let state = &app.core.state;
    let shown = &mut app.shown.menus;
    shown.serial += 1;
    menu.serial = shown.serial;
    shown.apps_due = person
        && state
            .selected
            .as_deref()
            .is_some_and(|room| state.app_commands.index(room).is_none());
    shown.target = target;
    shown.person = person;
    app.vm.menu = menu;
    header_open(&mut app.vm, &app.shown.menus);
}

/// Someone's menu under a voice channel, at the pointer: their name and
/// volumes on top, unless it's this account, then what their menu offers
/// there (`open-voice-menu`, `ui/yapfast.slint:1611`).
pub(crate) fn open_voice(app: &mut App, room: String, user: String, at: Spot) -> Task<Message> {
    let target = format!("{IN_VOICE}{room}/{user}");
    let rows = rows::rows(&app.core, &app.layout, &target, true);
    let local = app.core.state.discord_user.as_deref() == Some(user.as_str());
    let found =
        (app.core.voice_person(&room, &user)).map(|person| (person.name.clone(), person.streaming));
    let Some((name, live)) = found.filter(|_| !local) else {
        if !rows.is_empty() {
            put(app, target, true, Menu::new(0, at, rows));
        }
        return Task::none();
    };
    let size = anomi_widgets::menu::size(&rows, card::WIDTH);
    let volume = |kind| {
        let volume = yapfast_core::volume::get(kind, &user);
        yapfast_core::volume::to_slider(volume).round() as i32
    };
    let made = ParticipantMenu {
        name,
        live,
        voice_volume: volume(yapfast_core::volume::Kind::Voice),
        stream_volume: volume(yapfast_core::volume::Kind::Stream),
        joined: !rows.is_empty(),
        width: size.width.max(card::WIDTH),
        person: user,
    };
    let pointer = at.anchor.position();
    let (top, below) = made.place(pointer, size, app.size);
    app.vm.voice.participant_menu = made;
    app.vm
        .shell
        .open_at(Layer::ParticipantMenu, Spot::pointer(top));
    if !rows.is_empty() {
        let menu = Menu {
            narrowest: Some(card::WIDTH),
            headed: true,
            ..Menu::new(0, Spot::pointer(below), rows)
        };
        put(app, target, true, menu);
    }
    Task::none()
}

/// The window's menu closes, playing its way out, with the card over it.
pub(crate) fn close(app: &mut App) {
    app.vm.menu.open = false;
    if app.vm.shell.is_open(Layer::ParticipantMenu) {
        app.vm.shell.close(Layer::ParticipantMenu);
    }
    header_open(&mut app.vm, &app.shown.menus);
}
