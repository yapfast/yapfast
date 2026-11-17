//! Samples of the timeline for tests and shots, after the fake Discord's
//! data: #general's history of Bob's numbered messages a minute apart (the
//! fake's `--extra`, its newest page), its "New" divider over message 51;
//! Person 5's DM with its one "Hi from Person 5"; Bob's DM; the Lounge's
//! empty chat. The messages are `message::Message`'s (C02's) as they come;
//! the timeline marks them. Task C01's.

use yapfast_lang::{tr, trn};

use crate::chat::message::Message;
use crate::chat::timeline::group::key;
use crate::chat::timeline::{
    Edge, Intro, Jump, Keys, Kind, May, Room, Row, Span, Timeline, Unread,
};
use crate::icon::Icon;

use crate::fixtures::avatar;
use crate::fixtures::pictures::pictured;
use crate::fixtures::sidebar::channels::{GENERAL, LOUNGE, PLANS};

/// #general's first message loaded and its last (`--extra 56`).
pub const FIRST: usize = 6;
pub const LAST: usize = 55;
/// The first message unread, the "New" divider over it.
pub const UNREAD: usize = 51;

/// The key of #general's message `index`.
pub fn message_key(index: usize) -> u64 {
    key(&format!("m{index}"))
}

/// A date as the core writes it, such as "1 October 2026".
pub fn day(date: u32, month: &str, year: u32) -> String {
    tr!("{0} {1} {2}", date, month, year)
}

fn row(id: &str, first: bool) -> Row {
    Row {
        key: key(id),
        first,
        message: Message::default(),
        ..Row::default()
    }
}

/// A server channel of the fake's first server, by the id the sidebar's
/// samples list it under.
fn channel(id: &str, name: &str, icon: Icon, intro: Option<Intro>) -> Room {
    Room {
        id: id.into(),
        name: name.into(),
        kind: Kind::Channel(icon),
        intro,
    }
}

fn general_room() -> Room {
    channel(
        GENERAL,
        "general",
        Icon::Hash,
        Some(Intro::Server("Server".into())),
    )
}

/// #general's messages `from..=to`, Bob's alone a minute apart, so one
/// group, the divider and Bob's name again over message 51.
fn bobs(from: usize, to: usize) -> Vec<Row> {
    (from..=to)
        .map(|index| Row {
            key: message_key(index),
            first: index == from || index == UNREAD,
            day: (index == from).then(|| day(1, tr!("October"), 2026)),
            new: index == UNREAD,
            message: Message::default(),
        })
        .collect()
}

/// What the bar says of #general's five unread messages.
fn unread() -> Unread {
    Unread {
        label: trn!(
            "{n} new message since {}",
            "{n} new messages since {}",
            5,
            "16:55"
        ),
        caught_up: false,
    }
}

/// #general at its newest: earlier messages to load above, the divider in
/// view (`server/general_unread_divider`).
pub fn general() -> Timeline {
    let rows = bobs(FIRST, LAST);
    Timeline {
        room: Some(general_room()),
        ready: true,
        shown: Some(Span::new(&rows, rows.len() - 12, rows.len() - 1)),
        rows,
        unread: Some(unread()),
        ..Timeline::default()
    }
}

/// #general read up to message 52, its history above the divider in view
/// (`chan/history_plain_a`, which scrolled there).
pub fn history() -> Timeline {
    let rows = bobs(FIRST, 52);
    Timeline {
        shown: Some(Span::new(&rows, rows.len() - 16, rows.len() - 1)),
        rows,
        ..general()
    }
}

/// Scrolled down past the divider while the conversation stays unread:
/// the bar over the messages says how much is new.
pub fn unread_above() -> Timeline {
    let mut timeline = general();
    let last = timeline.rows.len() - 1;
    timeline.shown = Some(Span::new(&timeline.rows, last - 2, last));
    timeline
}

/// #general's oldest messages with `earlier` above them: few enough that
/// all show, the edge at the top.
pub fn start_of_general(earlier: Edge) -> Timeline {
    let mut rows = vec![row("welcome", true), row("sunset", false)];
    rows[0].day = Some(day(1, tr!("October"), 2026));
    rows.extend((0..4).map(|index| row(&format!("m{index}"), false)));
    Timeline {
        room: Some(general_room()),
        ready: true,
        rows,
        earlier,
        ..Timeline::default()
    }
}

/// #general from its first message, with Discord's welcome to the server.
pub fn welcome_to_server() -> Timeline {
    start_of_general(Edge::Start)
}

/// Another channel from its first message: its welcome under a hash.
pub fn welcome_to_channel() -> Timeline {
    Timeline {
        room: Some(channel(
            "discord:plans",
            "plans",
            Icon::Hash,
            Some(Intro::Channel),
        )),
        ..start_of_general(Edge::Start)
    }
}

/// A window of older messages around a reply's original, which a jump
/// went to and lit: newer messages below it to load, and the bar back to
/// the newest.
pub fn older_window(later: Edge) -> Timeline {
    let rows = bobs(20, 32);
    Timeline {
        jump: Some(Jump {
            key: message_key(26),
            lit: true,
        }),
        later: Some(later),
        shown: Some(Span::new(&rows, 0, rows.len() - 1)),
        rows,
        unread: None,
        ..general()
    }
}

/// #general with the keyboard on message 53: Up and Down step from it,
/// and every key may act on it.
pub fn browsing() -> Timeline {
    Timeline {
        keys: Keys {
            active: Some(message_key(53)),
            browsing: true,
            may: May {
                menu: true,
                react: true,
                reply: true,
                edit: false,
                delete: false,
                copy: true,
                forward: true,
                mark_unread: true,
            },
            confirming: false,
        },
        ..general()
    }
}

/// Person 5's DM: its start, the day, and their one message
/// (`dm/person_chat`).
pub fn person_chat() -> Timeline {
    let mut hi = row("hi-p5", true);
    hi.day = Some(day(1, tr!("October"), 2026));
    Timeline {
        room: Some(Room {
            id: "discord:p5".into(),
            name: "Person 5".into(),
            kind: Kind::Direct(pictured(avatar("P", 0x3a9357), "Person 5")),
            intro: None,
        }),
        ready: true,
        rows: vec![hi],
        earlier: Edge::Start,
        ..Timeline::default()
    }
}

/// Bob's DM: two messages, then the "New" divider over two more of Bob's
/// (`dm/conversation_profile`).
pub fn bob() -> Timeline {
    let mut rows = vec![
        row("trip-notes", true),
        row("snacks", true),
        row("great", true),
        row("nine", false),
    ];
    rows[0].day = Some(day(2, tr!("October"), 2026));
    rows[2].new = true;
    Timeline {
        room: Some(Room {
            id: "discord:d1".into(),
            name: "Bob".into(),
            kind: Kind::Direct(pictured(avatar("B", 0x9a5c6a), "Bob")),
            intro: None,
        }),
        ready: true,
        rows,
        earlier: Edge::Start,
        unread: Some(Unread {
            label: tr!("1 new message since {}", "16:04"),
            caught_up: false,
        }),
        ..Timeline::default()
    }
}

/// A conversation opening, its history not come yet.
pub fn loading() -> Timeline {
    Timeline {
        room: Some(general_room()),
        ..Timeline::default()
    }
}

/// The Lounge's chat, with nothing written in it yet
/// (`voice/text_chat_of_voice_channel`).
pub fn empty_channel() -> Timeline {
    Timeline {
        room: Some(channel(LOUNGE, "Lounge", Icon::Volume, None)),
        ready: true,
        ..Timeline::default()
    }
}

/// A DM nobody has written in yet.
pub fn empty_dm() -> Timeline {
    Timeline {
        rows: Vec::new(),
        ..person_chat()
    }
}

/// A thread nobody has written in yet.
pub fn empty_thread() -> Timeline {
    Timeline {
        room: Some(Room {
            id: PLANS.into(),
            name: "Plans".into(),
            kind: Kind::Thread(Icon::Thread),
            intro: None,
        }),
        ready: true,
        ..Timeline::default()
    }
}

/// A long history, `count` messages of one person's, for scrolling.
pub fn long(count: usize) -> Timeline {
    let mut timeline = general();
    timeline.rows = bobs(0, count - 1);
    timeline.unread = None;
    timeline.shown = None;
    timeline
}
