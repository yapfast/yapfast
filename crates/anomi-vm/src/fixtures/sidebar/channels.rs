//! Samples of the channel list, after the fake Discord's "Server": the
//! "Channels" category with #general, its threads "Plans" and "Book club",
//! the forum "ideas", the voice channel "Lounge" where Bob streams and Carol
//! and Dave are muted, and "Create Room" (`testing/fake_discord.ts:2476`).
//! The golden screenshots' (`server/general_unread_divider.png`,
//! `voice/joined_sidebar_dock.png`, `thread/book_club_not_joined.png`,
//! `menu/channel_context.png`), then the states they don't show. Task N04's.

mod people;
mod window;

pub use window::*;

use yapfast_lang::tr;

use crate::icon::Icon;
use crate::sidebar::channels::{Arranging, ChannelKind, ChannelRow, Channels, Moving};

pub use people::{alice, bob, carol, dave, erin};

/// The first server's id, as the rail's sample has it.
pub const SERVER: &str = "discord:g1";
/// The rooms of the samples.
pub const GENERAL: &str = "discord:c1";
pub const PLANS: &str = "discord:c2";
pub const IDEAS: &str = "discord:c3";
pub const LOUNGE: &str = "discord:c4";
pub const CREATE_ROOM: &str = "discord:c5";
pub const BOOK_CLUB: &str = "discord:c6";
/// The "Channels" category's id, which it folds by.
pub const CATEGORY: &str = "discord:k1";

fn channel(id: &str, name: &str, kind: ChannelKind) -> ChannelRow {
    ChannelRow {
        id: id.into(),
        name: name.into(),
        kind,
        category: CATEGORY.into(),
        draggable: true,
        ..ChannelRow::default()
    }
}

/// The server as the golden window lists it: #general open, its thread
/// Plans under it, the forum unread, the Lounge busy for 47 minutes.
pub fn channels() -> Channels {
    let mut general = channel(GENERAL, "general", ChannelKind::Text);
    general.heading = "Channels".into();
    general.selected = true;
    let plans = channel(PLANS, "Plans", ChannelKind::Thread);
    let mut ideas = channel(IDEAS, "ideas", ChannelKind::Forum);
    ideas.unread = true;
    let mut lounge = channel(LOUNGE, "Lounge", ChannelKind::Voice);
    lounge.chat = true;
    lounge.lasted = tr!("{}m", 47);
    lounge.people = vec![bob(), carol(), dave()];
    lounge.summary = "3 people · 1 live".into();
    let create_room = channel(CREATE_ROOM, "Create Room", ChannelKind::Voice);
    let mut rows = vec![general, plans, ideas, lounge, create_room];
    placed(&mut rows);
    Channels {
        server: SERVER.into(),
        rows,
        ..Channels::default()
    }
}

/// Numbers what drags in a Discord server's list, as `show_sidebar` does:
/// each heading, then each channel showing outside a folded category,
/// threads and pinned rows aside.
fn placed(rows: &mut [ChannelRow]) {
    let mut places = 0;
    for row in rows.iter_mut().filter(|row| !row.pinned) {
        if !row.heading.is_empty() {
            places += 1;
            row.heading_place = places;
            row.heading_draggable = true;
            row.may_create = true;
        }
        if !row.hidden && !row.thread() && !row.collapsed {
            places += 1;
            row.place = places;
        }
    }
}

/// Alice joined the Lounge, still on #general: she leads the call's
/// participants, connected, and the call goes on 57 minutes in. The forum
/// is read by then, as `voice/joined_sidebar_dock.png` shows it.
pub fn joined() -> Channels {
    let mut channels = channels();
    channels.rows[2].unread = false;
    let lounge = &mut channels.rows[3];
    lounge.in_call = true;
    lounge.lasted = tr!("{}m", 57);
    lounge.summary = "4 people · 1 live".into();
    lounge.people.insert(0, alice());
    for person in &mut lounge.people {
        person.connected = true;
    }
    channels
}

/// In the call, Carol speaking, Erin still connecting and Alice on another
/// client too: the participants' states the golden window doesn't show.
pub fn speaking() -> Channels {
    let mut channels = joined();
    let lounge = &mut channels.rows[3];
    lounge.people[0].elsewhere = true;
    lounge.people[2].speaking = true;
    lounge.people.push(erin());
    channels
}

/// Book club, a thread of #general Alice isn't in, open: it shows under
/// its channel for now, its line running on to Plans.
pub fn book_club() -> Channels {
    let mut channels = channels();
    channels.rows[0].selected = false;
    channels.rows[2].unread = false;
    let mut club = channel(BOOK_CLUB, "Book club", ChannelKind::Thread);
    club.selected = true;
    club.thread_continues = true;
    channels.rows.insert(1, club);
    channels
}

/// The category folded: only what's open, unread or busy stays, and a
/// second category with nothing left is its heading alone.
pub fn folded() -> Channels {
    let mut channels = channels();
    channels
        .rows
        .retain(|row| row.id != PLANS && row.id != CREATE_ROOM);
    for row in &mut channels.rows {
        row.collapsed = true;
        row.place = 0;
    }
    channels.rows.push(ChannelRow {
        id: "discord:c9".into(),
        heading: "Archive".into(),
        category: "discord:k2".into(),
        collapsed: true,
        hidden: true,
        ..ChannelRow::default()
    });
    channels
}

/// Two pinned channels at the top under their heading, a line under them,
/// then the category; a muted channel, a locked one, mentions waiting.
pub fn pinned() -> Channels {
    let mut channels = channels();
    let mut rules = channel("discord:c7", "rules", ChannelKind::Text);
    rules.icon = Some(Icon::Rules);
    rules.heading = tr!("PINNED CHANNELS").into();
    rules.category = rules.heading.clone();
    rules.pinned = true;
    rules.badge = "3".into();
    rules.unread = true;
    let mut news = channel("discord:c8", "news", ChannelKind::Text);
    news.icon = Some(Icon::Megaphone);
    news.pinned = true;
    news.muted = true;
    // Alice on another client, in a call she isn't in here: dimmed, her
    // marks hidden.
    let mut elsewhere = alice();
    (elsewhere.elsewhere, elsewhere.muted) = (true, true);
    channels.rows[3].people.push(elsewhere);
    let mut staff = channel("discord:c10", "staff-only", ChannelKind::Text);
    staff.icon = Some(Icon::Lock);
    staff.locked = true;
    let mut rows = vec![rules, news];
    rows.append(&mut channels.rows);
    rows.insert(5, staff);
    for row in &mut rows {
        (row.place, row.heading_place) = (0, 0);
    }
    placed(&mut rows);
    channels.rows = rows;
    channels
}

/// The Lounge picked up and dragged: the text channels it can't go among
/// dim, and it may drop on Create Room's place and the category's.
pub fn arranging() -> Channels {
    let mut channels = channels();
    // The category, #general, ideas, the Lounge, Create Room.
    channels.arranging = Some(Box::new(Arranging {
        place: channels.rows[3].place,
        droppable: vec![true, false, false, false, true],
    }));
    channels
}

/// Carol dragged from the Lounge over Create Room, which she may move to:
/// it's outlined, and the rest dim.
pub fn moving() -> Channels {
    let mut channels = channels();
    channels.moving = Some(Box::new(Moving {
        droppable: vec![CREATE_ROOM.into()],
        over: Some(CREATE_ROOM.into()),
    }));
    channels
}

/// No channel matches the filter.
pub fn empty() -> Channels {
    Channels {
        server: SERVER.into(),
        empty: tr!("No matching conversations.").into(),
        ..Channels::default()
    }
}

/// A Community server's list with Browse Channels at its top, open.
pub fn browsing() -> Channels {
    let mut channels = channels();
    channels.rows[0].selected = false;
    channels.browse = Some(true);
    channels
}
