//! Arranging the servers on the rail: shown at once, sent whole, and put
//! back when Discord refuses.

use super::*;
use crate::chat::{NEW_FOLDER, ServerGroup, ServerListEntry};
use yapfast_discord::service::{Snapshot, Status};
use yapfast_discord::settings::Folder;

/// Servers A (10), B (20) and C (30), B and C in the folder "Games" (7).
fn snapshot(order: [&str; 3], folders: Vec<Folder>) -> Snapshot {
    let server = |id: &str| {
        let name = match id {
            "10" => "A",
            "20" => "B",
            _ => "C",
        };
        marked(
            yapfast_discord::ready::server(&serde_json::json!({
                "id": id, "owner_id": "99", "properties": {"name": name},
                "roles": [{"id": id, "name": "@everyone", "position": 0, "permissions": "0"}],
                "channels": [],
            }))
            .unwrap(),
            &[],
        )
    };
    Snapshot {
        status: Status::Online,
        user_id: Some("1".into()),
        servers: order.into_iter().map(server).collect(),
        folders,
        ..Default::default()
    }
}

fn games(servers: &[&str]) -> Folder {
    Folder {
        id: "7".into(),
        name: Some("Games".into()),
        color: Some(0xed4245),
        servers: servers.iter().map(|&id| id.into()).collect(),
    }
}

fn three_servers() -> Application {
    let mut app = Application::default();
    app.apply(discord_event(snapshot(
        ["10", "20", "30"],
        vec![games(&["20", "30"])],
    )));
    app
}

/// The rail's order now: each server's id, in its folder's brackets.
fn shown(app: &Application) -> Vec<(String, Option<String>)> {
    app.state
        .servers
        .iter()
        .map(|server| {
            (
                server.id.clone(),
                server.folder.as_ref().map(|folder| folder.id.clone()),
            )
        })
        .collect()
}

fn group(folder: Option<&str>, servers: &[&str]) -> ServerGroup {
    ServerGroup {
        folder: folder.map(Into::into),
        servers: servers.iter().map(|id| format!("discord:{id}")).collect(),
    }
}

fn sent(effects: &[Effect]) -> Option<&Vec<ServerListEntry>> {
    effects.iter().find_map(|effect| match effect {
        Effect::Backend(Command::Discord(DiscordCommand::ArrangeServers(entries))) => Some(entries),
        _ => None,
    })
}

#[test]
fn an_arrangement_shows_at_once_and_goes_whole_keeping_the_folders_name_and_colour() {
    let mut app = three_servers();
    let effects = app.dispatch(Action::ArrangeServers(vec![
        group(None, &["20"]),
        group(Some("discord-folder:7"), &["30", "10"]),
    ]));
    assert_eq!(
        sent(&effects),
        Some(&vec![
            ServerListEntry {
                servers: vec!["20".into()],
                ..ServerListEntry::default()
            },
            ServerListEntry {
                id: "7".into(),
                name: Some("Games".into()),
                color: Some(0xed4245),
                servers: vec!["30".into(), "10".into()],
            },
        ])
    );
    let folder = Some("discord-folder:7".to_owned());
    assert_eq!(
        shown(&app),
        [
            ("discord:20".to_owned(), None),
            ("discord:30".to_owned(), folder.clone()),
            ("discord:10".to_owned(), folder),
        ]
    );
    // The same list again asks nothing.
    assert!(
        app.dispatch(Action::ArrangeServers(vec![
            group(None, &["20"]),
            group(Some("discord-folder:7"), &["30", "10"]),
        ]))
        .is_empty()
    );
}

#[test]
fn a_new_folder_gets_an_id_of_its_own_and_servers_left_out_keep_their_places_after() {
    let mut app = three_servers();
    let effects = app.dispatch(Action::ArrangeServers(vec![group(
        Some(NEW_FOLDER),
        &["30", "10"],
    )]));
    let entries = sent(&effects).unwrap();
    assert_eq!(entries.len(), 2);
    let made = &entries[0];
    assert!(
        made.id.parse::<u64>().is_ok() && made.id != "7",
        "{}",
        made.id
    );
    assert_eq!((&made.name, made.color), (&None, None));
    assert_eq!(made.servers, ["30", "10"]);
    assert_eq!(
        entries[1],
        ServerListEntry {
            servers: vec!["20".into()],
            ..ServerListEntry::default()
        },
        "B, left out, after it"
    );
}

/// The rail's folder "discord-folder:7": its name and colour as it shows.
fn folder_shown(app: &Application) -> Option<(Option<String>, Option<u32>)> {
    app.state
        .servers
        .iter()
        .find_map(|server| server.folder.as_ref())
        .filter(|folder| folder.id == "discord-folder:7")
        .map(|folder| (folder.name.clone(), folder.color))
}

fn set_folder(name: &str, color: Option<u32>) -> Action {
    Action::SetFolder {
        folder: "discord-folder:7".into(),
        name: name.into(),
        color,
    }
}

#[test]
fn a_folders_name_and_colour_show_at_once_and_go_with_the_whole_list() {
    let mut app = three_servers();
    let effects = app.dispatch(set_folder("Raids", Some(0x3498db)));
    assert_eq!(
        sent(&effects),
        Some(&vec![
            ServerListEntry {
                servers: vec!["10".into()],
                ..ServerListEntry::default()
            },
            ServerListEntry {
                id: "7".into(),
                name: Some("Raids".into()),
                color: Some(0x3498db),
                servers: vec!["20".into(), "30".into()],
            },
        ]),
        "the order as it was, the folder with its servers"
    );
    assert_eq!(
        folder_shown(&app),
        Some((Some("Raids".into()), Some(0x3498db)))
    );
    // The same again asks nothing; a name past the limit is cut to it.
    assert!(app.dispatch(set_folder("Raids", Some(0x3498db))).is_empty());
    let long = "x".repeat(FOLDER_NAME_LIMIT + 5);
    app.dispatch(set_folder(&long, Some(0x3498db)));
    let (name, _) = folder_shown(&app).unwrap();
    assert_eq!(
        name.map(|name| name.chars().count()),
        Some(FOLDER_NAME_LIMIT)
    );
}

#[test]
fn an_empty_name_and_the_default_colour_are_left_out() {
    let mut app = three_servers();
    let effects = app.dispatch(set_folder("  ", None));
    let entries = sent(&effects).expect("the list goes");
    assert_eq!(
        (&entries[1].id, &entries[1].name, entries[1].color),
        (&"7".to_owned(), &None, None),
        "neither a blank name nor the default colour is sent"
    );
    assert_eq!(folder_shown(&app), Some((None, None)));
    // Not a folder, or not a Discord one: nothing.
    assert!(
        app.dispatch(Action::SetFolder {
            folder: "discord-folder:8".into(),
            name: "Other".into(),
            color: None,
        })
        .is_empty()
    );
    assert!(
        app.dispatch(Action::SetFolder {
            folder: "7".into(),
            name: "Other".into(),
            color: None,
        })
        .is_empty()
    );
}

#[test]
fn a_refused_folder_change_puts_its_name_and_colour_back_with_its_own_notice() {
    let mut app = three_servers();
    app.dispatch(set_folder("", None));
    assert_eq!(folder_shown(&app), Some((None, None)));
    app.apply(Event::ServersArranged(Err("Too many requests".into())));
    assert_eq!(
        folder_shown(&app),
        Some((Some("Games".into()), Some(0xed4245)))
    );
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Couldn't change the folder: Too many requests")
    );
}

#[test]
fn a_refusal_puts_the_order_back_with_a_notice_and_another_devices_list_replaces_it() {
    let mut app = three_servers();
    let before = shown(&app);
    app.dispatch(Action::ArrangeServers(vec![group(None, &["30"])]));
    assert_ne!(shown(&app), before);
    app.apply(Event::ServersArranged(Err("Too many requests".into())));
    assert_eq!(shown(&app), before);
    assert_eq!(
        app.state.notice.as_deref(),
        Some("Couldn't move the server: Too many requests")
    );
    // Another device's list, as Discord's settings bring it.
    app.apply(discord_event(snapshot(["30", "10", "20"], vec![])));
    assert_eq!(
        shown(&app),
        [
            ("discord:30".to_owned(), None),
            ("discord:10".to_owned(), None),
            ("discord:20".to_owned(), None),
        ]
    );
}
