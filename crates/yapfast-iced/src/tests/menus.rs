//! Places' and people's menus with their submenus: a server's notification
//! choices and what each sends, the bell's and a channel's alike, a submenu
//! beside its row (turned to the other side without room) that the pointer
//! and the keys open and close, roles ticked in place, the servers to
//! invite someone to, the choices that ask first, and a server's events
//! menu.

use iced::{
    Point, Rectangle, Settings,
    keyboard::{self, key},
};
use iced_test::{Simulator, selector::Candidate};
use yapfast_core::{
    application::{Action, MUTE_TIMES, Navigation, member_server, with_clubs},
    chat::{Command, DiscordCommand, Muting, NotificationChange, NotifyLevel},
    yapfast_discord::ready,
};

use super::{interact, preview, server::discord_server, shows};
use crate::{
    app::{App, Message},
    controls::Check,
};

const SERVER: &str = "discord:10";
const GENERAL: &str = "discord:10:11";
const TALK: &str = "discord:10:4";

/// The open menu's rows: each one's label, subtitle and tick.
fn rows(app: &App) -> Vec<(String, String, Check)> {
    app.win
        .menu
        .as_ref()
        .expect("a menu")
        .rows
        .iter()
        .map(|row| {
            let subtitle = row.subtitle.as_deref().unwrap_or_default();
            (row.label.to_string(), subtitle.to_owned(), row.check)
        })
        .collect()
}

fn labels(app: &App) -> Vec<String> {
    rows(app).into_iter().map(|row| row.0).collect()
}

/// The labels of the submenu of the row labelled `label`, and their ticks.
fn submenu(app: &App, label: &str) -> Vec<(String, Check)> {
    let menu = app.win.menu.as_ref().expect("a menu");
    let row = menu
        .rows
        .iter()
        .find(|row| row.label == label)
        .unwrap_or_else(|| panic!("no {label:?}"));
    row.submenu
        .iter()
        .map(|row| (row.label.to_string(), row.check))
        .collect()
}

fn index(app: &App, label: &str) -> usize {
    labels(app)
        .iter()
        .position(|row| row == label)
        .unwrap_or_else(|| panic!("no {label:?}"))
}

/// The notification changes the core sent Discord since this was last
/// asked.
fn changes(app: &mut App) -> Vec<(String, NotificationChange)> {
    std::mem::take(&mut app.client.sent)
        .into_iter()
        .filter_map(|command| match command {
            Command::Discord(DiscordCommand::Notifications { place, change }) => {
                Some((place, change))
            }
            _ => None,
        })
        .collect()
}

fn key(app: &mut App, named: key::Named) {
    let _ = app.update(Message::Key(
        keyboard::Key::Named(named),
        keyboard::Modifiers::default(),
    ));
}

/// Where the text `label` shows in the window.
fn bounds(app: &App, label: &str) -> Rectangle {
    let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
    let mut found = None;
    let _ = ui.find(|candidate: Candidate<'_>| {
        if let Candidate::Text {
            content,
            visible_bounds: Some(bounds),
            ..
        } = candidate
            && content == label
        {
            found = Some(bounds);
        }
        None::<()>
    });
    found.unwrap_or_else(|| panic!("{label:?} isn't in view"))
}

#[test]
fn a_right_click_on_a_servers_tile_opens_its_menu() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    // The server "Test" has no icon: its tile shows its initial.
    interact(&mut app, |ui| super::right_click(ui, "T"));
    let menu = app.win.menu.as_ref().expect("the server's menu");
    assert_eq!(menu.place.as_deref(), Some(SERVER));
    assert!(labels(&app).contains(&"Leave server".to_owned()));
}

#[test]
fn a_servers_menu_shows_and_changes_its_notifications() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    let _ = crate::place_menus::open_place_menu(&mut app, SERVER, Point::new(100.0, 100.0), false);
    assert_eq!(
        labels(&app),
        [
            "Mark as read",
            "Mute server",
            "Notification settings",
            "Hide muted channels",
            "Edit server profile",
            "Leave server",
            "Copy link",
        ]
    );
    let shown = rows(&app);
    assert_eq!(shown[2].1, "Only @mentions", "the server's default");
    assert_eq!(shown[3].2, Check::Box(true));
    assert_eq!(submenu(&app, "Mute server").len(), MUTE_TIMES.len());
    assert_eq!(
        submenu(&app, "Notification settings"),
        [
            ("All messages".into(), Check::None),
            ("Only @mentions".into(), Check::Current),
            ("Nothing".into(), Check::None),
            ("Suppress @everyone and @here".into(), Check::Box(false)),
            ("Suppress all role @mentions".into(), Check::Box(true)),
        ]
    );

    // Clicked itself, Mute server mutes until it's turned back on.
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(1)));
    assert!(app.win.menu.is_none());
    assert_eq!(
        changes(&mut app),
        [(SERVER.into(), NotificationChange::Mute(Muting::Always))]
    );

    // Its times are its submenu's, which the pointer opens.
    let _ = crate::place_menus::open_place_menu(&mut app, SERVER, Point::new(100.0, 100.0), false);
    let _ = app.update(Message::Menu(crate::menus::Event::Light(1)));
    interact(&mut app, |ui| {
        ui.click("For 1 hour").expect("the time");
    });
    assert_eq!(
        changes(&mut app),
        [(SERVER.into(), NotificationChange::Mute(Muting::Minutes(60)))]
    );

    // A level, then a switch, which turns over and keeps the menu open, as
    // Hide muted channels does in the menu itself.
    let _ = crate::place_menus::open_place_menu(&mut app, SERVER, Point::new(100.0, 100.0), false);
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(2)));
    assert_eq!(app.win.menu.as_ref().and_then(|menu| menu.sub), Some(2));
    let _ = app.update(Message::Menu(crate::menus::Event::SubChoose(4)));
    assert_eq!(
        submenu(&app, "Notification settings")[4].1,
        Check::Box(false)
    );
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(3)));
    assert_eq!(rows(&app)[3].2, Check::Box(false));
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(2)));
    let _ = app.update(Message::Menu(crate::menus::Event::SubChoose(2)));
    assert!(app.win.menu.is_none());
    assert_eq!(
        changes(&mut app),
        [
            (SERVER.into(), NotificationChange::SuppressRoles(false)),
            (SERVER.into(), NotificationChange::HideMuted(false)),
            (
                SERVER.into(),
                NotificationChange::Level(Some(NotifyLevel::Nothing))
            ),
        ]
    );

    // The header's dropdown has the same rows, in Discord's order.
    let _ = crate::place_menus::open_place_menu(&mut app, SERVER, Point::new(100.0, 100.0), true);
    assert_eq!(
        labels(&app),
        [
            "Notification settings",
            "Edit server profile",
            "Hide muted channels",
            "Leave server"
        ]
    );
}

#[test]
fn the_bell_and_a_channels_menu_offer_the_same_notification_choices() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    app.client.dispatch(Action::SelectRoom(GENERAL.into()));
    let _ = app.sync();
    assert_eq!(app.client.state().selected.as_deref(), Some(GENERAL));
    assert!(app.client.app.notifications(GENERAL).is_some());
    let _ = app.update(Message::Bell);
    let row = |label: &str, subtitle: &str, check| (label.into(), subtitle.into(), check);
    assert_eq!(
        rows(&app),
        [
            row("Mute channel", "", Check::None),
            row("Use category default", "Only @mentions", Check::Current),
            row("All messages", "", Check::None),
            row("Only @mentions", "", Check::None),
            row("Nothing", "", Check::None),
        ]
    );
    assert_eq!(submenu(&app, "Mute channel").len(), MUTE_TIMES.len());
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(3)));
    assert_eq!(
        changes(&mut app),
        [(
            GENERAL.into(),
            NotificationChange::Level(Some(NotifyLevel::Mentions))
        )]
    );

    // Its own menu has the same, the levels in a submenu.
    let _ = crate::place_menus::open_place_menu(&mut app, GENERAL, Point::new(100.0, 100.0), false);
    let at = index(&app, "Notification settings");
    assert_eq!(rows(&app)[at].1, "Only @mentions");
    assert_eq!(submenu(&app, "Notification settings").len(), 4);
    assert!(labels(&app).contains(&"Mute channel".to_owned()));

    // A category's says what it is.
    let _ = crate::place_menus::open_place_menu(&mut app, TALK, Point::new(100.0, 100.0), false);
    assert_eq!(
        labels(&app),
        ["Mark as read", "Mute category", "Notification settings"]
    );
    assert_eq!(rows(&app)[2].1, "Only @mentions");
}

#[test]
fn a_submenu_opens_beside_its_row_and_follows_the_keys() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    let _ = crate::place_menus::open_place_menu(&mut app, SERVER, Point::new(100.0, 100.0), false);
    let _ = app.update(Message::Menu(crate::menus::Event::Light(2)));
    let row = bounds(&app, "Notification settings");
    let first = bounds(&app, "All messages");
    assert!(first.x > row.x + row.width, "to the right of the menu");
    assert!(
        (first.y - row.y).abs() < 2.0,
        "its first row level with the row: {first:?} {row:?}"
    );
    // The pointer onto another row closes it.
    let _ = app.update(Message::Menu(crate::menus::Event::Light(4)));
    assert_eq!(app.win.menu.as_ref().and_then(|menu| menu.sub), None);

    // In a window too short for it the menu scrolls, and says how far...
    let frame = app.shell.frame;
    app.shell.frame = iced::Size::new(frame.width, 200.0);
    app.window = app.shell.frame;
    let _ = crate::place_menus::open_place_menu(&mut app, SERVER, Point::new(100.0, 20.0), false);
    let mut ui = Simulator::with_size(Settings::default(), app.shell.frame, app.view());
    ui.point_at(Point::new(150.0, 100.0));
    ui.simulate(crate::widgets::swipe(-40.0));
    let scrolled: Vec<f32> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::Menu(crate::menus::Event::Scrolled(down)) => Some(down),
            _ => None,
        })
        .collect();
    assert_eq!(scrolled.last(), Some(&40.0), "{scrolled:?}");
    app.shell.frame = frame;
    app.window = frame;
    // ...and a submenu opens beside its row where the row shows, that
    // far up from its unscrolled place.
    let _ = crate::place_menus::open_place_menu(&mut app, SERVER, Point::new(100.0, 100.0), false);
    let _ = app.update(Message::Menu(crate::menus::Event::Light(2)));
    let unscrolled = bounds(&app, "All messages");
    let _ = app.update(Message::Menu(crate::menus::Event::Scrolled(40.0)));
    let first = bounds(&app, "All messages");
    assert!((unscrolled.y - 40.0 - first.y).abs() < 1.0, "{first:?}");

    // Without room on the right, it opens on the left.
    let right = app.window.width - 10.0;
    let _ = crate::place_menus::open_place_menu(&mut app, SERVER, Point::new(right, 100.0), false);
    let _ = app.update(Message::Menu(crate::menus::Event::Light(2)));
    let row = bounds(&app, "Notification settings");
    let first = bounds(&app, "All messages");
    assert!(first.x + first.width < row.x, "{first:?} {row:?}");

    // Right opens the lit row's, its first row lit; the arrows move in it,
    // Left leaves it and Escape closes the menu.
    let _ = crate::place_menus::open_place_menu(&mut app, SERVER, Point::new(100.0, 100.0), false);
    key(&mut app, key::Named::ArrowDown);
    key(&mut app, key::Named::ArrowDown);
    key(&mut app, key::Named::ArrowRight);
    let menu = app.win.menu.as_ref().expect("open");
    assert_eq!((menu.sub, menu.sub_lit), (Some(1), Some(0)));
    key(&mut app, key::Named::ArrowDown);
    assert_eq!(app.win.menu.as_ref().and_then(|menu| menu.sub_lit), Some(1));
    key(&mut app, key::Named::ArrowLeft);
    let menu = app.win.menu.as_ref().expect("open");
    assert_eq!((menu.sub, menu.lit), (None, Some(1)));
    key(&mut app, key::Named::ArrowRight);
    key(&mut app, key::Named::Enter);
    assert!(app.win.menu.is_none());
    assert_eq!(
        changes(&mut app),
        [(SERVER.into(), NotificationChange::Mute(Muting::Minutes(15)))]
    );
}

#[test]
fn a_members_roles_tick_in_place_and_invite_to_server_lists_the_servers() {
    let (mut app, _folder) = super::preview();
    let mut core = member_server(&["30"]);
    with_clubs(&mut core);
    app.client.app = core;
    let _ = app.sync();
    let _ = crate::person_menu::open_person_menu(
        &mut app,
        "4",
        Point::new(300.0, 200.0),
        crate::person_menu::CONTEXT_MENU,
    );
    // Roles: those this account may give or take, ticked where held.
    assert_eq!(
        submenu(&app, "Roles"),
        [
            ("Crew".into(), Check::Box(true)),
            ("Wardens".into(), Check::Box(false)),
        ]
    );
    assert_eq!(
        submenu(&app, "Invite to server"),
        [("Club".into(), Check::None)],
        "not the server she's shown in, nor one that doesn't let Alice invite"
    );
    let roles = index(&app, "Roles");
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(roles)));
    let _ = app.update(Message::Menu(crate::menus::Event::SubChoose(0)));
    assert_eq!(
        submenu(&app, "Roles")[0].1,
        Check::Box(false),
        "Crew unticks at once, and the menu stays open"
    );
}

#[test]
fn a_channels_management_choices_ask_first() {
    let (mut app, _folder) = discord_server(
        yapfast_core::yapfast_discord::ready::permission::MANAGE_CHANNELS
            | yapfast_core::yapfast_discord::ready::permission::CREATE_INSTANT_INVITE,
        |_, _| {},
    );
    let _ = crate::place_menus::open_place_menu(&mut app, GENERAL, Point::new(100.0, 100.0), false);
    let delete = index(&app, "Delete channel");
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(delete)));
    assert!(app.client.state().channel_dialog.is_some(), "it asks");
    app.client.app.state.channel_dialog = None;

    let _ = crate::place_menus::open_place_menu(&mut app, GENERAL, Point::new(100.0, 100.0), false);
    let invite = index(&app, "Invite to channel");
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(invite)));
    assert!(app.client.state().invite.is_some(), "the invite's dialog");

    let _ = crate::place_menus::open_place_menu(&mut app, SERVER, Point::new(100.0, 100.0), false);
    let leave = index(&app, "Leave server");
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(leave)));
    assert!(app.client.state().leave.is_some(), "Leave server asks");
}

#[test]
fn a_servers_top_shows_its_boost_goal_and_events_as_discords_does() {
    let (mut app, _folder) = discord_server(0, |server, _| {
        server.banner = Some("https://cdn.example/banner.png".into());
        server.boost_bar = true;
        server.boosts = 7;
        server.events = std::sync::Arc::new(ready::scheduled_events(&serde_json::json!([
            {"id": "1", "name": "Movie night", "status": 2,
             "scheduled_start_time": "2026-11-14T20:00:00+00:00",
             "entity_metadata": {"location": "Lounge"}},
            {"id": "2", "name": "Over", "status": 3,
             "scheduled_start_time": "2026-11-01T20:00:00+00:00"},
        ])));
    });
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    let _ = app.sync();
    for words in ["Boost Goal", "7/33 Boosts", "1 Event"] {
        assert!(shows(&app, words), "{words}");
    }
    interact(&mut app, |ui| {
        ui.click("1 Event").expect("the events row");
    });
    assert_eq!(
        rows(&app),
        [(
            "Movie night".to_owned(),
            "Happening now · Lounge".to_owned(),
            Check::None
        )]
    );
    // The menu opens under the row, inside the sidebar.
    let menu = app.win.menu.as_ref().expect("the events menu");
    assert!(menu.at.y >= app.sidebar.top_height() - 8.0, "{:?}", menu.at);
}

#[test]
fn a_community_without_events_says_so_and_a_server_without_a_bar_shows_none() {
    // Only a member who may create events is offered the row.
    let (mut plain, _plain_folder) = discord_server(0, |server, _| {
        server.community = true;
    });
    plain
        .client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    let _ = plain.sync();
    assert!(!shows(&plain, "Events"), "a plain member has no row");
    let (mut app, _folder) = discord_server(
        yapfast_core::yapfast_discord::ready::permission::CREATE_EVENTS,
        |server, _| {
            server.community = true;
            server.boosts = 7;
        },
    );
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    let _ = app.sync();
    assert!(!shows(&app, "Boost Goal"), "the bar is off");
    interact(&mut app, |ui| {
        ui.click("Events").expect("the events row");
    });
    let menu = app.win.menu.as_ref().expect("the events menu");
    assert!(menu.rows[0].disabled);
    assert_eq!(labels(&app), ["No upcoming events"]);
}

#[test]
fn the_window_size_follows_a_resize() {
    let (mut app, _folder) = discord_server(0, |_, _| {});
    // As it opens, then resized smaller than it lays out in.
    let _ = app.update(Message::Shell(crate::shell::Event::Resized(
        app.shell.frame,
    )));
    let size = iced::Size::new(700.0, 500.0);
    let _ = app.update(Message::Shell(crate::shell::Event::Resized(size)));
    assert_eq!(app.window, size);
}

#[test]
fn a_messages_menu_takes_its_apps_in_place_once_they_come() {
    use std::sync::Arc;
    use yapfast_core::chat::{ChatMessage, CommandIndex, Event};
    let (mut app, _folder) = discord_server(0, |_, _| {});
    app.client
        .dispatch(Action::Navigate(Navigation::Server(SERVER.into())));
    app.client.dispatch(Action::SelectRoom(GENERAL.into()));
    assert_eq!(app.client.state().selected.as_deref(), Some(GENERAL));
    let said = |item: &str, body: &str| {
        Arc::new(ChatMessage {
            item: item.into(),
            author: "Sam".into(),
            body: body.into(),
            timestamp: 1,
            can_react: true,
            can_reply: true,
            ..Default::default()
        })
    };
    super::change(&mut app, |state| {
        state.messages = Arc::new(vec![said("m0", "first"), said("m1", "second")]);
    });
    let _ = crate::menus::open_menu(&mut app, 1, Point::new(400.0, 300.0));
    assert_eq!(
        app.win.menu.as_ref().and_then(|menu| menu.item.as_deref()),
        Some("m1")
    );
    let before = rows(&app);
    assert!(
        !before.iter().any(|(label, ..)| label == "Apps"),
        "no index yet"
    );

    let _ = crate::menus::menu_light(&mut app, 0);
    // Earlier history comes in above while the menu is open, then the
    // room's commands.
    super::change(&mut app, |state| {
        let mut messages = vec![said("m-1", "earlier")];
        messages.extend(state.messages.iter().cloned());
        state.messages = Arc::new(messages);
    });
    let notes = CommandIndex::parse(
        &serde_json::json!({
            "applications": [{"id": "8", "name": "Notes", "flags": "0"}],
            "application_commands": [
                {"id": "4", "type": 3, "application_id": "8", "name": "Save note",
                 "description": "", "integration_types": [1], "contexts": [0, 1, 2],
                 "version": "1"}],
        }),
        true,
    );
    app.client.apply(Event::CommandIndex {
        room: GENERAL.into(),
        index: Ok(Arc::new(notes)),
    });
    let _ = app.sync();
    let menu = app.win.menu.as_ref().expect("still open");
    assert_eq!(menu.item.as_deref(), Some("m1"), "the same message's");
    assert_eq!(menu.lit, Some(0), "its light kept");
    let after = rows(&app);
    assert_eq!(after.len(), before.len() + 1);
    let apps = after
        .iter()
        .position(|(label, ..)| label == "Apps")
        .expect("Apps");
    assert_eq!(menu.rows[apps].submenu[0].label, "Save note");
    // Once in, it isn't built again on the next batch.
    let _ = crate::menus::menu_light(&mut app, apps);
    let _ = app.sync();
    assert_eq!(app.win.menu.as_ref().expect("open").sub, Some(apps));
}

#[test]
fn a_menus_keys_pass_over_greyed_rows_and_clicks_on_them_do_nothing() {
    use crate::menus::{Menu, MenuAct, MenuRow};
    use yapfast_core::application::MenuChoice;
    let (mut app, _folder) = preview();
    let row = |choice, disabled| MenuRow {
        disabled,
        ..MenuRow::new(MenuAct::Place(choice), "row", crate::pictures::Icon::Id)
    };
    app.win.menu = Some(Menu {
        place: Some("nowhere".into()),
        ..Menu::new(
            Point::new(200.0, 120.0),
            vec![
                row(MenuChoice::MarkRead, false),
                row(MenuChoice::AddFriend, true),
                row(MenuChoice::OpenChat, false),
                row(MenuChoice::AcceptFriend, true),
            ],
        )
    });
    let down = || {
        Message::Key(
            keyboard::Key::Named(key::Named::ArrowDown),
            keyboard::Modifiers::default(),
        )
    };
    let mut lit = Vec::new();
    for _ in 0..4 {
        let _ = app.update(down());
        lit.push(app.win.menu.as_ref().expect("the menu stays").lit);
    }
    assert_eq!(lit, [Some(0), Some(2), Some(0), Some(2)]);
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(1)));
    assert!(app.win.menu.is_some(), "a greyed row keeps the menu open");
}

/// The member list's menu opened on `user` with the Lounge's chat open,
/// where Dave, server muted, and Alice are in the call.
fn lounge_member_menu(user: &str) -> (App, tempfile::TempDir) {
    let (mut app, folder) = super::preview();
    app.client.app = member_server(&["30"]);
    app.client.app.state.selected = Some("discord:10:12".into());
    let _ = app.sync();
    let _ = crate::person_menu::open_person_menu(
        &mut app,
        user,
        Point::new(300.0, 200.0),
        crate::person_menu::CONTEXT_MENU,
    );
    (app, folder)
}

#[test]
fn a_members_menu_beside_a_voice_channel_ticks_and_changes_the_calls_rows() {
    let (mut app, _folder) = lounge_member_menu("4");
    let shown = rows(&app);
    let row = |label: &str| {
        shown
            .iter()
            .find(|row| row.0 == label)
            .unwrap_or_else(|| panic!("no {label:?}"))
            .clone()
    };
    assert_eq!(row("Mute").2, Check::Box(false));
    assert_eq!(
        row("Server mute").2,
        Check::Box(true),
        "Dave is server muted"
    );
    assert_eq!(row("Server deafen").2, Check::Box(false));
    let menu = app.win.menu.as_ref().expect("a menu");
    for label in [
        "Mute",
        "Mute Soundboard",
        "Server mute",
        "Server deafen",
        "Disconnect",
    ] {
        let row = menu
            .rows
            .iter()
            .find(|row| row.label == label)
            .expect(label);
        assert!(!row.disabled, "{label} acts");
    }
    let _ = std::mem::take(&mut app.client.sent);
    let server_mute = index(&app, "Server mute");
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(server_mute)));
    assert!(app.win.menu.is_some(), "a tick keeps the menu open");
    assert_eq!(rows(&app)[server_mute].2, Check::Box(false));
    let sent = std::mem::take(&mut app.client.sent);
    assert!(
        sent.iter().any(|command| matches!(
            command,
            Command::Discord(DiscordCommand::Member { user, change, .. })
                if user == "4"
                    && *change == yapfast_core::chat::MemberChange::ServerMute(false)
        )),
        "unmuted on the server"
    );
    // Clicked again before Discord answers, the row's tick is what goes.
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(server_mute)));
    assert_eq!(rows(&app)[server_mute].2, Check::Box(true));
    let sent = std::mem::take(&mut app.client.sent);
    assert!(
        sent.iter().any(|command| matches!(
            command,
            Command::Discord(DiscordCommand::Member { user, change, .. })
                if user == "4"
                    && *change == yapfast_core::chat::MemberChange::ServerMute(true)
        )),
        "muted again, as the box shows"
    );
}

#[test]
fn ones_own_members_menu_opens_the_server_profile_editor() {
    let (mut app, _folder) = lounge_member_menu("1");
    let edit = index(&app, "Edit server profile");
    assert!(!app.win.menu.as_ref().expect("a menu").rows[edit].disabled);
    let _ = app.update(Message::Menu(crate::menus::Event::Choose(edit)));
    let editor = app.client.state().profile_editor.as_ref();
    assert!(
        editor.is_some_and(|editor| editor.server.is_some()),
        "the server's profile opens in the editor"
    );
}

#[test]
fn add_note_says_whom_the_note_is_for() {
    let (app, _folder) = lounge_member_menu("4");
    let note = rows(&app)
        .into_iter()
        .find(|row| row.0 == "Add note")
        .expect("Add note");
    assert_eq!(note.1, "Only visible to you");
}

#[test]
fn a_dm_rows_menu_joins_the_call_going_rather_than_starting_one() {
    use std::sync::Arc;
    let (mut app, _folder) = preview();
    super::change(&mut app, |state| {
        let riley = state
            .rooms
            .iter_mut()
            .find(|room| room.id == "riley")
            .expect("Riley's DM");
        Arc::make_mut(riley).capabilities.dm_calls = true;
    });
    let menu = |app: &mut App| {
        let _ = app.update(Message::Side(crate::sidebar::Event::Menu(
            "riley".into(),
            Point::new(300.0, 200.0),
        )));
        let shown = labels(app);
        key(app, key::Named::Escape);
        shown
    };
    let shown = menu(&mut app);
    assert!(shown.contains(&"Start call".to_owned()), "{shown:?}");
    super::change(&mut app, |state| {
        let riley = state
            .rooms
            .iter_mut()
            .find(|room| room.id == "riley")
            .expect("Riley's DM");
        Arc::make_mut(riley).people = vec![yapfast_core::chat::VoicePerson {
            id: "riley".into(),
            name: "Riley".into(),
            ..Default::default()
        }];
    });
    let shown = menu(&mut app);
    assert!(shown.contains(&"Join call".to_owned()), "{shown:?}");
    assert!(!shown.contains(&"Start call".to_owned()));
}

#[test]
fn shift_and_tab_step_back_through_a_menu() {
    use crate::menus::{Menu, MenuAct, MenuRow};
    use yapfast_core::application::MenuChoice;
    let (mut app, _folder) = preview();
    let row = |choice| MenuRow::new(MenuAct::Place(choice), "row", crate::pictures::Icon::Id);
    app.win.menu = Some(Menu {
        place: Some("nowhere".into()),
        ..Menu::new(
            Point::new(200.0, 120.0),
            vec![
                row(MenuChoice::MarkRead),
                row(MenuChoice::AddFriend),
                row(MenuChoice::OpenChat),
            ],
        )
    });
    let tab = |app: &mut App, modifiers: keyboard::Modifiers| {
        app.modifiers = modifiers;
        let _ = app.update(Message::Key(
            keyboard::Key::Named(key::Named::Tab),
            modifiers,
        ));
        app.win.menu.as_ref().expect("the menu stays").lit
    };
    let none = keyboard::Modifiers::default();
    assert_eq!(tab(&mut app, none), Some(0));
    assert_eq!(tab(&mut app, none), Some(1));
    assert_eq!(tab(&mut app, keyboard::Modifiers::SHIFT), Some(0));
    assert_eq!(
        tab(&mut app, keyboard::Modifiers::SHIFT),
        Some(2),
        "round to the last"
    );
}

#[test]
fn every_row_of_a_persons_and_a_places_menu_has_its_icon() {
    let iconless = |app: &App| -> Vec<String> {
        let menu = app.win.menu.as_ref().expect("a menu");
        menu.rows
            .iter()
            .filter(|row| row.icon.is_none())
            .map(|row| row.label.to_string())
            .collect()
    };
    // A member's, from the member list.
    let (mut app, _folder) = super::preview();
    app.client.app = member_server(&["30"]);
    let _ = app.sync();
    let _ = crate::person_menu::open_person_menu(
        &mut app,
        "4",
        Point::new(300.0, 200.0),
        crate::person_menu::CONTEXT_MENU,
    );
    assert_eq!(iconless(&app), [""; 0], "a member's");
    // A server's, from its tile and from its name, and a channel's.
    let (mut app, _folder) = discord_server(0, |_, _| {});
    for (place, header) in [(SERVER, false), (SERVER, true), (GENERAL, false)] {
        let at = Point::new(100.0, 100.0);
        let _ = crate::place_menus::open_place_menu(&mut app, place, at, header);
        assert_eq!(iconless(&app), [""; 0], "{place}'s, {header}");
    }
}
