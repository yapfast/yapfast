//! Samples of the activity panel and the cards for tests and shots, after
//! the fake Discord's data: task P06's. Bob plays Factorio (42 min), Carol
//! listens to Marconi Union (2 min), Dave played Celeste (3 h ago); two more
//! make five.

use crate::fixtures::avatar;
use crate::fixtures::pictures::{picture, pictured};
use crate::kind::{Doing, Expected, Status};
use crate::people::activity::card::Card;
use crate::people::activity::{Activity, Row};
use yapfast_lang::{tr, trn};

/// The icon the fake serves for Factorio and Celeste.
const GAME: &str = "/app-icons/1400000000000000900/factorioicon.png";
/// The cover the fake serves for Spotify.
const COVER: &str = "/spotify/ab67616d0000b273fake";

#[allow(clippy::too_many_arguments)]
fn row(
    id: &str,
    name: &str,
    tint: u32,
    doing: Doing,
    game: &str,
    since: String,
    live: bool,
    thumb: &str,
) -> Row {
    let mut avatar = pictured(avatar(&name[..1], tint), name);
    match name {
        "Carol" => {
            avatar.picture = Some(picture(
                "/guilds/1400000000000000100/users/1400000000000000003/avatars/carol-server.png",
            ));
        }
        "Dave" => avatar.picture = Some(picture("/embed/avatars/2.png")),
        _ => {}
    }
    Row {
        id: id.into(),
        name: name.into(),
        avatar,
        doing,
        live,
        game: game.into(),
        since,
        thumb: Expected::Ready(picture(thumb)),
    }
}

fn rows() -> Vec<Row> {
    vec![
        row(
            "1400000000000000002",
            "Bob",
            0xc2554d,
            Doing::Playing,
            "Factorio",
            tr!("for {} min", 42),
            true,
            GAME,
        ),
        row(
            "1400000000000000003",
            "Carol",
            0x3a9357,
            Doing::Listening,
            "Marconi Union",
            tr!("for {} min", 2),
            true,
            COVER,
        ),
        row(
            "1400000000000000004",
            "Dave",
            0x4a5bd0,
            Doing::Playing,
            "Celeste",
            tr!("{}h ago", 3),
            false,
            GAME,
        ),
        row(
            "1400000000000000002",
            "Bob",
            0xc2554d,
            Doing::Competing,
            "Factorio",
            tr!("Most played: {}h", 13),
            false,
            GAME,
        ),
        row(
            "1400000000000000003",
            "Carol",
            0x3a9357,
            Doing::Competing,
            "Marconi Union",
            trn!("{n} track this week", "{n} tracks this week", 31),
            false,
            COVER,
        ),
    ]
}

/// The panel with five cards, three showing: the member list's.
pub fn panel() -> Activity {
    Activity {
        rows: rows(),
        ..Activity::default()
    }
}

/// All five showing, the header's chevron turned.
pub fn opened() -> Activity {
    Activity {
        open: true,
        ..panel()
    }
}

/// Only the header, which the eye button asked for.
pub fn hidden() -> Activity {
    Activity {
        hidden: true,
        ..panel()
    }
}

/// Two cards, which have nothing to open.
pub fn two() -> Activity {
    Activity {
        rows: rows().into_iter().take(2).collect(),
        ..Activity::default()
    }
}

/// No cards: the panel is not there.
pub fn none() -> Activity {
    Activity::default()
}

fn named(text: &str) -> Status {
    Status {
        text: text.into(),
        ..Status::default()
    }
}

/// The clock the cards count from: a minute into the games and songs.
pub const NOW: i64 = 700_000_000;

/// A game with its pictures and how long it has run.
pub fn game() -> Card {
    Card {
        heading: tr!("Playing").to_uppercase(),
        name: named("Factorio"),
        details: "Building the factory".into(),
        state: "Solo".into(),
        large: Expected::Ready(picture(GAME)),
        large_text: "Factorio".into(),
        small: Expected::Ready(picture("/app-icons/steam.png")),
        small_text: "Steam".into(),
        start: NOW - 42 * 60 - 7,
        game: true,
        ..Card::default()
    }
}

/// A song from Spotify: its page, and how far along it is.
pub fn song() -> Card {
    Card {
        heading: tr!("Listening to {}", "Spotify").to_uppercase(),
        name: named("Weightless"),
        details: tr!("by {}", "Marconi Union"),
        state: tr!("on {}", "Distance"),
        large: Expected::Ready(picture(COVER)),
        start: NOW - 6 * 60 - 51,
        end: NOW + 69,
        link: "https://open.spotify.com/track/weightless".into(),
        ..Card::default()
    }
}

/// A stream with its button; no pictures, so nothing is kept for them.
pub fn stream() -> Card {
    Card {
        heading: tr!("Streaming").to_uppercase(),
        name: named("Speedrun night"),
        details: "Celeste".into(),
        url: "https://twitch.tv/anyone".into(),
        start: NOW - 3723,
        ..Card::default()
    }
}

/// A Hang status's words with the emoji's room kept, and nothing counting.
pub fn hanging() -> Card {
    Card {
        heading: tr!("Hanging out").to_uppercase(),
        name: Status {
            text: "Chilling".into(),
            emoji: Expected::Waiting,
            doing: Doing::Nothing,
        },
        ..Card::default()
    }
}
