//! Places and devices: a chosen device gone away, who is in a server's
//! voice channel, how a voice channel's chat and a call mark the rail.

use std::collections::HashMap;
use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::chat::{ChatRoom, ChatServer, ServerFolder, VoicePerson};
use yapfast_core::voice::{Device, Phase};

use anomi_vm::rail::Rail;

use super::preview;
use crate::pictures::Requests;
use crate::project::rail::tests::layout;
use crate::project::rail::{Shown, show};
use crate::project::sidebar::people::voice_places;
use crate::project::voice::devices::rows;

#[test]
fn a_chosen_device_that_went_away_shows_as_not_connected() {
    let device = |id: &str, name: &str| Device {
        id: id.into(),
        name: name.into(),
    };
    let mut names = HashMap::new();
    let both = [
        device("{speakers}", "Speakers"),
        device("{headphones}", "Headphones"),
    ];
    let (listed, index) = rows(&both, Some("{headphones}"), &mut names);
    assert_eq!(listed, ["Speakers", "Headphones"]);
    assert_eq!(index, Some(1));
    // They go away: still chosen, by the name they had, not another device.
    let (listed, index) = rows(&both[..1], Some("{headphones}"), &mut names);
    assert_eq!(listed, ["Speakers", "Headphones (not connected)"]);
    assert_eq!(index, Some(1));
    // A device known by its name, as on PulseAudio, has it in its id.
    let (listed, index) = rows(&both[..1], Some("name:USB headset"), &mut HashMap::new());
    assert_eq!(listed, ["Speakers", "USB headset (not connected)"]);
    assert_eq!(index, Some(1));
    // The default entry's id names the default it had; it stays the default.
    let pulse = [
        device("name:default: Headset", "default: Headset"),
        device("name:Headset", "Headset"),
        device("name:Built-in", "Built-in"),
    ];
    let (listed, index) = rows(&pulse, Some("name:default: Built-in"), &mut names);
    assert_eq!(listed.len(), 3);
    assert_eq!(index, Some(0));
    let (listed, index) = rows(&[], None, &mut names);
    assert!(listed.is_empty());
    assert_eq!(index, None, "nothing to choose");
    assert_eq!(
        rows(&both, None, &mut names).1,
        Some(0),
        "the default, first"
    );
}

/// The preview's first room as `id`.
fn room(state: &AppState, id: &str, voice: bool, direct: bool) -> ChatRoom {
    ChatRoom {
        id: id.into(),
        name: id.into(),
        voice,
        direct,
        unread: 0,
        mentions: 0,
        people: Vec::new(),
        ..(*state.rooms[0]).clone()
    }
}

fn server(id: &str, channels: &[&str], discord: bool) -> ChatServer {
    ChatServer {
        id: id.into(),
        name: id.into(),
        channels: channels.iter().map(|&id| id.to_owned()).collect(),
        discord,
        ..ChatServer::default()
    }
}

fn by_id(state: &AppState) -> HashMap<&str, &ChatRoom> {
    state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), &**room))
        .collect()
}

#[test]
fn friends_in_a_servers_voice_channel_show_where() {
    let person = |id: &str| VoicePerson {
        id: id.into(),
        name: id.into(),
        ..Default::default()
    };
    let mut state = preview().state;
    let lounge = ChatRoom {
        people: vec![person("ann"), person("bob")],
        ..room(&state, "lounge", true, false)
    };
    // A DM call isn't a server's voice channel.
    let dm = ChatRoom {
        people: vec![person("cat")],
        ..room(&state, "dm", true, true)
    };
    state.rooms = [lounge, room(&state, "general", false, false), dm]
        .map(Arc::new)
        .into();
    state.servers = vec![Arc::new(server("Alpha", &["lounge", "general"], true))];
    let rooms = by_id(&state);
    let places = voice_places(&state, &rooms);
    let mut found: Vec<_> = places
        .iter()
        .map(|(&user, place)| (user, place.room, place.channel, place.server))
        .collect();
    found.sort_unstable();
    assert_eq!(
        found,
        [
            ("ann", "lounge", "lounge", "Alpha"),
            ("bob", "lounge", "lounge", "Alpha"),
        ]
    );
}

/// The rail as `state` makes it, with `open` folders.
fn rail(state: &AppState, open: &[&str]) -> Rail {
    let mut layout = layout();
    layout.edit(|saved| saved.open_folders = open.iter().map(|&id| id.to_owned()).collect());
    let mut rail = Rail::default();
    let mut requests = Requests::default();
    show(
        state,
        &layout,
        &mut rail,
        &mut Shown::default(),
        &mut requests,
    );
    rail
}

#[test]
fn a_voice_channels_chat_marks_its_server_only_by_mentions() {
    let unread_server = |rooms: &[(&str, bool, u64, u64)], discord| {
        let mut state = preview().state;
        let made: Vec<_> = rooms
            .iter()
            .map(|&(id, voice, unread, mentions)| ChatRoom {
                unread,
                mentions,
                ..room(&state, id, voice, false)
            })
            .collect();
        state.rooms = made.into_iter().map(Arc::new).collect();
        let ids: Vec<&str> = rooms.iter().map(|room| room.0).collect();
        state.servers = vec![Arc::new(server("Alpha", &ids, discord))];
        state.list_changes += 1;
        let tile = rail(&state, &[]).servers.remove(0).tile;
        (tile.unread, tile.badge)
    };
    assert_eq!(
        unread_server(&[("lounge", true, 3, 0), ("general", false, 0, 0)], true),
        (false, String::new()),
        "Discord's client leaves a server read for its voice channels' chat"
    );
    assert_eq!(
        unread_server(&[("lounge", true, 3, 1)], true),
        (true, "1".into()),
        "a mention there still counts"
    );
    assert_eq!(
        unread_server(&[("lounge", true, 3, 0), ("general", false, 1, 0)], true),
        (true, String::new())
    );
    assert_eq!(
        unread_server(&[("lounge", true, 3, 0)], false),
        (true, String::new()),
        "a Matrix space's call room stays as it was"
    );
}

/// The server's own call is `rail`'s
/// `the_call_shows_on_its_server_and_rebuilds_the_rail_once`.
#[test]
fn a_call_shows_on_its_servers_icon_and_its_closed_folder() {
    let mut state = preview().state;
    let folder = ServerFolder {
        id: "discord-folder:7".into(),
        name: None,
        color: Some(0xff0000),
    };
    let rooms = ["alpha-lounge", "bravo-lounge"].map(|id| Arc::new(room(&state, id, true, false)));
    state.rooms = rooms.to_vec();
    state.servers = vec![
        Arc::new(server("Alpha", &["alpha-lounge"], true)),
        Arc::new(ChatServer {
            folder: Some(folder),
            ..server("Bravo", &["bravo-lounge"], true)
        }),
    ];
    let calls = |state: &AppState, open: &[&str]| {
        let rail = rail(state, open);
        let servers: Vec<_> = rail
            .servers
            .iter()
            .map(|server| (server.id.clone(), server.call))
            .collect();
        (servers, rail.folders[0].call)
    };
    let none = (
        vec![("Alpha".to_owned(), false), ("Bravo".to_owned(), false)],
        false,
    );
    assert_eq!(calls(&state, &[]), none);
    // A voice channel chosen but not joined yet shows nothing.
    state.voice.room = Some("bravo-lounge".into());
    assert_eq!(calls(&state, &[]), none);
    state.voice.phase = Phase::Connected;
    let in_bravo = (
        vec![("Alpha".to_owned(), false), ("Bravo".to_owned(), true)],
        true,
    );
    assert_eq!(calls(&state, &[]), in_bravo, "closed, the folder shows it");
    assert_eq!(
        calls(&state, &["discord-folder:7"]),
        in_bravo,
        "open, it and the server"
    );
    state.voice.room = Some("alpha-lounge".into());
    assert_eq!(
        calls(&state, &[]),
        (
            vec![("Alpha".to_owned(), true), ("Bravo".to_owned(), false)],
            false
        )
    );
}
