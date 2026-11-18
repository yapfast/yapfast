//! The profile cards' and the full profile's intents (`enlarge-profile`,
//! `close-profile`, `message-from-profile`, `save-note`, `befriend`,
//! `profile-tab-opened`, `show-friend-profile`, `enlarge-panel-profile`):
//! one set for the popout, the panel beside a DM and the full profile's
//! side, each intent naming its place. A note is saved as its box lets go
//! of the keys, on Enter, and on every way the card closes, enlarges or
//! messages, as iced tells nothing of a box that leaves the screen.

use iced::Task;
use yapfast_core::application::{Action, ProfileTab};

use anomi_vm::menu::{Menu, Row, Spot};
use anomi_vm::people::activity::card;
use anomi_vm::people::full_profile::{self, Page};
use anomi_vm::people::profile::{Card, Intent, Place};
use anomi_vm::settings::frame::Page as SettingsPage;
use anomi_vm::shell::Layer;
use yapfast_lang::tr;

use crate::app::App;
use crate::message::Message;
use crate::project::menus::subs::PROFILE_MORE;

/// Someone's card beside what was clicked: the core shows their profile,
/// and the popout opens at `spot` while it does.
pub(crate) fn open_card(app: &mut App, user: String, spot: Spot) -> Task<Message> {
    if user.is_empty() {
        return Task::none();
    }
    let task = save_notes(app);
    let task = Task::batch([
        task,
        app.dispatch(Action::ShowProfile { user, large: false }),
    ]);
    if app
        .core
        .state
        .profile
        .as_ref()
        .is_some_and(|view| !view.large)
    {
        app.vm.shell.open_at(Layer::Profile, spot);
    }
    task
}

/// The card of `place`, to change what's done on it.
fn card(app: &mut App, place: Place) -> Option<&mut Card> {
    let people = &mut app.vm.people;
    match place {
        Place::Popout => Some(&mut people.profile.card),
        Place::Panel => people.profile.panel.as_mut(),
        Place::Side => Some(&mut people.full_profile.card),
        Place::Preview => None,
    }
}

/// Saves the note typed on `place`'s card, when it changed.
fn save_note(app: &mut App, place: Place) -> Task<Message> {
    let Some(card) = card(app, place) else {
        return Task::none();
    };
    let (Some(typed), Some(saved)) = (&card.touch.note, &card.note) else {
        return Task::none();
    };
    if typed == saved || card.id.is_empty() {
        return Task::none();
    }
    let (user, note) = (card.id.clone(), typed.clone());
    card.note = Some(typed.clone());
    app.dispatch(Action::SaveNote { user, note })
}

/// Saves what's typed in every card's note.
fn save_notes(app: &mut App) -> Task<Message> {
    let tasks = [Place::Popout, Place::Side, Place::Panel].map(|place| save_note(app, place));
    Task::batch(tasks)
}

/// Escape, a click outside or the close button: the note is saved and the
/// core lets the profile go; the projection closes it.
pub(crate) fn close(app: &mut App, layer: Layer) -> Task<Message> {
    app.vm.shell.close(layer);
    let task = save_notes(app);
    Task::batch([task, app.dispatch(Action::CloseProfile)])
}

/// The profile card's intents.
pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Enlarge(place) => enlarge(app, place, None),
        Intent::EnlargeOn(place, tab) => enlarge(app, place, Some(tab.into())),
        Intent::Befriend(user) => app.dispatch(Action::Befriend(user)),
        Intent::More(user, spot) => {
            super::super::menus::open(app, format!("{PROFILE_MORE}{user}"), true, spot)
        }
        Intent::Message(user) => {
            let task = save_notes(app);
            Task::batch([task, app.dispatch(Action::MessagePerson(user))])
        }
        Intent::Draft(place, text) => {
            if let Some(card) = card(app, place) {
                card.touch.message = text;
            }
            Task::none()
        }
        Intent::Send(place) => {
            let Some(card) = card(app, place) else {
                return Task::none();
            };
            let text = std::mem::take(&mut card.touch.message);
            if text.trim().is_empty() {
                return Task::none();
            }
            let user = card.id.clone();
            let task = save_notes(app);
            Task::batch([
                task,
                app.dispatch(Action::MessageFromProfile { user, text }),
            ])
        }
        Intent::Note(place, text) => {
            if let Some(card) = card(app, place) {
                card.touch.note = Some(text);
            }
            Task::none()
        }
        Intent::SaveNote(place) => save_note(app, place),
        Intent::NoteFocus(place, focused) => {
            if let Some(card) = card(app, place) {
                card.touch.note_focused = focused;
            }
            if focused {
                Task::none()
            } else {
                save_note(app, place)
            }
        }
        Intent::EditProfile => {
            let task = close(app, Layer::Profile);
            Task::batch([task, app.dispatch(Action::EditProfile)])
        }
        Intent::OpenLink(url) => crate::platform::links::follow(app, &url, false),
        Intent::Link(link) => super::super::header::link(app, link),
        Intent::Tag(server, bounds) => {
            let tag = [Place::Popout, Place::Side, Place::Panel]
                .into_iter()
                .find_map(|place| {
                    card(app, place)?
                        .tag
                        .clone()
                        .filter(|tag| tag.server == server)
                });
            match tag {
                Some(tag) => super::tag_card(app, tag, Spot::beside(bounds)),
                None => Task::none(),
            }
        }
        Intent::Activity { intent, privacy } => activity_card(app, intent, privacy),
        Intent::Pointed(place, on) => touch(app, place, |card| card.touch.pointed = on),
        Intent::Bubble(place, on) => touch(app, place, |card| card.touch.bubble = on),
        Intent::AllBadges(place) => touch(app, place, |card| card.touch.all_badges = true),
        Intent::EffectWanted(place) => {
            let state = &app.core.state;
            let shown = &mut app.shown.people.profile;
            crate::project::people::profile::effect_wanted(
                state,
                place == Place::Panel,
                shown,
                &mut app.pictures,
            );
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Changes what's done on `place`'s card, which the window alone shows.
fn touch(app: &mut App, place: Place, change: impl FnOnce(&mut Card)) -> Task<Message> {
    if let Some(card) = card(app, place) {
        change(card);
    }
    Task::none()
}

/// The full profile: the popout's, or the panel's person's in their DM;
/// on `page` when given, else on its first.
fn enlarge(app: &mut App, place: Place, page: Option<Page>) -> Task<Message> {
    let task = save_notes(app);
    app.vm.people.full_profile.chosen = page;
    let action = match place {
        Place::Popout => Action::EnlargeProfile,
        Place::Panel => match app.core.state.selected.clone() {
            Some(room) => Action::ShowDmProfile(room),
            None => return task,
        },
        Place::Side | Place::Preview => return task,
    };
    Task::batch([task, app.dispatch(action)])
}

/// After each update: the core hears which tab of the full profile shows,
/// once per tab and person, as Slint's `tab-opened` (`ui/people.slint`), so
/// the tab it opens on asks Discord for what it lists as a chosen one does.
pub(crate) fn after(app: &mut App) -> Task<Message> {
    if !app.vm.shell.is_open(Layer::FullProfile) {
        app.shown.people.profile.told = None;
        return Task::none();
    }
    let full = &app.vm.people.full_profile;
    let (person, page) = (&full.card.id, full.page());
    let told = &mut app.shown.people.profile.told;
    if person.is_empty()
        || told
            .as_ref()
            .is_some_and(|told| (&told.0, told.1) == (person, page))
    {
        return Task::none();
    }
    *told = Some((person.clone(), page));
    // A tab asks Discord for what it lists the first time.
    let tab = match page.asks() {
        Some(full_profile::Asks::Activity) => ProfileTab::Activity,
        Some(full_profile::Asks::Board) => ProfileTab::Board,
        Some(full_profile::Asks::Wishlist) => ProfileTab::Wishlist,
        None => return Task::none(),
    };
    app.dispatch(Action::OpenProfileTab(tab))
}

/// The full profile's intents.
pub(super) fn full(app: &mut App, intent: full_profile::Intent) -> Task<Message> {
    use full_profile::Intent as I;
    match intent {
        I::Close => close(app, Layer::FullProfile),
        // The core hears of the tab after the update (`after`).
        I::Choose(page) => {
            app.vm.people.full_profile.choose(page);
            Task::none()
        }
        I::OpenServer(server, bounds) => super::cards::server_card(app, &server, bounds),
        I::OpenFriend(user) => {
            let task = save_notes(app);
            app.vm.people.full_profile.chosen = None;
            Task::batch([
                task,
                app.dispatch(Action::ShowFriendProfile { user, large: true }),
            ])
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// An activity card's link, and its menu: View All Activity, and on one's
/// own game Manage Privacy (`activity-menu`, `ui/yapfast.slint:2377`).
pub(super) fn activity_card(app: &mut App, intent: card::Intent, privacy: bool) -> Task<Message> {
    use anomi_vm::Intent as Window;
    use anomi_vm::people::Intent as People;
    let row = |text: &str, intent: card::Intent| {
        Row::new(text, Window::People(People::Activity(intent.into())))
    };
    match intent {
        card::Intent::Link(url) => crate::platform::links::follow(app, &url, false),
        card::Intent::More(spot) => {
            let mut rows = vec![row(tr!("View All Activity"), card::Intent::ViewAll)];
            if privacy {
                rows.push(row(tr!("Manage Privacy"), card::Intent::Privacy));
            }
            super::super::menus::show(app, Menu::new(0, spot, rows));
            Task::none()
        }
        card::Intent::ViewAll => {
            let place = if app.core.state.profile.is_some() {
                Place::Popout
            } else {
                Place::Panel
            };
            enlarge(app, place, Some(Page::Activity))
        }
        card::Intent::Privacy => {
            let task = close(app, Layer::Profile);
            app.vm.shell.close(Layer::FullProfile);
            Task::batch([
                task,
                crate::intent::settings::open(app, SettingsPage::Games),
            ])
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}
