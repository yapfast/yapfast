//! The header's buttons on the core's preview, its DM Sam given Discord's
//! abilities: each asks the core, and a popover opens at its button while
//! the core shows what it lists.

use std::sync::Arc;

use yapfast_core::application::Action;
use yapfast_core::network::Network;

use anomi_vm::header::{Header, Intent, Screen, Title};
use anomi_vm::menu::Spot;
use anomi_vm::shell::Layer;

use crate::app::App;

fn app(name: &str) -> (App, std::path::PathBuf) {
    let folder = std::env::temp_dir().join(format!("anomi-header-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let mut app = App::for_tests(&folder);
    drop(app.core.dispatch(Action::Preview));
    for room in app.core.state.rooms.iter_mut() {
        if room.id == "sam" {
            let room = Arc::make_mut(room);
            room.capabilities = Network::Discord.capabilities();
            room.person = Some("sam".into());
        }
    }
    app.core.state.list_changes += 1;
    app.core.state.discord_online = true;
    (app, folder)
}

fn done(app: App, folder: std::path::PathBuf) {
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

fn ask(app: &mut App, intent: Intent) {
    drop(super::handle(app, intent));
}

#[test]
fn the_pins_and_the_friends_picker_open_at_their_buttons_and_close() {
    let (mut app, folder) = app("popovers");
    drop(app.dispatch(Action::Open("sam".into())));
    let spot = Spot::pointer(iced::Point::new(900.0, 40.0));
    ask(&mut app, Intent::Pins(spot));
    assert!(super::pins_open(&app.core.state));
    assert_eq!(
        app.vm
            .shell
            .opened(Layer::Pins)
            .and_then(|opened| opened.at),
        Some(spot)
    );
    // The friends picker takes the pins' place, as in the core.
    ask(&mut app, Intent::AddPeople(spot));
    assert!(super::add_people_open(&app.core.state));
    assert!(!app.vm.shell.is_open(Layer::Pins) && app.vm.shell.is_open(Layer::AddPeople));
    // Escape or a press outside lets the core go too.
    drop(crate::intent::shell::handle(
        &mut app,
        anomi_vm::shell::Intent::Close(Layer::AddPeople),
    ));
    assert!(!super::add_people_open(&app.core.state));
    ask(&mut app, Intent::Pins(spot));
    ask(&mut app, Intent::Pins(spot));
    assert!(!super::pins_open(&app.core.state) && !app.vm.shell.is_open(Layer::Pins));
    done(app, folder);
}

#[test]
fn the_topic_toggles_and_friends_keeps_its_tab() {
    let (mut app, folder) = app("topic");
    let spot = Spot::default();
    ask(&mut app, Intent::Topic(spot));
    assert!(app.vm.shell.is_open(Layer::Topic));
    ask(&mut app, Intent::Topic(spot));
    assert!(!app.vm.shell.is_open(Layer::Topic));
    ask(&mut app, Intent::Topic(spot));
    ask(&mut app, Intent::Close);
    assert!(!app.vm.shell.is_open(Layer::Topic));
    drop(app.dispatch(Action::Navigate(
        yapfast_core::application::Navigation::DirectMessages,
    )));
    drop(app.dispatch(Action::OpenDmScreen(
        yapfast_core::application::DmScreen::Friends,
    )));
    ask(&mut app, Intent::FriendsAll(true));
    assert!(app.vm.people.friends.all);
    let Header::Open(place) = &app.vm.header else {
        panic!("a place")
    };
    assert_eq!(place.title, Title::Screen(Screen::Friends { all: true }));
    done(app, folder);
}

#[test]
fn a_stranger_is_offered_friendship_and_their_card_opens_under_them() {
    let (mut app, folder) = app("person");
    drop(app.dispatch(Action::Open("sam".into())));
    let Header::Open(place) = &app.vm.header else {
        panic!("a place")
    };
    assert_eq!(place.tools.friend, anomi_vm::header::Friend::Add);
    let spot = Spot::pointer(iced::Point::new(40.0, 40.0));
    ask(&mut app, Intent::Profile(spot));
    assert_eq!(
        app.vm
            .shell
            .opened(Layer::Profile)
            .and_then(|opened| opened.at),
        Some(spot)
    );
    done(app, folder);
}
