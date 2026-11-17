//! Samples of the Requests screen for tests and shots, after the fake
//! Discord's data: task P02's. Erin asks to be friends and wrote the first
//! message (`friends/requests`, the spam folded); the rest show every line
//! a row can take.

use yapfast_lang::tr;

use crate::fixtures::avatar;
use crate::fixtures::chat::timeline::day;
use crate::fixtures::pictures::pictured;
use crate::people::requests::{Friend, Message, Request, Requests};

fn person(id: &str, name: &str, tint: u32, friend: Friend) -> Request {
    Request {
        id: id.into(),
        name: name.into(),
        avatar: pictured(avatar(&name[..1], tint), name),
        friend,
        message: None,
    }
}

fn wrote(mut request: Request, room: &str, text: &str, time: &str, unread: bool) -> Request {
    request.message = Some(Message {
        room: room.into(),
        text: crate::text::plain(text),
        time: time.into(),
        unread,
    });
    request
}

/// Erin: a friend request with her message, unread.
pub fn erin() -> Request {
    wrote(
        person("1400000000000000005", "Erin", 0x3a9357, Friend::Asked),
        "discord:erin",
        "Hi Alice, Bob gave me your name. Can I ask about the trip?",
        &day(2, tr!("October"), 2026),
        true,
    )
}

/// What Discord took for spam: a friend request with a come-on.
pub fn spammer() -> Request {
    wrote(
        person("1400000000000000009", "Promo Bot", 0xc2554d, Friend::Asked),
        "discord:promo",
        "Free nitro, click here",
        &day(1, tr!("October"), 2026),
        false,
    )
}

/// The screen as the golden shows it (`friends/requests`): one request, one
/// spam folded.
pub fn waiting() -> Requests {
    Requests {
        incoming: vec![erin()],
        spam: vec![spammer()],
        ..Requests::default()
    }
}

/// The same with the spam unfolded.
pub fn spam_open() -> Requests {
    Requests {
        spam_open: true,
        ..waiting()
    }
}

/// Every kind of row: a friend request alone, one with a message read, a
/// message alone (read, and unread with nothing known of it) and one this
/// account sent.
pub fn everything() -> Requests {
    let sent = person("1400000000000000007", "Dave", 0x7a6bd1, Friend::Sent);
    let asked = person("1400000000000000008", "Frank", 0x3a9357, Friend::Asked);
    let stranger = wrote(
        person("1400000000000000010", "Zoe", 0xc2554d, Friend::No),
        "discord:zoe",
        "Is this the Factorio server?",
        "10:42",
        false,
    );
    let unknown = wrote(
        person("1400000000000000011", "Quinn", 0x3a9357, Friend::No),
        "discord:quinn",
        "",
        "",
        true,
    );
    Requests {
        incoming: vec![erin(), asked, stranger, unknown],
        sent: vec![sent],
        marks: true,
        ..waiting()
    }
}

/// Nothing waits.
pub fn none() -> Requests {
    Requests::default()
}
