//! The conversation header's intents: its buttons, its person, the way back
//! from a thread, the topic in full and the Friends tabs. The Slint
//! header's callbacks (`ui/yapfast.slint:3138`-`:3464`) and their handlers
//! in `src/ui/callbacks.rs` (`:1039`, `:1071`-`:1090`, `:1283`, `:1638`-
//! `:1672`, `:2770`).
//!
//! The member list's and a DM's profile switches are the layout's, routed to
//! `intent::shell` (I01). A popover a button opens is a layer at the
//! button: it opens here once the core shows what it lists, and closes as
//! the core lets that go (`project::header`), or through [`close`] when
//! Escape or a press outside closes it first.

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::header::Intent;
use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;
use anomi_vm::text::Link;

use crate::app::App;
use crate::message::Message;
use crate::project::header::{add_people_open, pins_open};

/// The header's intents.
pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let state = &app.core.state;
    match intent {
        // Their card opens under the button, its left edge on the button's
        // (`header-card`).
        Intent::Profile(spot) => {
            let Some(user) = state.one_to_one().and_then(|room| room.person.clone()) else {
                return Task::none();
            };
            let task = app.dispatch(Action::ShowProfile { user, large: false });
            app.vm.shell.open_at(Layer::Profile, spot);
            task
        }
        Intent::Parent => match state.selected_room().and_then(|room| room.parent.clone()) {
            Some(parent) => app.dispatch(Action::SelectRoom(parent)),
            None => Task::none(),
        },
        Intent::Topic(spot) => {
            if app.vm.shell.is_open(Layer::Topic) {
                app.vm.shell.close(Layer::Topic);
            } else {
                app.vm.shell.open_at(Layer::Topic, spot);
            }
            Task::none()
        }
        Intent::Close => {
            app.vm.shell.close(Layer::Topic);
            Task::none()
        }
        Intent::Befriend => match state.selected_room().and_then(|room| room.person.clone()) {
            Some(user) => app.dispatch(Action::Befriend(user)),
            None => Task::none(),
        },
        Intent::VoiceChat(chat) => app.dispatch(Action::ShowVoiceChat(chat)),
        Intent::Call => match state.selected.clone() {
            Some(room) => app.dispatch(Action::RequestJoinVoice(room)),
            None => Task::none(),
        },
        Intent::VideoCall => match state.selected.clone() {
            Some(room) => app.dispatch(Action::StartVideoCall(room)),
            None => Task::none(),
        },
        Intent::Threads(spot) => {
            let open = state.thread_list().is_some();
            popover(app, Layer::Threads, spot, Action::ShowThreads(!open))
        }
        Intent::Pins(spot) => {
            let open = pins_open(state);
            popover(app, Layer::Pins, spot, Action::ShowPins(!open))
        }
        Intent::AddPeople(spot) => {
            let action = if add_people_open(state) {
                Action::CloseAddPeople
            } else {
                Action::OpenAddPeople
            };
            popover(app, Layer::AddPeople, spot, action)
        }
        // The Friends screen's tab is the window's own (`friends-all`).
        Intent::FriendsAll(all) => {
            app.vm.people.friends.all = all;
            app.refresh();
            Task::none()
        }
        Intent::AddFriend => app.dispatch(Action::OpenAddFriend),
        Intent::Link(link) => self::link(app, link),
        // The bell's and a thread's menus: `notifications:` and
        // `thread-toolbar:` and the room (I10).
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// A button's popover: the core shows or lets go what it lists, and the
/// layer opens at the button while the core shows it.
fn popover(app: &mut App, layer: Layer, spot: Spot, action: Action) -> Task<Message> {
    let task = app.dispatch(action);
    let state = &app.core.state;
    let shows = match layer {
        Layer::Pins => pins_open(state),
        Layer::Threads => state.thread_list().is_some(),
        _ => add_people_open(state),
    };
    if shows {
        app.vm.shell.open_at(layer, spot);
    }
    task
}

/// Escape or a press outside closed one of the header's popovers: the core
/// lets go what it listed (the popovers' `close`).
pub fn close(app: &mut App, layer: Layer) -> Task<Message> {
    app.vm.shell.close(layer);
    match layer {
        Layer::Pins if pins_open(&app.core.state) => app.dispatch(Action::ShowPins(false)),
        Layer::Threads if app.core.state.thread_list().is_some() => {
            app.dispatch(Action::ShowThreads(false))
        }
        Layer::AddPeople if add_people_open(&app.core.state) => {
            app.dispatch(Action::CloseAddPeople)
        }
        _ => Task::none(),
    }
}

/// A link in the topic in full: a channel opens, a person's card or a
/// role's members show (`open-link`, `src/ui/callbacks.rs:2139`-`:2151`);
/// `@everyone` and `@here` open nothing.
fn link(app: &mut App, link: Link) -> Task<Message> {
    let action = match link {
        // Its chat; a voice channel's doesn't join the call.
        Link::Channel(room) => {
            app.vm.shell.close(Layer::Topic);
            Action::Open(room)
        }
        Link::User(user) => Action::ShowProfile { user, large: false },
        Link::Role(role) => Action::ShowRole(role),
        Link::Everyone => return Task::none(),
        // A web address, a masked link and an emoji's card (I15).
        link => {
            tracing::debug!(?link, "not handled by the iced interface yet");
            return Task::none();
        }
    };
    app.dispatch(action)
}

#[cfg(test)]
mod tests;
