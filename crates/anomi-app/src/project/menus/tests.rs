//! The window menu's rows, from a state with a Discord server, a channel
//! of it and a DM: what each offers, in its groups, each row choosing by
//! the id `src/ui/menus.rs` reads.

use std::sync::Arc;

use yapfast_core::application::Action;
use yapfast_core::chat::{ChatRoom, ChatServer};

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
fn a_server_without_events_says_so() {
    let folder = Folder::new("menus-events");
    let app = discord_app(&folder);
    let events = rows(&app.core, &app.layout, "events:discord:g1", false);
    assert_eq!(texts(&events), ["No upcoming events"]);
    assert!(events[0].disabled);
    assert!(rows(&app.core, &app.layout, "nowhere", false).is_empty());
}
