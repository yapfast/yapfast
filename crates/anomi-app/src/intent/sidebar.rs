//! The sidebar's intents and Browse Channels': the DM list's
//! (`src/ui/callbacks.rs:1282`-`:1305`, `:986`), the channel list's
//! (`:2738`-`:3024`, `:3319`), the server header's (`:1358`) and Browse
//! Channels' (`:3044`-`:3058`). The search fields filter the rows without
//! the core; folding a category is the layout's; dragging is in [`drag`].
//! The menus they open are task I10's, a voice person's profile I16's.

mod drag;
#[cfg(test)]
mod tests;

use iced::Task;
use yapfast_core::application::{Action, DmScreen, Navigation};
use yapfast_core::chat::ChannelKind;

use anomi_vm::browse;
use anomi_vm::sidebar::{Intent, channels, dms, server_header};

use crate::app::App;
use crate::message::Message;

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Filter(text) => {
            app.vm.sidebar.filter = text;
            app.refresh();
            Task::none()
        }
        Intent::Dms(intent) => dms(app, intent),
        Intent::Channels(intent) => channels(app, intent),
        Intent::ServerHeader(intent) => header(app, intent),
    }
}

fn dms(app: &mut App, intent: dms::Intent) -> Task<Message> {
    use dms::Intent as I;
    match intent {
        I::Open(room) => app.dispatch(Action::SelectRoom(room)),
        I::Friends => app.dispatch(Action::OpenDmScreen(DmScreen::Friends)),
        I::Requests => app.dispatch(Action::OpenDmScreen(DmScreen::Requests)),
        I::AddFriend => app.dispatch(Action::OpenAddFriend),
        I::NewMessage => app.dispatch(Action::OpenNewDm),
        // Discord's Wave sends a waving hand.
        I::Wave(user) => app.dispatch(Action::MessageFromProfile {
            user,
            text: "\u{1F44B}".into(),
        }),
        I::OpenVoice(room) => app.dispatch(Action::Open(room)),
        I::Seen(ids) => {
            crate::project::dms_seen(&app.core.state, &mut app.shown, ids, &mut app.pictures);
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

fn channels(app: &mut App, intent: channels::Intent) -> Task<Message> {
    use channels::Intent as I;
    match intent {
        // A voice channel's joins it, as its click in Discord does.
        I::Open(room) => app.dispatch(Action::SelectRoom(room)),
        I::OpenChat(room) => app.dispatch(Action::OpenChat(room)),
        I::Browse => app.dispatch(Action::BrowseChannels),
        I::Fold(category) => {
            fold(app, category);
            Task::none()
        }
        I::Create(id) => match category_of(app, &id) {
            Some(place) => app.dispatch(Action::NewChannel {
                place,
                kind: ChannelKind::Text,
            }),
            None => Task::none(),
        },
        I::Watch { channel, person } => app.dispatch(Action::ShowVoicePerson {
            room: channel,
            user: person,
        }),
        I::Arrange(arrange) => drag::arrange(app, arrange),
        I::Move(moving) => drag::moving(app, moving),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

fn header(app: &mut App, intent: server_header::Intent) -> Task<Message> {
    match intent {
        server_header::Intent::Invite => match &app.core.state.navigation {
            Navigation::Server(server) => {
                let server = server.clone();
                app.dispatch(Action::OpenInvite(server))
            }
            Navigation::DirectMessages => Task::none(),
        },
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Folds a category of the server shown, or opens it again, as the layout
/// keeps each server's (`on_toggle_category`).
fn fold(app: &mut App, category: String) {
    let Navigation::Server(server) = app.core.state.navigation.clone() else {
        return;
    };
    app.layout.edit(|saved| {
        let collapsed = saved.collapsed.entry(server.clone()).or_default();
        if !collapsed.remove(&category) {
            collapsed.insert(category);
        }
        if collapsed.is_empty() {
            saved.collapsed.remove(&server);
        }
    });
    app.refresh();
}

/// The category a heading's row stands for: its first channel's, or the
/// category itself for one without channels (`on_category_of`).
fn category_of(app: &App, id: &str) -> Option<String> {
    let core = &app.core;
    core.category_of(id)
        .or_else(|| core.state.is_category(id).then(|| id.to_owned()))
        .filter(|category| !category.is_empty())
}

/// Browse Channels' intents (N14).
pub fn browse(app: &mut App, intent: browse::Intent) -> Task<Message> {
    match intent {
        browse::Intent::Filter(text) => {
            app.vm.browse.filter = text;
            app.refresh();
            Task::none()
        }
        browse::Intent::Follow { room, follow } => {
            app.dispatch(Action::BrowseFollow { room, follow })
        }
        browse::Intent::View(room) => app.dispatch(Action::SelectRoom(room)),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}
