//! What stands around the rows: the open room as the start of its history
//! names it (`room-name`, `intro-kind`, `show_header`,
//! `src/ui/refresh.rs:2146`-`:2180`), what the rows are made with
//! ([`look`]), and the bar's words to the "New" divider (`unread_label`,
//! `:5598`).

use std::sync::Arc;

use yapfast_core::application::{Application, Navigation};
use yapfast_core::chat::{ChatMessage, ChatRoom, Reacts};
use yapfast_core::clock;
use yapfast_core::discord;
use yapfast_core::network::Network;
use yapfast_lang::{tr, trn};

use anomi_vm::chat::message::Size;
use anomi_vm::chat::timeline::{Intro, Kind, Room};

use crate::layout::Layout;
use crate::project::chat::message::Look;
use crate::project::rows;

/// The open room as its timeline names it.
pub(super) fn room(core: &Application, room: &ChatRoom) -> Room {
    let state = &core.state;
    let kind = if room.direct {
        Kind::Direct(rows::avatar(
            state,
            &room.name,
            &room.id,
            room.avatar.as_deref(),
        ))
    } else if room.parent.is_some() {
        Kind::Thread(rows::channel_icon(room))
    } else {
        Kind::Channel(rows::channel_icon(room))
    };
    Room {
        id: room.id.clone(),
        name: room.name.clone(),
        kind,
        intro: intro(core, room),
    }
}

/// What opens a server channel's history in Discord's client: "Welcome to"
/// the server for its first channel, else the channel itself.
fn intro(core: &Application, room: &ChatRoom) -> Option<Intro> {
    let state = &core.state;
    let Navigation::Server(id) = &state.navigation else {
        return None;
    };
    if room.network != Network::Discord || room.direct || room.voice || room.parent.is_some() {
        return None;
    }
    let server = state.servers.iter().find(|server| &server.id == id)?;
    let first = server
        .channels
        .iter()
        .filter_map(|channel| state.rooms.iter().find(|room| &room.id == channel))
        .find(|room| !room.voice && room.parent.is_none() && !room.forum)
        .is_some_and(|first| first.id == room.id);
    Some(if first {
        Intro::Server(server.name.clone())
    } else {
        Intro::Channel
    })
}

/// The settings and rights the rows are made with, now.
pub(super) fn look(core: &Application, layout: &Layout, now: u64) -> Look {
    let state = &core.state;
    let room = state.selected.as_deref();
    let reacts = room
        .filter(|_| state.shows_text())
        .map_or(Reacts::None, |room| state.reacts_in(room));
    Look {
        twelve_hour: layout.twelve_hour_clock,
        seconds: layout.clock_seconds,
        compact: layout.compact_messages,
        size: match layout.text_size {
            Some(0) => Size::Small,
            Some(2) => Size::Large,
            _ => Size::Normal,
        },
        link_previews: layout.link_previews.unwrap_or(true),
        collapse_long: layout.collapse_long,
        cosmetics: state.cosmetics,
        in_server: room
            .and_then(discord::parse_room)
            .is_some_and(|(guild, _)| guild != discord::DM_GUILD),
        preview: state.preview,
        writes: room.is_some_and(|room| state.can_write_in(room)),
        reacts,
        forwards: state.preview
            || state.trusted
            || room.is_some_and(|room| Network::of(room) == Network::Discord),
        threads: core.can_start_threads(),
        today: clock::date(now),
        date_format: layout.date_format,
    }
}

/// What the bar that jumps to the "New" divider says: how many messages
/// others sent from `unread` on, and since when, with the date unless it was
/// today. `more` came after them, past a window of older messages.
pub(super) fn unread_label(unread: &[Arc<ChatMessage>], more: bool, now: u64) -> String {
    let count = unread.iter().filter(|message| !message.own).count();
    let single = count == 1 && !more;
    let Some(first) = unread.first().filter(|first| first.time.contains(':')) else {
        return match (more, single) {
            (true, _) => tr!("{}+ new messages", count),
            (false, true) => tr!("1 new message").to_owned(),
            (false, false) => trn!("{n} new message", "{n} new messages", count),
        };
    };
    let on = clock::day(first.timestamp);
    if on.is_empty() || on == clock::day(now) {
        match (more, single) {
            (true, _) => tr!("{}+ new messages since {}", count, first.time),
            (false, true) => tr!("1 new message since {}", first.time),
            (false, false) => trn!(
                "{n} new message since {}",
                "{n} new messages since {}",
                count,
                first.time
            ),
        }
    } else {
        match (more, single) {
            (true, _) => tr!("{}+ new messages since {} on {}", count, first.time, on),
            (false, true) => tr!("1 new message since {} on {}", first.time, on),
            (false, false) => trn!(
                "{n} new message since {} on {}",
                "{n} new messages since {} on {}",
                count,
                first.time,
                on
            ),
        }
    }
}

/// The next local midnight after `now`, in Unix seconds: when "Today"
/// turns to "Yesterday".
pub(super) fn next_midnight(now: u64) -> u64 {
    use chrono::{Duration, Local, TimeZone};
    let Some(today) = clock::date(now) else {
        return now + 24 * 3600;
    };
    let tomorrow = today + Duration::days(1);
    tomorrow
        .and_hms_opt(0, 0, 0)
        .and_then(|midnight| Local.from_local_datetime(&midnight).earliest())
        .map_or(now + 24 * 3600, |midnight| {
            u64::try_from(midnight.timestamp()).unwrap_or(now)
        })
        .max(now + 1)
}
