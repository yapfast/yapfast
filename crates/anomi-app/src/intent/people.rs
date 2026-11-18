//! People's intents: friends, requests, Active Now, the member list and its
//! Activity, profiles and their cards, the dialogs about a person, the
//! friends picker (`src/ui/callbacks.rs:979`-`:1062`, `:1237`-`:1262`,
//! `:1383`-`:1412`, `:1804`, `:3037`). Status's and the profile editor's are
//! their own modules'.

mod cards;
mod dialogs;
mod friends;
mod profile;
#[cfg(test)]
mod tests;

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::menu::Spot;
use anomi_vm::people::{self, activity, badges, members};
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;

pub(crate) use cards::tag_card;
pub(crate) use profile::open_card;

/// People's intents, but status's and the editor's.
pub fn handle(app: &mut App, intent: people::Intent) -> Task<Message> {
    use people::Intent as P;
    match intent {
        P::Friends(intent) => friends::friends(app, intent),
        P::Requests(intent) => friends::requests(app, intent),
        P::AddFriend(intent) => friends::add_friend(app, intent),
        P::ActiveNow(intent) => friends::active_now(app, intent),
        P::Members(intent) => self::members(app, intent),
        P::Activity(intent) => self::activity(app, intent),
        P::Profile(intent) => profile::handle(app, intent),
        P::FullProfile(intent) => profile::full(app, intent),
        P::Badges(badges::Intent::OpenLink(url)) => {
            crate::platform::links::follow(app, &url, false)
        }
        P::Person(intent) => dialogs::person(app, intent),
        P::AddPeople(intent) => dialogs::add_people(app, intent),
        P::Cards(intent) => cards::handle(app, intent),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The member list: a member's card beside the list, the rows in view.
fn members(app: &mut App, intent: members::Intent) -> Task<Message> {
    match intent {
        members::Intent::Open(user, spot) => open_card(app, user, spot),
        members::Intent::Shown(first, last) => {
            let state = &app.core.state;
            crate::project::people::members_seen(
                state,
                &mut app.shown.people,
                first,
                last,
                &mut app.pictures,
            );
            // Asked for as the update ends (`App::update`).
            Task::none()
        }
        members::Intent::Pointed(user) => {
            crate::project::people::members_pointed(&mut app.shown.people, user);
            app.show(crate::project::Reach::MEMBERS);
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The Activity panel above the member list.
fn activity(app: &mut App, intent: activity::Intent) -> Task<Message> {
    match intent {
        activity::Intent::Expand(open) => {
            app.vm.people.activity.open = open;
            Task::none()
        }
        activity::Intent::Hide(hidden) => {
            if !hidden {
                app.vm.people.activity.open = false;
            }
            app.dispatch(Action::ActivityCardsHidden(hidden))
        }
        activity::Intent::Profile(user, bounds) => open_card(app, user, Spot::beside(bounds)),
        activity::Intent::Card(intent) => profile::activity_card(app, intent, false),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Escape, a press outside or a close button closed a layer about people:
/// the core lets go of what it showed, and the projection follows.
pub(crate) fn close(app: &mut App, layer: Layer) -> Task<Message> {
    match layer {
        Layer::Profile | Layer::FullProfile => profile::close(app, layer),
        Layer::Person => dialogs::close(app),
        Layer::AddFriend => friends::close(app),
        Layer::RoleCard => {
            app.vm.shell.close(layer);
            if app.core.state.role.is_some() {
                app.dispatch(Action::CloseRole)
            } else {
                Task::none()
            }
        }
        layer => {
            app.vm.shell.close(layer);
            Task::none()
        }
    }
}
