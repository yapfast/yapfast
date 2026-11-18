//! The inbox popout from the core's state: the rooms with something waiting,
//! as `show_inbox` (`src/ui/refresh.rs:1746`) lists them. Its rows are
//! built only while it is open, again as the rooms change or the tab does,
//! and let go once it has played its way out, as Slint emptied its model.

use std::collections::HashMap;

use yapfast_core::application::AppState;
use yapfast_core::chat::ChatRoom;

use anomi_vm::inbox::{Inbox, Row, Tab};
use anomi_vm::kind::{Avatar, count_badge};
use anomi_vm::shell::{Layer, Shell};

use super::rows;
use crate::pictures::Requests;

/// What the inbox showed last, and the rows in view by room id.
#[derive(Default)]
pub(super) struct Shown {
    key: Option<Key>,
    seen: Vec<String>,
}

#[derive(PartialEq)]
struct Key {
    lists: u64,
    pictures: u64,
    tab: Tab,
}

pub(super) fn show(
    state: &AppState,
    shell: &Shell,
    inbox: &mut Inbox,
    shown: &mut Shown,
    requests: &mut Requests,
) {
    if !shell.is_open(Layer::Inbox) {
        return;
    }
    let key = Key {
        lists: state.list_changes,
        pictures: state.picture_changes,
        tab: inbox.tab,
    };
    if shown.key.as_ref() == Some(&key) {
        return;
    }
    inbox.rows = rows(state, inbox.tab);
    want(state, &shown.seen, requests);
    shown.key = Some(key);
}

/// The popout played its way out: its rows go.
pub(super) fn gone(inbox: &mut Inbox, shown: &mut Shown) {
    inbox.rows = Vec::new();
    *shown = Shown::default();
}

/// The rows in view, by room id: their conversations' pictures are asked
/// for, now and as the rows are built again.
pub(super) fn seen(state: &AppState, shown: &mut Shown, ids: Vec<String>, requests: &mut Requests) {
    want(state, &ids, requests);
    shown.seen = ids;
}

fn want(state: &AppState, ids: &[String], requests: &mut Requests) {
    requests.want(
        state,
        state
            .rooms
            .iter()
            .filter(|room| room.direct && ids.contains(&room.id))
            .filter_map(|room| room.avatar.as_deref()),
    );
}

/// The rooms that mention this account, and on Unreads those with unread
/// messages too, a muted one only for a mention; no message request.
fn rows(state: &AppState, tab: Tab) -> Vec<Row> {
    let mentions_only = tab == Tab::Mentions;
    let waiting = |room: &ChatRoom| {
        !room.request
            && !room.unlisted
            && (room.mentions > 0 || (!mentions_only && room.unread > 0 && !room.muted))
    };
    if !state.rooms.iter().any(|room| waiting(room)) {
        return Vec::new();
    }
    let servers: HashMap<&str, &str> = state
        .servers
        .iter()
        .flat_map(|server| {
            let name = server.name.as_str();
            server.channels.iter().map(move |id| (id.as_str(), name))
        })
        .collect();
    state
        .rooms
        .iter()
        .filter(|room| waiting(room))
        .map(|room| Row {
            id: room.id.clone(),
            name: if room.direct {
                room.name.clone()
            } else {
                format!("#{}", room.name)
            },
            place: servers
                .get(room.id.as_str())
                .copied()
                .unwrap_or_default()
                .to_owned(),
            // A channel keeps its initial on its tint.
            avatar: if room.direct {
                rows::avatar(state, &room.name, &room.id, room.avatar.as_deref())
            } else {
                Avatar {
                    initial: rows::initial(&room.name),
                    tint: rows::tint(&room.id),
                    ..Avatar::default()
                }
            },
            badge: count_badge(room.mentions),
        })
        .collect()
}
