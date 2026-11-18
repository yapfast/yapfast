//! A server's channel list: the channel half of `show_sidebar`
//! (`src/ui/refresh.rs:1296`-`:1546`) and the icons of `Icons::room`
//! (`:349`).

use yapfast_core::application::Application;
use yapfast_core::chat::{ChatRoom, MarkKind};
use yapfast_lang::tr;

use anomi_vm::icon::Icon;
use anomi_vm::kind::count_badge;
use anomi_vm::sidebar::channels::{ChannelKind, ChannelRow, Channels};

use super::drags::Rights;
use super::{Key, fold, people};

pub(super) fn build(
    core: &Application,
    server: &str,
    key: &Key,
    places: &mut Vec<String>,
    rights: &mut Rights,
) -> Channels {
    let state = &core.state;
    let query = key.query();
    let mut listed: Vec<&ChatRoom> = state
        .scoped_rooms()
        .filter(|room| room.name.to_lowercase().contains(&query))
        .collect();
    // A Discord server's pinned channels come first, each with its threads,
    // under a heading of their own; as in Discord, they leave their
    // categories rather than show twice.
    let found = state.servers.iter().find(|known| known.id == server);
    let pinned = found.map_or(&[][..], |server| server.pinned.as_slice());
    let pinned_at = |room: &ChatRoom| {
        pinned
            .iter()
            .position(|id| *id == room.id || room.parent.as_ref() == Some(id))
    };
    let mut in_pinned = 0;
    if !pinned.is_empty() {
        listed.sort_by_key(|room| pinned_at(room).unwrap_or(usize::MAX));
        in_pinned = listed
            .iter()
            .take_while(|room| pinned_at(room).is_some())
            .count();
    }
    let mut headings = Headings::default();
    let mut rows: Vec<ChannelRow> = listed
        .into_iter()
        .enumerate()
        .map(|(index, room)| {
            let (heading, category) = headings.next(room, index, in_pinned);
            row(core, key, room, heading, category, index < in_pinned)
        })
        .collect();
    // A channel's threads follow it, so a thread's line runs on to the next
    // row when that's a thread too.
    for index in 1..rows.len() {
        if rows[index].thread() && rows[index - 1].thread() {
            rows[index - 1].thread_continues = true;
        }
    }
    // Categories fold as their server last left them, except while
    // filtering; one without channels shows as its heading alone. The pinned
    // channels above them don't fold.
    if query.is_empty() {
        let rest = rows.split_off(in_pinned);
        let rest = fold::with_empty_categories(rest, &core.categories(server));
        rows.extend(match &key.collapsed {
            Some(collapsed) => fold::fold_collapsed(rest, collapsed, key.call.as_deref()),
            None => rest,
        });
    }
    let discord = found.is_some_and(|server| server.discord);
    // A Discord server's channels and categories drag to move, by their
    // places in the list. Pinned channels and threads stay where they are.
    if query.is_empty() && discord {
        fold::number(&mut rows, in_pinned, places);
        rights.follow(server, state);
        for row in &mut rows {
            if row.place > 0 {
                row.draggable = rights.may_drag(core, &row.id);
            }
            if row.heading_place > 0 {
                row.heading_draggable = rights.may_drag(core, &row.category);
            }
        }
    }
    for row in rows
        .iter_mut()
        .filter(|row| !row.heading.is_empty() && !row.pinned)
    {
        let category = core
            .category_of(&row.id)
            .or_else(|| state.is_category(&row.id).then(|| row.id.clone()));
        row.may_create = category.is_some_and(|category| core.may_create_channel(&category));
    }
    let empty = if !rows.is_empty() {
        String::new()
    } else if state.scoped_rooms().next().is_some() {
        tr!("No matching conversations.").to_owned()
    } else if discord {
        tr!("No channels in this server that you can see.").to_owned()
    } else {
        tr!("No joined channels in this server. Join a channel in another Matrix client to see it here.")
            .to_owned()
    };
    Channels {
        server: server.to_owned(),
        rows,
        // Discord's look leaves Browse Channels to the server's menu.
        browse: None,
        empty,
        ..Channels::default()
    }
}

/// The headings above the rows: the pinned channels', each Discord
/// category's as its first channel comes, or the text and voice channels'
/// where a server has no categories.
#[derive(Default)]
struct Headings<'a> {
    category: Option<(Option<&'a str>, &'a str)>,
    text: bool,
    voice: bool,
}

impl<'a> Headings<'a> {
    /// The heading row `index` starts, and what it folds by.
    fn next(&mut self, room: &'a ChatRoom, index: usize, pinned: usize) -> (String, String) {
        let heading = if index < pinned {
            if index == 0 {
                tr!("PINNED CHANNELS")
            } else {
                ""
            }
            .to_owned()
        } else if room.capabilities.categories {
            // Discord channels sit under their categories, which may share a
            // name.
            let this = (room.category_id.as_deref(), room.category.as_str());
            if self.category.replace(this) != Some(this) {
                let folds_by = room.category_id.as_deref().unwrap_or(&room.category);
                return (room.category.clone(), folds_by.to_owned());
            }
            String::new()
        } else if room.voice && !self.voice {
            self.voice = true;
            tr!("VOICE CHANNELS").to_owned()
        } else if !room.voice && !self.text {
            self.text = true;
            tr!("TEXT CHANNELS").to_owned()
        } else {
            String::new()
        };
        let category = heading.clone();
        (heading, category)
    }
}

fn row(
    core: &Application,
    key: &Key,
    room: &ChatRoom,
    heading: String,
    category: String,
    pinned: bool,
) -> ChannelRow {
    let kind = if room.voice {
        ChannelKind::Voice
    } else if room.forum {
        ChannelKind::Forum
    } else if room.parent.is_some() {
        ChannelKind::Thread
    } else {
        ChannelKind::Text
    };
    let voice = room.voice && !room.direct;
    ChannelRow {
        id: room.id.clone(),
        name: room.name.clone(),
        heading,
        kind,
        icon: Some(icon(room)),
        selected: key.selected.as_deref() == Some(room.id.as_str()),
        unread: room.unread > 0,
        badge: count_badge(room.mentions),
        // Discord doesn't dim a voice channel.
        muted: room.muted && !room.voice,
        locked: room.locked || room.join_locked,
        pinned,
        category,
        chat: room.voice_chat(),
        lasted: room
            .voice_started
            .map(|started| lasted(started, yapfast_core::clock::now_ms() / 1000))
            .unwrap_or_default(),
        people: if room.voice {
            people::of_room(core, room)
        } else {
            Vec::new()
        },
        summary: if voice {
            yapfast_core::voice_roster::Roster::of(&room.people).summary()
        } else {
            String::new()
        },
        ..ChannelRow::default()
    }
}

/// How long a voice channel's call has gone on, by the minute, as Discord's
/// client counts it.
pub(super) fn lasted(started: u64, now: u64) -> String {
    yapfast_core::clock::lasted(started, now / 60 * 60)
}

/// A channel's icon as the list shows it: a lock for one this account may
/// not read, else its kind's, marked as Discord's client marks it, with a
/// warning sign before a lock.
fn icon(room: &ChatRoom) -> Icon {
    if room.locked {
        return Icon::Lock;
    }
    if room.parent.is_some() {
        return Icon::Thread;
    }
    // Plain, private and age-restricted.
    let (plain, private, restricted) = match room.mark.kind {
        MarkKind::Rules => return Icon::Rules,
        MarkKind::Stage => (Icon::Stage, Icon::StageLock, Icon::Stage),
        _ if room.voice => (Icon::Volume, Icon::VolumeLock, Icon::VolumeWarning),
        MarkKind::Media => (Icon::Image, Icon::ImageLock, Icon::ImageWarning),
        _ if room.forum => (
            Icon::MessageSquareText,
            Icon::MessageSquareTextLock,
            Icon::MessageSquareTextWarning,
        ),
        MarkKind::Announcement => (Icon::Megaphone, Icon::MegaphoneLock, Icon::MegaphoneWarning),
        MarkKind::Plain => (Icon::Hash, Icon::HashLock, Icon::HashWarning),
    };
    if room.mark.age_restricted {
        restricted
    } else if room.mark.private || (room.voice && room.join_locked) {
        // A voice channel one can see but not join has Discord's lock too.
        private
    } else {
        plain
    }
}
