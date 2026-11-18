//! A member's card opens beside the list and closes through the core; what
//! is done on it stays while the same person shows; a note is saved as its
//! box lets go of the keys.

use std::sync::Arc;

use yapfast_core::application::Action;
use yapfast_core::chat::Presence;

use anomi_vm::menu::Spot;
use anomi_vm::people::Intent as People;
use anomi_vm::people::members::Intent as Members;
use anomi_vm::people::profile::{Intent, Place};
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::project::people::tests::member;

fn app(name: &str) -> (App, std::path::PathBuf) {
    let folder = std::env::temp_dir().join(format!("anomi-cards-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut app = App::for_tests(&folder);
    drop(app.core.dispatch(Action::Preview));
    let room = app
        .core
        .state
        .rooms
        .iter()
        .find(|room| !room.direct && !room.voice && !room.forum)
        .map(|room| room.id.clone())
        .expect("a channel");
    app.core.state.selected = Some(room);
    let list = vec![member("1", "Ann", Presence::Online, "Online — 1")];
    app.core.state.members = Some(Ok(Arc::new(list)));
    app.refresh();
    (app, folder)
}

fn done(app: App, folder: std::path::PathBuf) {
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

fn ask(app: &mut App, intent: People) {
    drop(super::handle(app, intent));
}

#[test]
fn a_members_card_opens_beside_the_list_and_closes_through_the_core() {
    let (mut app, folder) = app("open");
    let spot = Spot::pointer(iced::Point::new(1000.0, 200.0));
    ask(&mut app, People::Members(Members::Open("1".into(), spot)));
    assert!(app.core.state.profile.is_some());
    let opened = app.vm.shell.opened(Layer::Profile).expect("the popout");
    assert_eq!(opened.at, Some(spot));
    assert_eq!(app.vm.people.profile.card.name, "Ann");
    assert_eq!(app.vm.people.members.selected, "1");
    // The pointer on it stays as the profile comes again.
    ask(
        &mut app,
        People::Profile(Intent::Pointed(Place::Popout, true)),
    );
    app.core.state.people_changed();
    app.refresh();
    assert!(app.vm.people.profile.card.touch.pointed);
    // Escape: the core lets it go, the card closes.
    drop(crate::intent::shell::handle(
        &mut app,
        anomi_vm::shell::Intent::Close(Layer::Profile),
    ));
    assert!(app.core.state.profile.is_none());
    assert!(!app.vm.shell.is_open(Layer::Profile));
    assert!(app.vm.people.members.selected.is_empty());
    done(app, folder);
}

#[test]
fn going_large_shows_the_full_profile_in_place_of_the_popout() {
    let (mut app, folder) = app("large");
    ask(
        &mut app,
        People::Members(Members::Open("1".into(), Spot::default())),
    );
    ask(&mut app, People::Profile(Intent::Enlarge(Place::Popout)));
    assert!(
        app.core
            .state
            .profile
            .as_ref()
            .is_some_and(|view| view.large)
    );
    assert!(app.vm.shell.is_open(Layer::FullProfile));
    assert!(!app.vm.shell.is_open(Layer::Profile));
    assert_eq!(app.vm.people.full_profile.card.name, "Ann");
    drop(super::close(&mut app, Layer::FullProfile));
    assert!(!app.vm.shell.is_open(Layer::FullProfile));
    done(app, folder);
}

#[test]
fn a_note_is_saved_as_its_box_lets_go() {
    let (mut app, folder) = app("note");
    // Notes are Discord's: a linked account keeps them.
    app.core.state.discord_user = Some("100".into());
    ask(
        &mut app,
        People::Members(Members::Open("1".into(), Spot::default())),
    );
    ask(
        &mut app,
        People::Profile(Intent::Note(Place::Popout, "met at the con".into())),
    );
    ask(
        &mut app,
        People::Profile(Intent::NoteFocus(Place::Popout, false)),
    );
    let note = app
        .core
        .state
        .profile
        .as_ref()
        .map(|view| view.profile.note.clone());
    assert_eq!(note.as_deref(), Some("met at the con"));
    done(app, folder);
}

/// A full profile opened at once, as a call's participant menu opens it,
/// tells the core of the tab it opens on, which then asks Discord for it.
#[test]
fn the_tab_a_full_profile_opens_on_asks_for_what_it_lists() {
    let (mut app, folder) = app("tab");
    drop(app.dispatch(Action::ShowProfile {
        user: "1".into(),
        large: true,
    }));
    assert!(app.vm.shell.is_open(Layer::FullProfile));
    let opened = |app: &App| {
        app.core
            .state
            .profile
            .as_ref()
            .is_some_and(|view| view.activity_open)
    };
    assert!(!opened(&app));
    drop(super::profile::after(&mut app));
    assert!(opened(&app));
    assert!(app.shown.people.profile.told.is_some());
    drop(super::close(&mut app, Layer::FullProfile));
    drop(super::profile::after(&mut app));
    assert!(app.shown.people.profile.told.is_none());
    done(app, folder);
}
