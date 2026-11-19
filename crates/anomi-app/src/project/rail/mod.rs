//! The rail: `rail`, `rail_rows`, `unread_dm_rows` and `server_row` in
//! `src/ui/refresh.rs` (`:5367`-`:5588`), with the DM home's tile of
//! `show_workspace` (`:981`-`:1001`). The unread DMs above the servers, each
//! server and each folder with its servers after it, the speaker on the
//! server of the call going on.
//!
//! The pictures it shows are asked for as the rail reports the rows in view
//! ([`seen`]): the core leaves list pictures to the interface.

#![allow(clippy::needless_update)]

#[cfg(test)]
pub(super) mod tests;

use std::collections::{BTreeSet, HashMap};

use iced::Color;
use yapfast_core::application::{AppState, Navigation, ROLE_COLORS};
use yapfast_core::chat::{ChatRoom, ChatServer};
use yapfast_core::discord;
use yapfast_core::network::Network;
use yapfast_lang::tr;

use anomi_vm::kind::{Avatar, count_badge};
use anomi_vm::rail::folder::FolderSettings;
use anomi_vm::rail::{Dm, FOLDER_COLOR, Folder, Rail, Server, Tile};

use super::rows;
use crate::layout::Layout;
use crate::pictures::{self, Requests};

/// Discord's marks show only where Matrix is built in beside it, as the
/// Slint app's `Badges.discord`.
pub(super) const MARKS: bool = cfg!(feature = "matrix");

/// What the rail was built from, and the ids of the rows last in view.
#[derive(Default)]
pub(super) struct Shown {
    key: Option<Key>,
    seen: Vec<String>,
    /// The pictures the rows show: one coming builds them again.
    pictures: pictures::Watch<()>,
}

/// What the rail shows changes with the lists, where the person is, the
/// friend requests waiting, the folders open and the call; and with its
/// pictures, which [`Shown::pictures`] watches.
struct Key {
    lists: u64,
    navigation: Navigation,
    /// The core's `people_changes` when the requests waiting were counted.
    people: u64,
    requests: u64,
    open: BTreeSet<String>,
    call: Option<String>,
    can_join: bool,
}

pub(super) fn show(
    state: &AppState,
    layout: &Layout,
    rail: &mut Rail,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    // The requests are counted again only once the people lists changed.
    let people = state.people_changes;
    let waiting = match &shown.key {
        Some(key) if key.people == people => key.requests,
        _ => waiting(state),
    };
    let call = call_room(state);
    let pictured = !shown.pictures.moved(state).is_empty();
    let fresh = pictured
        || shown.key.as_ref().is_none_or(|key| {
            key.lists != state.list_changes
                || key.navigation != state.navigation
                || key.requests != waiting
                || key.open != layout.open_folders
                || key.call.as_deref() != call
                || key.can_join != (state.discord_online || state.preview)
        });
    if !fresh {
        if let Some(key) = &mut shown.key {
            key.people = people;
        }
        return;
    }
    let key = Key {
        lists: state.list_changes,
        navigation: state.navigation.clone(),
        people,
        requests: waiting,
        open: layout.open_folders.clone(),
        call: call.map(str::to_owned),
        can_join: state.discord_online || state.preview,
    };
    pictures::noting(state, &mut shown.pictures, || build(state, &key, rail));
    seen(state, &shown.seen, requests);
    shown.key = Some(key);
}

/// As in Discord, the DM home counts DM mentions and the friend requests
/// waiting for an answer (`src/ui/refresh.rs:989`).
fn waiting(state: &AppState) -> u64 {
    state
        .friend_requests
        .iter()
        .filter(|request| request.incoming && !request.spam)
        .count() as u64
}

fn build(state: &AppState, key: &Key, rail: &mut Rail) {
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
        badge: count_badge(dm_mentions + key.requests),
        ..Tile::default()
    };
    rail.dms = unread_dms(state);
    rail.can_join = key.can_join;
    let marks = |server: &ChatServer| marks(server, &rooms);
    let in_call = |server: &ChatServer| {
        key.call
            .as_deref()
            .is_some_and(|room| server.channels.iter().any(|channel| channel == room))
    };
    let selected = |server: &ChatServer| key.navigation == Navigation::Server(server.id.clone());
    rail.servers = state
        .servers
        .iter()
        .map(|server| {
            let (unread, mentions) = marks(server);
            Server {
                call: in_call(server),
                ..server_row(state, server, selected(server), unread, mentions)
            }
        })
        .collect();
    let mut folders = Vec::new();
    let mut servers = state.servers.iter().peekable();
    while let Some(server) = servers.next() {
        let Some(folder) = &server.folder else {
            continue;
        };
        let mut inside: Vec<&ChatServer> = vec![&**server];
        while let Some(next) = servers.next_if(|next| next.folder.as_ref() == Some(folder)) {
            inside.push(&**next);
        }
        let (unread, mentions) = inside
            .iter()
            .map(|server| marks(server))
            .fold((false, 0), |(unread, mentions), (one, more)| {
                (unread || one, mentions + more)
            });
        folders.push(Folder {
            id: folder.id.clone(),
            name: folder_name(folder.name.as_deref(), &inside),
            tint: folder.color.map_or(FOLDER_COLOR, |rgb| {
                let [_, red, green, blue] = rgb.to_be_bytes();
                Color::from_rgb8(red, green, blue)
            }),
            open: key.open.contains(&folder.id),
            tile: Tile {
                selected: inside.iter().any(|server| selected(server)),
                unread,
                badge: count_badge(mentions),
                ..Tile::default()
            },
            call: inside.iter().any(|server| in_call(server)),
            discord: MARKS,
            ..Folder::default()
        });
    }
    rail.folders = folders;
}

/// A server's channels: whether any is unread, and its mentions. One its
/// channel list leaves out, or a Discord voice channel's chat, marks it only
/// by mentions, as in Discord's client (GuildReadStateStore).
fn marks(server: &ChatServer, rooms: &HashMap<&str, &ChatRoom>) -> (bool, u64) {
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
    (unread, channels().map(|room| room.mentions).sum())
}

/// Folder `id`'s settings as they open: its own name and colour, the name
/// the rail gives it without one for the field's placeholder, and the
/// twenty colours Discord offers a role.
pub(crate) fn folder_settings(state: &AppState, id: &str) -> Option<FolderSettings> {
    let inside: Vec<&ChatServer> = state
        .servers
        .iter()
        .map(|server| &**server)
        .filter(|server| server.folder.as_ref().is_some_and(|folder| folder.id == id))
        .collect();
    let folder = inside.first()?.folder.as_ref()?;
    Some(FolderSettings::opened(
        id,
        folder.name.as_deref().unwrap_or_default(),
        &folder_name(None, &inside),
        folder.color,
        &ROLE_COLORS,
    ))
}

/// A folder's name: its own, or its first three servers' joined by commas.
fn folder_name(own: Option<&str>, inside: &[&ChatServer]) -> String {
    own.map_or_else(
        || {
            let mut names: Vec<&str> = inside
                .iter()
                .take(3)
                .map(|server| server.name.as_str())
                .collect();
            if inside.len() > 3 {
                names.push("…");
            }
            names.join(", ")
        },
        str::to_owned,
    )
}

fn server_row(
    state: &AppState,
    server: &ChatServer,
    selected: bool,
    unread: bool,
    mentions: u64,
) -> Server {
    // On the theme's ground until its icon comes, as Slint's server rows.
    let avatar = Avatar {
        initial: rows::initials(&server.name),
        picture: pictures::find(state, server.icon.as_deref()),
        ..Avatar::default()
    };
    let tip = if server.unavailable {
        tr!("{} (unavailable in a Discord outage)", server.name)
    } else if server.discord && MARKS {
        tr!("{} (Discord)", server.name)
    } else {
        String::new()
    };
    Server {
        id: server.id.clone(),
        name: server.name.clone(),
        tip,
        avatar,
        tile: Tile {
            selected,
            unread,
            badge: count_badge(mentions),
            ..Tile::default()
        },
        discord: server.discord && MARKS,
        unavailable: server.unavailable,
        folder: server.folder.as_ref().map(|folder| folder.id.clone()),
        // Discord's settings keep a Discord server's place.
        movable: server.discord && !state.preview,
        ..Server::default()
    }
}

/// The DMs with unread messages, above the servers (`unread_dm_rows`).
fn unread_dms(state: &AppState) -> Vec<Dm> {
    state
        .rooms
        .iter()
        .filter(|room| {
            room.direct
                && !room.request
                && room.category != discord::SPAM
                && (room.unread > 0 || room.mentions > 0)
        })
        .map(|room| Dm {
            id: room.id.clone(),
            name: room.name.clone(),
            avatar: Avatar {
                initial: rows::initial(&room.name),
                picture: pictures::find(state, room.avatar.as_deref()),
                ..Avatar::default()
            },
            // Discord counts each of a DM's messages; Matrix counts its
            // unread messages apart from its mentions.
            badge: count_badge(room.mentions.max(room.unread)),
            discord: MARKS && room.network == Network::Discord,
            ..Dm::default()
        })
        .collect()
}

/// The voice channel of the call going on, if there is one.
pub(super) fn call_room(state: &AppState) -> Option<&str> {
    state
        .voice
        .active()
        .then_some(state.voice.room.as_deref())
        .flatten()
}

/// Remembers the rows in view, by id, and asks for their pictures: a
/// server's icon, a folder's first four servers' (its tile shows them), an
/// unread DM's picture.
pub(super) fn seen_now(
    state: &AppState,
    shown: &mut Shown,
    ids: Vec<String>,
    requests: &mut Requests,
) {
    seen(state, &ids, requests);
    shown.seen = ids;
}

fn seen(state: &AppState, ids: &[String], requests: &mut Requests) {
    let mut wanted = Vec::new();
    for id in ids {
        if let Some(server) = state.servers.iter().find(|server| server.id == *id) {
            wanted.extend(server.icon.as_deref());
        } else if let Some(room) = state.rooms.iter().find(|room| room.id == *id) {
            wanted.extend(room.avatar.as_deref());
        } else {
            wanted.extend(
                state
                    .servers
                    .iter()
                    .filter(|server| server.folder.as_ref().is_some_and(|f| f.id == *id))
                    .take(4)
                    .filter_map(|server| server.icon.as_deref()),
            );
        }
    }
    requests.want(state, wanted);
}
