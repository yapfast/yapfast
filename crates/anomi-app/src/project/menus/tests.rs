//! The window menu's rows, from a state with a Discord server, a channel
//! of it and a DM: what each offers, in its groups, each row choosing by
//! the id `src/ui/menus.rs` reads.

use std::sync::Arc;

use yapfast_core::application::Action;
use yapfast_core::chat::{ChatRoom, ChatServer, ServerFolder};

use anomi_vm::Intent;
use anomi_vm::icon::Icon;
use anomi_vm::menu::{self, Choice, Row};

use super::rows;
use crate::app::App;
use crate::tests::Folder;

/// An app whose state has the Discord server `discord:g1` with its
/// channel `discord:g1:c1`, and a DM with `discord:u2`.
pub(crate) fn discord_app(folder: &Folder) -> App {
    let mut app = App::for_tests(folder.path());
    drop(app.dispatch(Action::Preview));
    let state = &mut app.core.state;
    let template = (*state.rooms[0]).clone();
    state.preview = false;
    state.discord_user = Some("discord:u1".into());
    state.servers = vec![Arc::new(ChatServer {
        id: "discord:g1".into(),
        name: "Server".into(),
        channels: vec!["discord:g1:c1".into()],
        discord: true,
        ..ChatServer::default()
    })];
    state.rooms = vec![
        Arc::new(ChatRoom {
            id: "discord:g1:c1".into(),
            name: "general".into(),
            ..template.clone()
        }),
        Arc::new(ChatRoom {
            id: "discord:dm:d1".into(),
            name: "bob".into(),
            direct: true,
            person: Some("discord:u2".into()),
            ..template
        }),
    ];
    app
}

/// The folder of [`folder_app`].
pub(crate) const FOLDER: &str = "discord-folder:7";

/// [`discord_app`] with "Three" alone, then `discord:g1` and "Two" in
/// the folder [`FOLDER`], which has no name or colour and is open; nothing
/// is unread.
pub(crate) fn folder_app(folder: &Folder) -> App {
    let mut app = discord_app(folder);
    let state = &mut app.core.state;
    let inside = ServerFolder {
        id: FOLDER.into(),
        name: None,
        color: None,
    };
    let first = (*state.servers[0]).clone();
    let other = |id: &str, name: &str, folder| ChatServer {
        id: id.into(),
        name: name.into(),
        channels: Vec::new(),
        folder,
        ..first.clone()
    };
    state.servers = vec![
        Arc::new(other("discord:g3", "Three", None)),
        Arc::new(ChatServer {
            folder: Some(inside.clone()),
            ..first.clone()
        }),
        Arc::new(other("discord:g2", "Two", Some(inside))),
    ];
    for room in &mut state.rooms {
        let room = Arc::make_mut(room);
        room.unread = 0;
        room.mentions = 0;
    }
    state.list_changes = state.list_changes.wrapping_add(1);
    app.layout.edit(|saved| {
        saved.open_folders.insert(FOLDER.into());
    });
    app.refresh();
    app
}

fn texts(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|row| row.text.as_str()).collect()
}

fn chooses(row: &Row) -> Option<&Choice> {
    match &row.intent {
        Some(Intent::Menu(menu::Intent::Choose(choice))) => Some(choice),
        _ => None,
    }
}

#[test]
fn a_servers_menu_starts_with_mark_as_read_greyed() {
    let folder = Folder::new("menus-server");
    let mut app = discord_app(&folder);
    let made = rows(&app.core, &app.layout, "discord:g1", false);
    assert_eq!(texts(&made), ["Mark as read", "Copy link"]);
    assert!(made[0].disabled && !made[0].choosable());
    assert!(made[1].separated, "a line under Mark as read");
    // Discord's menus have no icons, but on Copy ID.
    assert!(made.iter().all(|row| row.plain && row.icon.is_none()));
    let link = chooses(&made[1]).expect("Copy link chooses");
    assert_eq!(
        (link.target.as_str(), link.id.as_str()),
        ("discord:g1", "copy-link")
    );
    // Copy ID shows with Developer Mode, with its icon.
    app.layout.edit(|saved| saved.developer_mode = true);
    let made = rows(&app.core, &app.layout, "discord:g1", false);
    let id = made.last().expect("Copy ID");
    assert_eq!(id.text, "Copy ID");
    assert_eq!(id.icon, Some(Icon::IdFilled));
}

#[test]
fn a_dm_and_its_person_offer_theirs() {
    let folder = Folder::new("menus-dm");
    let app = discord_app(&folder);
    let dm = rows(&app.core, &app.layout, "discord:dm:d1", false);
    assert_eq!(texts(&dm), ["Mark as read", "Profile", "Pin", "Copy link"]);
    assert!(chooses(&dm[2]).is_some_and(|choice| choice.id == "pin" && !choice.person));
    let person = rows(&app.core, &app.layout, "discord:u2", true);
    assert_eq!(texts(&person), ["Message", "Add friend", "Block"]);
    assert!(chooses(&person[0]).is_some_and(|choice| choice.person));
    // How this account stands with them is a group under a line.
    assert!(person[1].separated && !person[2].separated && person[2].danger);
}

#[test]
fn a_folders_menu_marks_it_read_moves_it_opens_its_settings_and_closes_folders() {
    let folder = Folder::new("menus-folder");
    let mut app = folder_app(&folder);
    let made = rows(&app.core, &app.layout, FOLDER, false);
    assert_eq!(
        texts(&made),
        [
            "Mark folder as read",
            "Move…",
            "Folder settings",
            "Close all folders"
        ]
    );
    assert!(made[0].disabled && !made[0].choosable(), "nothing to read");
    assert!(made[1].separated && made[2].separated && !made[3].separated);
    assert!(!made[3].disabled, "a folder is open");
    assert!(made.iter().all(|row| row.plain && row.icon.is_none()));
    assert_eq!(texts(&made[1].submenu), ["Top of list", "Bottom of list"]);
    let ids: Vec<&str> = (made[1].submenu.iter())
        .chain(&made[2..])
        .filter_map(chooses)
        .map(|choice| choice.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "folder-top",
            "folder-bottom",
            "folder-settings",
            "close-folders"
        ]
    );
    // Something to read in it, and no folder open.
    Arc::make_mut(&mut app.core.state.rooms[0]).unread = 2;
    app.layout.edit(|saved| saved.open_folders.clear());
    let made = rows(&app.core, &app.layout, FOLDER, false);
    assert!(made[0].choosable() && made[3].disabled);
    let mark = chooses(&made[0]).expect("Mark folder as read chooses");
    assert_eq!(
        (mark.target.as_str(), mark.id.as_str()),
        (FOLDER, "mark-read")
    );
    // Without a Discord account, nothing to keep the list in: marking read
    // and closing folders alone, under a line.
    app.core.state.discord_user = None;
    let made = rows(&app.core, &app.layout, FOLDER, false);
    assert_eq!(texts(&made), ["Mark folder as read", "Close all folders"]);
    assert!(made[1].separated);
    // A server in the folder keeps its own menu.
    let server = rows(&app.core, &app.layout, "discord:g1", false);
    assert_eq!(texts(&server)[0], "Mark as read");
}

#[test]
fn a_server_without_events_says_so() {
    let folder = Folder::new("menus-events");
    let app = discord_app(&folder);
    let events = rows(&app.core, &app.layout, "events:discord:g1", false);
    assert_eq!(texts(&events), ["No upcoming events"]);
    assert!(events[0].disabled);
    assert!(rows(&app.core, &app.layout, "nowhere", false).is_empty());
}
