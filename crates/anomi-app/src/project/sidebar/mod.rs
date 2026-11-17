//! The sidebar: the DM list or a server's channels, from `show_sidebar` in
//! `src/ui/refresh.rs`. The skeleton lists them plainly; voice channels'
//! people, threads under their channels, folding and pinning headings come
//! with N03, N04 and I02.
//!
//! The screens' tasks may add fields to the view-models built here: struct
//! literals end in `..Type::default()`, so a new field keeps its
//! default until this area's I task fills it.

#![allow(clippy::needless_update)]

use yapfast_core::application::{AppState, DmScreen, Navigation};
use yapfast_core::chat::ChatRoom;
use yapfast_core::discord;

use anomi_vm::kind::count_badge;
use anomi_vm::sidebar::channels::{ChannelKind, ChannelRow, Channels};
use anomi_vm::sidebar::dms::{DmRow, Dms};
use anomi_vm::sidebar::server_header::ServerHeader;
use anomi_vm::sidebar::{Place, Server, Sidebar};

use super::rows;

#[derive(PartialEq)]
pub(super) struct Key {
    lists: u64,
    pictures: u64,
    navigation: Navigation,
    selected: Option<String>,
    screen: Option<DmScreen>,
    pinned: Vec<String>,
    filter: String,
}

pub(super) fn show(state: &AppState, sidebar: &mut Sidebar, shown: &mut Option<Key>) {
    let key = Key {
        lists: state.list_changes,
        pictures: state.picture_changes,
        navigation: state.navigation.clone(),
        selected: state.selected.clone(),
        screen: state.dm_screen,
        pinned: state.pinned.clone(),
        filter: sidebar.filter.to_lowercase(),
    };
    if shown.as_ref() == Some(&key) {
        return;
    }
    let listed = |room: &&ChatRoom| room.name.to_lowercase().contains(key.filter.trim());
    sidebar.place = match &key.navigation {
        Navigation::DirectMessages => Place::Dms(dms(state, &key, listed)),
        Navigation::Server(id) => Place::Server(server(state, id, &key, listed)),
    };
    sidebar.version += 1;
    *shown = Some(key);
}

fn dms(state: &AppState, key: &Key, listed: impl Fn(&&ChatRoom) -> bool) -> Dms {
    let waiting =
        |room: &&ChatRoom| room.category == discord::REQUESTS || room.category == discord::SPAM;
    let requests = state
        .scoped_rooms()
        .filter(|room| room.category == discord::REQUESTS)
        .count();
    let mut rooms: Vec<&ChatRoom> = state
        .scoped_rooms()
        .filter(|room| !waiting(room))
        .filter(listed)
        .collect();
    // Pinned DMs first, in the order they were pinned.
    rooms.sort_by_key(|room| {
        key.pinned
            .iter()
            .position(|id| *id == room.id)
            .unwrap_or(usize::MAX)
    });
    let rows = rooms
        .into_iter()
        .map(|room| DmRow {
            id: room.id.clone(),
            name: room.name.clone(),
            avatar: rows::avatar(state, &room.name, &room.id, room.avatar.as_deref()),
            presence: if room.person.is_some() {
                rows::presence(Some(
                    room.presence
                        .unwrap_or(yapfast_core::chat::Presence::Offline),
                ))
            } else {
                rows::presence(None)
            },
            detail: rows::dm_detail(room),
            selected: key.selected.as_deref() == Some(room.id.as_str()) && key.screen.is_none(),
            unread: room.unread > 0,
            badge: count_badge(room.mentions),
            muted: room.muted,
            ..DmRow::default()
        })
        .collect();
    Dms {
        friends_selected: key.screen == Some(DmScreen::Friends),
        requests_selected: key.screen == Some(DmScreen::Requests),
        requests_badge: count_badge(requests as u64),
        rows,
        ..Dms::default()
    }
}

fn server(state: &AppState, id: &str, key: &Key, listed: impl Fn(&&ChatRoom) -> bool) -> Server {
    let name = state
        .servers
        .iter()
        .find(|server| server.id == id)
        .map_or_else(String::new, |server| server.name.clone());
    let mut category = None;
    let rows = state
        .scoped_rooms()
        .filter(listed)
        .map(|room| {
            let this = (room.category_id.clone(), room.category.clone());
            let heading = if room.capabilities.categories && category.as_ref() != Some(&this) {
                category = Some(this);
                room.category.clone()
            } else {
                String::new()
            };
            ChannelRow {
                id: room.id.clone(),
                name: room.name.clone(),
                heading,
                kind: if room.voice {
                    ChannelKind::Voice
                } else if room.forum {
                    ChannelKind::Forum
                } else if room.parent.is_some() {
                    ChannelKind::Thread
                } else {
                    ChannelKind::Text
                },
                selected: key.selected.as_deref() == Some(room.id.as_str()),
                unread: room.unread > 0 && !room.muted,
                badge: count_badge(room.mentions),
                muted: room.muted,
                ..ChannelRow::default()
            }
        })
        .collect();
    Server {
        header: ServerHeader {
            name,
            ..ServerHeader::default()
        },
        channels: Channels {
            rows,
            ..Channels::default()
        },
    }
}
