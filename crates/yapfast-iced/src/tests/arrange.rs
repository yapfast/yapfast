//! Dragging a Discord server's channels and categories in the sidebar, as
//! the Slint client's `RoomList` and the core's own tests of it check:
//! a press moved 4 px starts a drag where this account may move the row,
//! and letting go on a place it may land on moves it there, asking first
//! whether a channel takes its new category's permissions.

use std::sync::Arc;

use iced::{Point, mouse};
use yapfast_core::{
    application::{Action, Navigation},
    yapfast_discord::ready::{Overwrite, permission},
};

use super::{interact, server::discord_server, shows};
use crate::{
    app::{App, Message},
    arrange::Event,
};

const SERVER: &str = "discord:10";

/// The server open with Manage Channels and Manage Roles, its empty
/// category "Later" closed to @everyone's messages, so a channel moved
/// into it asks.
fn arranging() -> (App, tempfile::TempDir) {
    let (mut app, folder) = discord_server(
        permission::MANAGE_CHANNELS | permission::MANAGE_ROLES,
        |server, _| {
            let mut categories = (*server.categories).clone();
            let later = categories.get_mut("5").expect("Later");
            later.overwrites = Arc::new(vec![Overwrite {
                id: "10".into(),
                member: false,
                allow: 0,
                deny: permission::SEND_MESSAGES,
            }]);
            server.categories = Arc::new(categories);
        },
    );
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    let _ = app.sync();
    (app, folder)
}

fn place(app: &App, room: &str) -> usize {
    app.sidebar
        .places
        .ids
        .iter()
        .position(|id| id == room)
        .unwrap_or_else(|| panic!("{room} has no place"))
}

#[test]
fn a_channel_clicked_opens_and_pressed_and_moved_is_dragged() {
    let (mut app, _folder) = arranging();
    assert_eq!(
        app.sidebar.places.ids,
        [
            "discord:10:4",
            "discord:10:11",
            "discord:10:14",
            "discord:10:5"
        ],
        "Talk, #general, the Lounge and Later",
    );
    // Pressed, moved 3 px and let go, it's a click: #general opens.
    let press = |app: &mut App, by: f32, release: bool| {
        interact(app, |ui| {
            let general = ui
                .find("general")
                .expect("#general")
                .visible_bounds()
                .expect("in view");
            let from = Point::new(general.center_x(), general.center_y());
            ui.point_at(from);
            ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left,
            ))]);
            let to = Point::new(from.x, from.y + by);
            ui.point_at(to);
            ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
                position: to,
            })]);
            if release {
                ui.simulate([iced::Event::Mouse(mouse::Event::ButtonReleased(
                    mouse::Button::Left,
                ))]);
            }
        });
    };
    press(&mut app, 3.0, true);
    assert!(app.sidebar.arranging.is_none());
    assert_eq!(
        app.client.state().selected.as_deref(),
        Some("discord:10:11")
    );
    app.client
        .dispatch(Action::SelectRoom("discord:10:14".into()));
    let _ = app.sync();
    // Moved 5 px, it's dragged instead.
    press(&mut app, 5.0, false);
    let dragged = app.sidebar.arranging.as_ref().expect("dragging #general");
    assert_eq!(dragged.dragged, place(&app, "discord:10:11"));
    // Let go where it was, it stays, and nothing opens.
    let selected = app.client.state().selected.clone();
    let _ = crate::arrange::update(&mut app, Event::Drop);
    assert!(app.sidebar.arranging.is_none());
    assert_eq!(app.client.state().selected, selected);
}

#[test]
fn a_channel_dropped_into_a_category_asks_to_take_its_permissions() {
    let (mut app, _folder) = arranging();
    let (general, later) = (place(&app, "discord:10:11"), place(&app, "discord:10:5"));
    let _ = app.update(Message::Arrange(Event::Start(general, 10.0)));
    assert!(app.sidebar.arranging.is_some());
    assert!(
        app.sidebar.arrange_dimmed(None, true),
        "the Lounge dims under a text channel"
    );
    assert!(!app.sidebar.arrange_dimmed(None, false));
    let _ = app.update(Message::Arrange(Event::Over(later, true)));
    assert_eq!(
        app.sidebar.drop_line(Some(later)),
        Some(true),
        "below Later"
    );
    let _ = app.update(Message::Arrange(Event::Drop));
    let asked = app.client.state().move_sync.as_ref().expect("asked");
    assert_eq!(
        (asked.channel.as_str(), asked.category.as_str()),
        ("general", "Later")
    );
    assert!(shows(
        &app,
        "Would you like to sync general permissions with Later?"
    ));
    interact(&mut app, |ui| {
        ui.click("Keep Current Permissions").expect("Keep");
    });
    assert!(app.client.state().move_sync.is_none());
}

#[test]
fn without_manage_channels_nothing_drags() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    let _ = app.sync();
    let general = app
        .sidebar
        .places
        .ids
        .iter()
        .position(|id| id == "discord:10:11")
        .expect("#general has a place");
    let _ = app.update(Message::Arrange(Event::Start(general, 10.0)));
    assert!(app.sidebar.arranging.is_none());
}

/// The server of the core's members fixture open, Alice in `roles`; Dave
/// and Alice, on another client, are in the Lounge.
fn voice_server(roles: &[&str]) -> (App, tempfile::TempDir) {
    let (mut app, folder) = super::preview();
    app.client.app = yapfast_core::application::member_server(roles);
    // The workspace shows, as while previewing.
    app.client.app.state.preview = true;
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    let _ = app.sync();
    (app, folder)
}

const LOUNGE: &str = "discord:10:12";
const DEN: &str = "discord:10:13";

/// Someone dragged lets go over the list, on `onto`.
fn drop_on(app: &mut App, onto: &str) {
    let over = Point::new(crate::rail::WIDTH + 100.0, app.shell.frame.height / 2.0);
    let _ = app.update(Message::Arrange(Event::At(over)));
    let _ = app.update(Message::Arrange(Event::Onto(onto.into(), true)));
    let _ = app.update(Message::Arrange(Event::Drop));
}

#[test]
fn someone_dragged_onto_another_voice_channel_moves_as_move_to_does() {
    // Alice is Staff, who may move members.
    let (mut app, _folder) = voice_server(&["30"]);
    let targets = |app: &App| -> Option<Vec<String>> {
        app.sidebar
            .moving_person
            .as_ref()
            .map(|moving| moving.targets.iter().cloned().collect())
    };
    // Pressed on Dave's row under the Lounge and moved 5 px, he's dragged:
    // Den, the one other channel his menu's Move to offers, takes him;
    // the Lounge, the Vault and #general dim.
    interact(&mut app, |ui| {
        let dave = ui
            .find("Dave")
            .expect("Dave under the Lounge")
            .visible_bounds()
            .expect("in view");
        let from = Point::new(dave.center_x(), dave.center_y());
        ui.point_at(from);
        ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        let to = Point::new(from.x, from.y + 5.0);
        ui.point_at(to);
        ui.simulate([iced::Event::Mouse(mouse::Event::CursorMoved {
            position: to,
        })]);
    });
    assert_eq!(targets(&app), Some(vec![DEN.to_owned()]));
    assert!(!app.sidebar.arrange_dimmed(Some(DEN), true));
    assert!(
        app.sidebar.arrange_dimmed(Some(LOUNGE), true)
            && app.sidebar.arrange_dimmed(Some("discord:10:11"), false)
    );
    assert!(shows(&app, "Dave"), "he follows the pointer");
    // Let go on a channel he can't go to, nothing is sent.
    drop_on(&mut app, "discord:10:11");
    assert!(app.sidebar.moving_person.is_none());
    assert!(app.client.state().notice.is_none());
    // Escape stops it.
    let take = |app: &mut App, user: &str| {
        let _ = app.update(Message::Arrange(Event::Take(LOUNGE.into(), user.into())));
    };
    take(&mut app, "4");
    let _ = app.update(Message::Arrange(Event::Cancel));
    assert!(app.sidebar.moving_person.is_none());
    // Let go beside the list, over Den, nothing moves either.
    take(&mut app, "4");
    let _ = app.update(Message::Arrange(Event::At(Point::new(900.0, 300.0))));
    let _ = app.update(Message::Arrange(Event::Onto(DEN.into(), true)));
    let _ = app.update(Message::Arrange(Event::Drop));
    assert!(app.client.state().notice.is_none());
    // Dropped on Den, the move goes out, as from his menu; here, with no
    // Discord connection, it fails.
    take(&mut app, "4");
    assert!(!app.sidebar.move_aimed(DEN), "not over it yet");
    drop_on(&mut app, DEN);
    assert!(app.sidebar.moving_person.is_none());
    assert!(app.client.state().notice.is_some());
    // Another list showing, with nowhere to drop him, stops it.
    take(&mut app, "4");
    app.client
        .dispatch(Action::Navigate(Navigation::DirectMessages));
    let _ = app.sync();
    assert!(app.sidebar.moving_person.is_none());
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    let _ = app.sync();
    // Alice, on another client, joins Den from here.
    take(&mut app, "1");
    assert_eq!(targets(&app), Some(vec![DEN.to_owned()]));
    drop_on(&mut app, DEN);
    assert_eq!(
        app.client.state().voice.room.as_deref(),
        Some(DEN),
        "joining it"
    );

    // Without Move Members, Dave stays put.
    let (mut app, _folder) = voice_server(&["20"]);
    take(&mut app, "4");
    assert!(app.sidebar.moving_person.is_none());
}
