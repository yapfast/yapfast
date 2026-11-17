//! Samples of the sticker picker for tests and shots, after the fake
//! Discord's (`testing/fake_discord.ts --servers 3`): Server's own Wave (a
//! PNG), Bounce (a GIF) and Lottie (a red square at half strength), Discord's
//! default packs Wumpus (Wumpus Wave, Wumpus Party) and Puff, which Frequently
//! Used starts from until a sticker is sent. [`locked`] adds the stickers of
//! two filler servers, which Alice may not use without Nitro (the fake's own
//! fillers have none). Task C19's. Its golden shot is
//! `picker/sticker` (the third tab of the picker above the composer).
//!
//! The GIF stickers play the fake's bouncing GIF and the Lottie ones its
//! square, which only the widgets can make: the samples take their picture
//! from a [`Moving`] (`LOTTIE` and `PUFF` are the Lottie ones), and the shots
//! hand those in.

use iced::widget::image;
use yapfast_lang::tr;

use crate::chat::sticker_picker::{
    Hovered, Section, SectionIcon, Sticker, StickerPicker, WIDTH, columns,
};
use crate::kind::Expected;

use super::super::pictures;

const SERVER: &str = "1400000000000000100";
const WAVE: &str = "1400000000000000310";
const BOUNCE: &str = "1400000000000000311";
pub const LOTTIE: &str = "1400000000000000312";
const WUMPUS: &str = "1400000000000000330";
const PARTY: &str = "1400000000000000331";
pub const PUFF: &str = "1400000000000000332";
/// The fillers' ids start here, a thousand apart.
const FILLER_START: u64 = 1_500_000_000_000_000_000;
/// The side the app asks a sticker's picture at.
const SIDE: u32 = 96;

/// An animated sticker's picture, by its id.
pub type Moving = fn(&str) -> image::Handle;

/// The fake's still for a sticker: a ring of the colour of its id.
pub fn still(id: &str) -> image::Handle {
    pictures::badge(id, SIDE)
}

/// A sticker whose picture moves when `animated`.
fn sticker(name: &str, id: &str, animated: bool, usable: bool, moving: Moving) -> Sticker {
    let picture = if animated { moving(id) } else { still(id) };
    Sticker {
        id: id.into(),
        name: name.into(),
        picture: Expected::Ready(picture),
        usable,
    }
}

/// A Lottie sticker, which the core renders: its picture comes from `moving`.
fn lottie(name: &str, id: &str, usable: bool, moving: Moving) -> Sticker {
    sticker(name, id, true, usable, moving)
}

/// A section with no icon of its own: its first sticker's picture shows it.
fn pack(id: &str, name: &str, stickers: Vec<Sticker>) -> Section {
    let first = &stickers[0];
    Section {
        id: id.into(),
        name: name.into(),
        icon: SectionIcon::Sticker {
            id: first.id.clone(),
            picture: first.picture.clone(),
        },
        stickers,
    }
}

fn frequent(moving: Moving) -> Section {
    Section {
        id: "frequent".into(),
        name: tr!("Frequently Used").into(),
        icon: SectionIcon::Frequent,
        stickers: vec![
            sticker("Wumpus Wave", WUMPUS, false, true, moving),
            sticker("Wumpus Party", PARTY, true, true, moving),
            lottie("Puff", PUFF, true, moving),
        ],
    }
}

fn server(moving: Moving) -> Section {
    Section {
        id: SERVER.into(),
        name: "Server".into(),
        icon: SectionIcon::Server {
            id: SERVER.into(),
            picture: pictures::server("Server").map_or(Expected::Waiting, Expected::Ready),
        },
        stickers: vec![
            sticker("Wave", WAVE, false, true, moving),
            sticker("Bounce", BOUNCE, true, true, moving),
            lottie("Lottie", LOTTIE, true, moving),
        ],
    }
}

/// A filler server's stickers, which Alice may not use without Nitro; the
/// second moves.
fn filler(index: u64, count: u64, moving: Moving) -> Section {
    let id = (FILLER_START + index * 1000).to_string();
    let stickers: Vec<Sticker> = (0..count)
        .map(|n| {
            let sticker_id = (FILLER_START + index * 1000 + 700 + n).to_string();
            sticker(
                &format!("sticker_{n}"),
                &sticker_id,
                n % 2 == 1,
                false,
                moving,
            )
        })
        .collect();
    // The fake's fillers have no icon: their first sticker's picture shows
    // them, as `show_stickers` does.
    let icon = SectionIcon::Sticker {
        id: stickers[0].id.clone(),
        picture: stickers[0].picture.clone(),
    };
    Section {
        id,
        name: format!("Filler {}", index + 2),
        icon,
        stickers,
    }
}

fn sections(moving: Moving) -> Vec<Section> {
    vec![
        frequent(moving),
        server(moving),
        pack(
            "1400000000000000320",
            "Wumpus",
            vec![
                sticker("Wumpus Wave", WUMPUS, false, true, moving),
                sticker("Wumpus Party", PARTY, true, true, moving),
            ],
        ),
        pack(
            "1400000000000000321",
            "Puff",
            vec![lottie("Puff", PUFF, true, moving)],
        ),
    ]
}

/// The picker as the fake's account sees it, its animated stickers from
/// `moving`.
pub fn picker_with(moving: Moving) -> StickerPicker {
    StickerPicker {
        sections: sections(moving),
        columns: columns(WIDTH),
        ..StickerPicker::default()
    }
}

pub fn picker() -> StickerPicker {
    picker_with(still)
}

/// The picker with two filler servers' stickers after the rest, which Alice
/// may not use without Nitro.
pub fn locked_with(moving: Moving) -> StickerPicker {
    let mut picker = picker_with(moving);
    picker.sections.push(filler(0, 6, moving));
    picker.sections.push(filler(1, 3, moving));
    picker
}

/// A search for `query` over the locked sample, its stickers from `moving`.
pub fn searched_locked_with(query: &str, moving: Moving) -> StickerPicker {
    searching(locked_with(moving), query)
}

pub fn locked() -> StickerPicker {
    locked_with(still)
}

/// Wave and Wumpus Party kept among the favourites: they list first, with
/// their stars filled.
pub fn favorited_with(moving: Moving) -> StickerPicker {
    let mut picker = picker_with(moving);
    let wave = sticker("Wave", WAVE, false, true, moving);
    let party = sticker("Wumpus Party", PARTY, true, true, moving);
    picker.favorites = vec![wave.id.clone(), party.id.clone()];
    picker
        .sections
        .insert(0, pack("favorites", tr!("Favorites"), vec![wave, party]));
    picker
}

pub fn favorited() -> StickerPicker {
    favorited_with(still)
}

/// A search for `query`: the sections that keep a sticker named by it,
/// holding only those, as the core lists them.
pub fn searched_with(query: &str, moving: Moving) -> StickerPicker {
    searching(picker_with(moving), query)
}

/// A search for `query` over the locked sample.
pub fn searched_locked(query: &str) -> StickerPicker {
    searching(locked(), query)
}

fn searching(mut picker: StickerPicker, query: &str) -> StickerPicker {
    let query = query.trim().to_lowercase();
    picker.sections.retain_mut(|section| {
        section
            .stickers
            .retain(|sticker| sticker.name.to_lowercase().contains(&query));
        !section.stickers.is_empty()
    });
    picker.query = query;
    picker
}

pub fn searched(query: &str) -> StickerPicker {
    searched_with(query, still)
}

/// A search with no match: the grid says so.
pub fn nothing() -> StickerPicker {
    searched("zzzz")
}

/// Discord's packs couldn't be fetched, and the account has no sticker of
/// its own.
pub fn failed() -> StickerPicker {
    StickerPicker {
        error: Some("Couldn't load the sticker packs.".into()),
        columns: columns(WIDTH),
        ..StickerPicker::default()
    }
}

/// The pointer over Bounce, which the foot names.
pub fn hovered_with(moving: Moving) -> StickerPicker {
    StickerPicker {
        hovered: Some(Hovered::Sticker(BOUNCE.into())),
        ..picker_with(moving)
    }
}

pub fn hovered() -> StickerPicker {
    hovered_with(still)
}

/// Just opened: the stickers' and the packs' pictures on their way, the
/// pointer over Server's button in the rail.
pub fn waiting() -> StickerPicker {
    let mut picker = picker();
    for section in &mut picker.sections {
        if let SectionIcon::Sticker { picture, .. } = &mut section.icon {
            *picture = Expected::Waiting;
        }
        for sticker in &mut section.stickers {
            sticker.picture = Expected::Waiting;
        }
    }
    picker.hovered = Some(Hovered::Section(1));
    picker
}
