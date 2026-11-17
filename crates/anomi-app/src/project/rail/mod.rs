//! The rail: `rail` and `rail_rows` in `src/ui/refresh.rs`. Folders, the
//! unread DMs above the servers and the call bubble come with N02 and I02.
//!
//! The screens' tasks may add fields to the view-models built here: struct
//! literals end in `..Type::default()`, so a new field keeps its
//! default until this area's I task fills it.

#![allow(clippy::needless_update)]

use std::collections::HashMap;

use yapfast_core::application::{AppState, Navigation};
use yapfast_core::chat::{ChatRoom, ChatServer};

use anomi_vm::kind::count_badge;
use anomi_vm::rail::{Rail, Server, Tile};

use super::rows;

/// What the rail shows changes with the lists, the pictures, where the
/// person is and the friend requests waiting.
#[derive(PartialEq)]
pub(super) struct Key {
    lists: u64,
    pictures: u64,
    navigation: Navigation,
    requests: u64,
}

pub(super) fn show(state: &AppState, rail: &mut Rail, shown: &mut Option<Key>) {
    // As in Discord, the DM home counts DM mentions and the friend requests
    // waiting for an answer (`src/ui/refresh.rs`, beside `set_dms_badge`).
    let requests = state
        .friend_requests
        .iter()
        .filter(|request| request.incoming && !request.spam)
        .count() as u64;
    let key = Key {
        lists: state.list_changes,
        pictures: state.picture_changes,
        navigation: state.navigation.clone(),
        requests,
    };
    if shown.as_ref() == Some(&key) {
        return;
    }
    let rooms: HashMap<&str, &ChatRoom> = state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), &**room))
        .collect();
    let dm_mentions: u64 = state
        .rooms
        .iter()
        .filter(|room| room.direct)
        .map(|room| room.mentions)
        .sum();
    rail.home = Tile {
        selected: key.navigation == Navigation::DirectMessages,
        unread: state
            .rooms
            .iter()
            .any(|room| room.direct && room.unread > 0),
        badge: count_badge(dm_mentions + requests),
        ..Tile::default()
    };
    rail.servers = state
        .servers
        .iter()
        .map(|server| row(state, server, &rooms, &key.navigation))
        .collect();
    *shown = Some(key);
}

fn row(
    state: &AppState,
    server: &ChatServer,
    rooms: &HashMap<&str, &ChatRoom>,
    at: &Navigation,
) -> Server {
    // As Discord's client marks a server: any unread channel it lists that
    // isn't muted or a voice channel's chat, and every mention.
    let channels = || {
        server
            .channels
            .iter()
            .filter_map(|channel| rooms.get(channel.as_str()))
            .filter(|room| !room.direct)
    };
    let unread = channels().any(|room| {
        room.mentions > 0
            || (room.unread > 0
                && !server.muted
                && !room.unlisted
                && !(server.discord && room.voice))
    });
    let mentions: u64 = channels().map(|room| room.mentions).sum();
    let mut avatar = rows::avatar(state, &server.name, &server.id, server.icon.as_deref());
    avatar.initial = rows::initials(&server.name);
    Server {
        id: server.id.clone(),
        name: server.name.clone(),
        avatar,
        tile: Tile {
            selected: *at == Navigation::Server(server.id.clone()),
            unread,
            badge: count_badge(mentions),
            ..Tile::default()
        },
        call: false,
        ..Server::default()
    }
}
