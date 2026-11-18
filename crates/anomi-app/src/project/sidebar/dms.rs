//! The DM home's list: the DM half of `show_sidebar` (`src/ui/refresh.rs:1144`
//! -`:1474`), the requests' count of `request_rows` (`:6094`) and the line
//! under a DM (`dm_detail`, `src/ui.rs:3455`).

use std::collections::HashMap;

use yapfast_core::application::{AppState, DmScreen};
use yapfast_core::chat::{ChatRoom, Presence as CorePresence};
use yapfast_core::discord;
use yapfast_core::network::Network;
use yapfast_lang::trn;

use anomi_vm::kind::{NameStyle, Nameplate, Spot, Status, Tag, count_badge};
use anomi_vm::sidebar::dms::{DmRow, Dms, Empty, Voice};

use super::Key;
use super::people::{VoicePlace, voice_places};
use crate::pictures::{self, Requests};
use crate::project::rows;

pub(super) fn build(state: &AppState, key: &Key) -> Dms {
    let query = key.query();
    let waiting =
        |room: &&ChatRoom| room.category == discord::REQUESTS || room.category == discord::SPAM;
    let mut rooms: Vec<&ChatRoom> = state
        .scoped_rooms()
        .filter(|room| !waiting(room))
        .filter(|room| room.name.to_lowercase().contains(&query))
        .collect();
    // Pinned DMs come first, in the order they were pinned.
    rooms.sort_by_key(|room| {
        state
            .pinned
            .iter()
            .position(|id| *id == room.id)
            .unwrap_or(usize::MAX)
    });
    let rooms_by_id: HashMap<&str, &ChatRoom> = state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), &**room))
        .collect();
    let in_voice = voice_places(state, &rooms_by_id);
    let rows: Vec<DmRow> = rooms
        .into_iter()
        .map(|room| row(state, key, room, &in_voice))
        .collect();
    let empty = if !rows.is_empty() {
        Empty::None
    } else if state.scoped_rooms().next().is_some() {
        Empty::NoMatch
    } else {
        Empty::NoDms
    };
    Dms {
        friends_selected: key.screen == Some(DmScreen::Friends),
        requests_selected: key.screen == Some(DmScreen::Requests),
        requests_badge: count_badge(requests(state)),
        rows,
        no_discord: !key.linked,
        preview: key.preview,
        empty,
        ..Dms::default()
    }
}

fn row(
    state: &AppState,
    key: &Key,
    room: &ChatRoom,
    in_voice: &HashMap<&str, VoicePlace<'_>>,
) -> DmRow {
    let cosmetics = key.cosmetics;
    let discord = room.network == Network::Discord;
    let voice = room
        .person
        .as_deref()
        .filter(|_| discord)
        .and_then(|person| in_voice.get(person));
    // Discord sends the presence of friends and of those sharing a server;
    // anyone else in a one-to-one DM shows offline, as in Discord's list.
    let presence = if room.person.is_some() && discord {
        room.presence.or(Some(CorePresence::Offline))
    } else {
        room.presence
    };
    let (detail, status) = detail(state, room);
    let plate = room
        .nameplate
        .as_ref()
        .filter(|_| !key.hide_nameplates)
        .map(|plate| Nameplate {
            picture: pictures::find(state, Some(&plate.picture)),
            palette: plate
                .colors
                .map(|[dark, light]| [rows::rgb(dark), rows::rgb(light)]),
        })
        .unwrap_or_default();
    DmRow {
        id: room.id.clone(),
        name: room.name.clone(),
        avatar: rows::avatar(state, &room.name, &room.id, room.avatar.as_deref()),
        presence: rows::presence(presence),
        detail,
        selected: key.selected.as_deref() == Some(room.id.as_str()) && key.screen.is_none(),
        unread: room.unread > 0,
        badge: count_badge(room.mentions),
        muted: room.muted,
        decoration: pictures::find(state, cosmetics.decoration(room.decoration.as_deref())),
        name_style: cosmetics
            .style(room.style.as_deref())
            .and_then(|style| NameStyle::new(style.font, style.effect, style.colors())),
        plate,
        tag: cosmetics.tag(room.tag.as_ref()).map(|tag| Tag {
            text: tag.text.to_string(),
            badge: rows::expected(state, tag.badge.as_deref()),
            ..Tag::default()
        }),
        status,
        voice: voice.map(|place| Voice {
            room: place.room.to_owned(),
            spot: Spot::new(place.channel, place.server),
        }),
        official: room.official,
        wave: if room.empty && !room.request {
            room.person.clone().unwrap_or_default()
        } else {
            String::new()
        },
        call: room.direct && !room.people.is_empty(),
        pinned: state.pinned.contains(&room.id),
        discord: crate::project::rail::MARKS && room.direct && discord,
        ..DmRow::default()
    }
}

/// The line under a Discord DM: a group's size, or the person's status,
/// words and emoji (`dm_detail` and `activity_line`).
fn detail(state: &AppState, room: &ChatRoom) -> (String, Status) {
    if !room.direct || room.network != Network::Discord {
        return Default::default();
    }
    if room.person.is_none() {
        return (
            trn!("{n} member", "{n} members", room.members),
            Status::default(),
        );
    }
    let (emoji, words) = room
        .activity
        .as_deref()
        .map_or((None, ""), discord::status_parts);
    let status = Status {
        text: words.split_whitespace().collect::<Vec<_>>().join(" "),
        emoji: rows::expected(state, emoji.as_deref()),
        ..Status::default()
    };
    (String::new(), status)
}

/// The requests waiting for an answer, as Requests' badge counts them: each
/// friend request asked of this account or with a message, and each message
/// request from someone who didn't ask to be friends; none taken for spam.
fn requests(state: &AppState) -> u64 {
    let waiting: Vec<&ChatRoom> = state
        .scoped_rooms()
        .filter(|room| room.category == discord::REQUESTS || room.category == discord::SPAM)
        .collect();
    let befriending = state
        .friend_requests
        .iter()
        .filter(|request| {
            let room = waiting
                .iter()
                .find(|room| room.person.as_deref() == Some(request.id.as_str()));
            let spammed = request.spam || room.is_some_and(|room| room.category == discord::SPAM);
            !spammed && (request.incoming || room.is_some())
        })
        .count();
    let messages = waiting
        .iter()
        .filter(|room| room.category == discord::REQUESTS)
        .filter(|room| {
            !state
                .friend_requests
                .iter()
                .any(|request| room.person.as_deref() == Some(request.id.as_str()))
        })
        .count();
    (befriending + messages) as u64
}

/// Asks for the pictures of the DMs in view, by room id: each one's avatar,
/// decoration, server tag's badge and status emoji. A nameplate shows only
/// under the pointer or chosen, and the core asks for the chosen one's.
pub(super) fn seen(state: &AppState, ids: &[String], requests: &mut Requests) {
    let cosmetics = state.cosmetics;
    let mut wanted: Vec<&str> = Vec::new();
    for room in state.rooms.iter().filter(|room| ids.contains(&room.id)) {
        wanted.extend(room.avatar.as_deref());
        wanted.extend(cosmetics.decoration(room.decoration.as_deref()));
        wanted.extend(
            cosmetics
                .tag(room.tag.as_ref())
                .and_then(|tag| tag.badge.as_deref()),
        );
        if room.network == Network::Discord {
            let emoji = room
                .activity
                .as_deref()
                .and_then(|activity| discord::status_parts(activity).0);
            if let Some(emoji) = emoji {
                requests.want(state, [emoji.as_str()]);
            }
        }
    }
    requests.want(state, wanted);
}
