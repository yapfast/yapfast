//! Samples of a server's Stickers settings for tests and shots, after the
//! fake Discord's data: task X11's.

use crate::Vm;
use crate::fixtures::pictures::{badge, sunset};
use crate::fixtures::{settings::server, shell};
use crate::kind::Expected;
use crate::settings::server::Section;
use crate::settings::stickers::{Card, Form, Stickers, Tile};
use crate::shell::Layer;

/// The fake's stickers (`STICKER_WAVE`, `STICKER_BOUNCE`, `STICKER_LOTTIE`).
pub const WAVE: &str = "1400000000000000310";
/// Its animated one: a shot draws it moving.
pub const BOUNCE: &str = "1400000000000000311";
pub const LOTTIE: &str = "1400000000000000312";

/// Bob's sticker; a Lottie one can't be drawn here.
fn tile(id: &str, name: &str) -> Tile {
    Tile {
        id: id.into(),
        name: name.into(),
        picture: if id == LOTTIE {
            Expected::None
        } else {
            Expected::Ready(badge(id, 80))
        },
        uploader: "Bob".into(),
        may_change: true,
    }
}

/// A boost level's card, locked: the boosts it takes.
fn level(level: u32, boosts: u32) -> Card {
    Card {
        title: format!("Level {level}"),
        status: format!("{boosts} Boosts"),
        ..Card::default()
    }
}

/// Discord's four cards as the fake's server has them: Free Slots with its
/// three stickers and the slot to fill, then the three locked levels.
pub fn stickers() -> Stickers {
    Stickers {
        cards: vec![
            Card {
                title: "Free Slots".into(),
                status: "2 of 5 slots available".into(),
                unlocked: true,
                next: true,
                tiles: vec![
                    tile(WAVE, "Wave"),
                    tile(BOUNCE, "Bounce"),
                    tile(LOTTIE, "Lottie"),
                ],
            },
            level(1, 2),
            level(2, 7),
            level(3, 14),
        ],
        may_upload: true,
        ..Stickers::default()
    }
}

/// The form on a file, filled in.
pub fn form() -> Form {
    Form {
        preview: Expected::Ready(sunset(160, 160)),
        has_file: true,
        file_name: "sunset.png".into(),
        tags: "\u{1f431}".into(),
        name: "sunset".into(),
        may_save: true,
        ..Form::default()
    }
}

/// The window with the server's settings open on the Stickers.
pub fn page() -> Vm {
    let mut vm = shell::open(shell::server_home(), Layer::ServerSettings);
    vm.settings.server = server::server();
    vm.settings.server.section = Section::Stickers;
    vm.settings.stickers = stickers();
    vm
}
