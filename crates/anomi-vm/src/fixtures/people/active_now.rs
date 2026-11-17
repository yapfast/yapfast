//! Samples of the Active Now column for tests and shots, after the fake
//! Discord's data: task P04's. [`around`] is the golden screenshot's:
//! Bob and Carol in the Lounge, their Spotify on the card's head, and
//! Marconi Union's "Weightless" running for a minute and a half.

use iced::Color;
use yapfast_lang::{tr, trn};

use crate::Vm;
use crate::fixtures::pictures::{picture, pictured, server};
use crate::fixtures::{avatar, conversation, header, shell};
use crate::kind::{Avatar, Expected};
use crate::people::active_now::{ActiveNow, Card, Kind, Row};

/// The clock the samples count from, in seconds.
pub const NOW: i64 = 1_800_000_000;

fn person(name: &str, tint: u32) -> Avatar {
    pictured(avatar(&name[..1], tint), name)
}

/// A game's or an app's picture, as the fake serves one.
fn art(name: &str) -> iced::widget::image::Handle {
    picture(&format!("/app-assets/{name}.png"))
}

/// What Spotify's Rich Presence shows, at the fake's path for it.
fn spotify() -> iced::widget::image::Handle {
    picture("/spotify/ab67616d0000b273fake")
}

fn tint(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The Lounge's row: the server, the channel, three of those in it.
fn lounge() -> Row {
    Row {
        kind: Kind::Voice,
        title: "Server".into(),
        detail: "Lounge".into(),
        icon: Avatar {
            initial: "S".into(),
            tint: tint(0xc2554d),
            picture: server("Server"),
        },
        faces: vec![
            person("Bob", 0xc2554d),
            Avatar {
                picture: Some(picture(
                    "/guilds/1400000000000000100/users/1400000000000000003/avatars/carol-server.png",
                )),
                ..person("Carol", 0x3a9357)
            },
            Avatar {
                picture: Some(picture("/embed/avatars/2.png")),
                ..person("Dave", 0x5865f2)
            },
        ],
        room: "discord:v1".into(),
        ..Row::default()
    }
}

/// What Bob and Carol listen to: Rich Presence's details and state, and how
/// long since it started.
fn weightless() -> Row {
    Row {
        kind: Kind::Detail,
        title: "Weightless".into(),
        detail: "Marconi Union".into(),
        icon: Avatar {
            initial: "W".into(),
            tint: tint(0xe0475f),
            picture: Some(spotify()),
        },
        start: NOW - 96,
        ..Row::default()
    }
}

/// Bob and Carol in the Lounge, listening to Spotify.
pub fn lounge_card() -> Card {
    Card {
        id: "discord:v1".into(),
        title: tr!("{} and {}", "Bob", "Carol"),
        doing: tr!("Listening to {}", "Spotify"),
        start: NOW - 60,
        face: person("Bob", 0xc2554d),
        icon: Expected::Ready(spotify()),
        rows: vec![lounge(), weightless()],
        person: "1400000000000000002".into(),
    }
}

/// A game several friends play: the game's row says how many, with them.
pub fn party_card() -> Card {
    Card {
        id: "person:1".into(),
        title: trn!(
            "{}, {}, and {n} other",
            "{}, {}, and {n} others",
            1,
            "Ann",
            "Bob"
        ),
        doing: "Dota 2".into(),
        start: NOW - 3 * 3600 - 120,
        face: person("Ann", 0x9a5c6a),
        icon: Expected::Ready(art("dota")),
        rows: vec![Row {
            kind: Kind::Game,
            title: "Dota 2".into(),
            detail: trn!("{n} Person", "{n} People", 3),
            icon: Avatar {
                initial: "D".into(),
                tint: tint(0x3a6f9a),
                picture: Some(art("dota")),
            },
            faces: vec![
                person("Ann", 0x9a5c6a),
                person("Bob", 0xc2554d),
                person("Carol", 0x3a9357),
            ],
            more: 2,
            ..Row::default()
        }],
        person: "1".into(),
    }
}

/// One friend alone at a game that has no picture yet, an hour and a day
/// in, with no rows of its own.
pub fn solo_card() -> Card {
    Card {
        id: "person:2".into(),
        title: "Dave".into(),
        doing: "Solitaire".into(),
        start: NOW - 2 * 86400,
        face: avatar("D", 0x7a6fb0),
        icon: Expected::Waiting,
        person: "2".into(),
        ..Card::default()
    }
}

/// The golden screenshot's column: the Lounge card alone.
pub fn around() -> ActiveNow {
    ActiveNow {
        cards: vec![lounge_card()],
        now: NOW,
    }
}

/// A column of every kind of card.
pub fn busy() -> ActiveNow {
    ActiveNow {
        cards: vec![lounge_card(), party_card(), solo_card()],
        now: NOW,
    }
}

/// Nobody doing anything: the quiet message.
pub fn quiet() -> ActiveNow {
    ActiveNow {
        now: NOW,
        ..ActiveNow::default()
    }
}

/// The friends screen with this column beside it, as the golden
/// `friends/online_tab_active_now` shows it.
pub fn window(active_now: ActiveNow) -> Vm {
    let mut vm = shell::dm_home();
    vm.header = header::friends();
    vm.conversation = conversation::friends();
    vm.people.friends = super::friends::online();
    vm.people.active_now = active_now;
    if let crate::sidebar::Place::Dms(dms) = &mut vm.sidebar.place {
        dms.friends_selected = true;
        for row in &mut dms.rows {
            row.selected = false;
        }
    }
    vm
}
