//! The rail and the sidebar: servers, folders and DMs opening, their
//! unread marks, categories folding and the DMs' search.

use std::sync::Arc;

use yapfast_core::application::Navigation;

use super::{change, interact, preview, shows, start, unread};
use crate::app::{App, Message};

#[test]
fn a_channel_clicked_in_the_list_opens() {
    let (mut app, _folder) = preview();
    assert!(!shows(&app, "What are we playing next?"));
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    assert_eq!(app.client.state().selected.as_deref(), Some("games"));
    assert_eq!(app.header.name, "games");
    assert!(!app.header.direct);
    assert!(shows(&app, "What are we playing next?"));
}

#[test]
fn an_unread_dm_shows_on_the_rail_above_the_servers_and_opens() {
    use crate::rail::{Event, Kind};
    let (mut app, _folder) = preview();
    let servers = app.client.state().servers.len();
    assert_eq!(app.rail.items.len(), servers + 1);
    assert_eq!(
        app.rail.items[0].kind,
        Kind::Divider,
        "the line leads the servers"
    );
    unread(&mut app, "riley", 2);
    assert_eq!(app.rail.items[0].kind, Kind::Dm);
    assert_eq!(
        (
            app.rail.items[0].name.as_str(),
            app.rail.items[0].badge.as_str()
        ),
        ("Riley", "2")
    );
    assert_eq!(app.rail.items[1].kind, Kind::Divider);
    assert_eq!(app.rail.home.badge, "2", "the DM home counts its mentions");
    let _ = app.update(Message::Rail(Event::Dm("riley".into())));
    let state = app.client.state();
    assert_eq!(state.navigation, Navigation::DirectMessages);
    assert_eq!(state.selected.as_deref(), Some("riley"));
}

#[test]
fn a_voice_channels_chat_marks_its_server_only_by_mentions() {
    let (mut app, _folder) = preview();
    let friends = |app: &App| {
        let row = app
            .rail
            .items
            .iter()
            .find(|item| item.id == "friends")
            .unwrap();
        (row.unread, row.badge.clone())
    };
    let discord = |app: &mut App, discord: bool| {
        change(app, |state| {
            let server = state
                .servers
                .iter_mut()
                .find(|server| server.id == "friends");
            Arc::make_mut(server.unwrap()).discord = discord;
        })
    };
    discord(&mut app, true);
    unread(&mut app, "lounge", 0);
    assert_eq!(
        friends(&app),
        (false, String::new()),
        "Discord's client leaves a server read for its voice channels' chat"
    );
    unread(&mut app, "lounge", 1);
    assert_eq!(friends(&app), (true, "1".into()), "a mention there counts");
    unread(&mut app, "lounge", 0);
    discord(&mut app, false);
    assert_eq!(
        friends(&app),
        (true, String::new()),
        "a Matrix space's call room stays as it was"
    );
}

#[test]
fn a_folder_on_the_rail_holds_its_servers_until_opened_and_stays_so() {
    use crate::rail::{Event, Kind};
    let (mut app, folder) = preview();
    let in_folder = yapfast_core::chat::ServerFolder {
        id: "discord-folder:7".into(),
        name: None,
        color: Some(0xff0000),
    };
    change(&mut app, |state| {
        for server in &mut state.servers {
            Arc::make_mut(server).folder = Some(in_folder.clone());
        }
    });
    let kinds = |app: &App| -> Vec<(String, bool)> {
        app.rail
            .items
            .iter()
            .map(|item| (item.id.clone(), matches!(item.kind, Kind::Nested { .. })))
            .collect()
    };
    let closed = kinds(&app);
    assert_eq!(closed.len(), 2, "the line, and the folder");
    assert_eq!(
        app.rail.items[1].name, "Friends, Co-op crew",
        "named by its servers"
    );
    assert!(
        app.rail.items[1].selected,
        "closed, it shows a server of its is open"
    );
    let _ = app.update(Message::Rail(Event::Folder("discord-folder:7".into())));
    assert_eq!(
        kinds(&app),
        [
            ("divider".into(), false),
            ("discord-folder:7".into(), false),
            ("friends".into(), true),
            ("coop".into(), true),
        ]
    );
    // Opened, it stays open the next time.
    let again = start(&folder, true);
    assert!(again.layout.open_folders.contains("discord-folder:7"));
    let _ = app.update(Message::Rail(Event::Folder("discord-folder:7".into())));
    assert_eq!(kinds(&app).len(), 2);
}

#[test]
fn a_server_on_the_rail_opens_its_channels() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Rail(crate::rail::Event::Server("coop".into())));
    assert_eq!(app.sidebar.header.name, "Co-op crew");
    assert!(shows(&app, "looking-for-group"));
    assert!(!shows(&app, "games"));
}

#[test]
fn clicking_a_category_folds_it_and_it_stays_folded() {
    let (mut app, folder) = preview();
    assert!(shows(&app, "games"));
    interact(&mut app, |ui| {
        ui.click("TEXT CHANNELS").expect("the heading");
    });
    // The open channel stays; the others go.
    assert!(shows(&app, "general"));
    assert!(!shows(&app, "games"));
    let again = start(&folder, true);
    assert!(again.layout.collapsed["friends"].contains("TEXT CHANNELS"));
    // An unread one stays too.
    unread(&mut app, "games", 0);
    assert!(shows(&app, "games"));
    interact(&mut app, |ui| {
        ui.click("TEXT CHANNELS").expect("the heading");
    });
    assert!(shows(&app, "weekend plans"));
}

#[test]
fn opening_an_unread_channel_clears_its_mark() {
    use crate::sidebar::SideRow;
    let (mut app, _folder) = preview();
    unread(&mut app, "games", 1);
    let marked = |app: &App| {
        app.sidebar.rows.iter().any(|row| {
            matches!(row, SideRow::Room(room) if room.id == "games" && room.unread && room.badge == "1")
        })
    };
    assert!(marked(&app));
    assert!(
        app.rail
            .items
            .iter()
            .any(|item| item.id == "friends" && item.badge == "1")
    );
    interact(&mut app, |ui| {
        ui.click("games").expect("the games channel");
    });
    assert_eq!(app.client.state().selected.as_deref(), Some("games"));
    // Read, the backend's room list says so, and the marks go.
    unread(&mut app, "games", 0);
    change(&mut app, |state| {
        for room in &mut state.rooms {
            Arc::make_mut(room).unread = 0;
        }
    });
    assert!(!marked(&app));
    assert!(
        app.rail
            .items
            .iter()
            .any(|item| item.id == "friends" && item.badge.is_empty() && !item.unread)
    );
}

#[test]
fn the_dm_search_keeps_the_dms_whose_names_hold_it() {
    let (mut app, _folder) = preview();
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    assert!(shows(&app, "Sam") && shows(&app, "Friday crew"));
    let _ = app.update(Message::Side(crate::sidebar::Event::Filter("fri".into())));
    assert!(shows(&app, "Friday crew"));
    assert!(!shows(&app, "Riley"));
    // It's the DM home's: a server shows all its channels, and home again
    // shows every DM.
    let _ = app.update(Message::Rail(crate::rail::Event::Server("coop".into())));
    assert!(shows(&app, "looking-for-group"), "the server's channels");
    let _ = app.update(Message::Rail(crate::rail::Event::Home));
    assert!(shows(&app, "Riley"), "every DM again");
}
