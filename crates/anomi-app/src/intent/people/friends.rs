//! Friends, Requests, Add Friend and Active Now (`befriend`,
//! `remove-friend-request`, `message-person`, `open-conversation`,
//! `friend-filter-edited`, `add-friend`): accepting, ignoring and sending
//! go to the core, which answers with the lists again; the screens follow
//! its count of people changes.

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::people::{active_now, add_friend, friends, requests};
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;

pub(super) fn friends(app: &mut App, intent: friends::Intent) -> Task<Message> {
    use friends::Intent as I;
    match intent {
        I::Message(user) => app.dispatch(Action::MessagePerson(user)),
        I::OpenVoice(room) => app.dispatch(Action::Open(room)),
        I::Menu(user, spot) => super::super::menus::open(app, user, true, spot),
        I::Filter(filter) => {
            app.vm.people.friends.filter = filter;
            app.refresh();
            Task::none()
        }
        I::AddFriend => app.dispatch(Action::OpenAddFriend),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

pub(super) fn requests(app: &mut App, intent: requests::Intent) -> Task<Message> {
    use requests::Intent as I;
    match intent {
        I::Accept(user) => app.dispatch(Action::Befriend(user)),
        I::Remove(user) => app.dispatch(Action::RemoveFriendRequest(user)),
        I::Open(room) => app.dispatch(Action::Open(room)),
        I::ToggleSpam => {
            let requests = &mut app.vm.people.requests;
            requests.spam_open = !requests.spam_open;
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

pub(super) fn add_friend(app: &mut App, intent: add_friend::Intent) -> Task<Message> {
    use add_friend::Intent as I;
    match intent {
        I::Name(name) => {
            app.vm.people.add_friend.name = name;
            Task::none()
        }
        I::Send(name) if app.vm.people.add_friend.may_send() => {
            app.dispatch(Action::AddFriend(name.trim().to_owned()))
        }
        I::Close => close(app),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Add Friend closes: the core lets it go, and the projection closes it.
pub(crate) fn close(app: &mut App) -> Task<Message> {
    app.vm.shell.close(Layer::AddFriend);
    if app.core.state.add_friend.is_some() {
        app.dispatch(Action::CloseAddFriend)
    } else {
        Task::none()
    }
}

pub(super) fn active_now(app: &mut App, intent: active_now::Intent) -> Task<Message> {
    use active_now::Intent as I;
    match intent {
        I::OpenRoom(room) => app.dispatch(Action::Open(room)),
        I::OpenPerson(user, spot) => super::super::menus::open(app, user, true, spot),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}
