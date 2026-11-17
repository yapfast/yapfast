//! Samples of the emoji picker for tests and shots, after the fake Discord's
//! account (`testing/fake_discord.ts --servers 3`): Alice's favourites,
//! :talk: and the skull and crossbones; what she reacts with and writes
//! most; Server's :party: and :talk:, hers to use; the fifty of Filler 2
//! and Filler 3, which she may not use without Nitro; then Unicode's
//! categories. Task C17's. Its golden shots are `picker/reaction_emoji`
//! (Add Reaction from a message's menu) and `picker/composer_emoji`.
//!
//! The animated emoji (:party:, every seventh filler) play the fake's
//! bouncing GIF, which only the widgets can make: the samples take their
//! picture from a [`Moving`], and the shots hand in that GIF.

use iced::widget::image;
use yapfast_lang::tr;

use crate::chat::emoji_picker::{
    Emoji, EmojiPicker, Hovered, POPUP_WIDTH, Purpose, Section, SectionIcon, categories, columns,
    found,
};
use crate::kind::Expected;

use super::super::pictures;

const EMOJI_TALK: &str = "1400000000000000300";
const EMOJI_PARTY: &str = "1400000000000000301";
const SERVER: &str = "1400000000000000100";
/// The fillers' ids start here, a thousand apart (`FILLER_START`).
const FILLER_START: u64 = 1_500_000_000_000_000_000;
/// Each filler's emoji (`FILLER_EMOJI`).
const FILLER_EMOJI: u64 = 50;
/// The side the app asks a custom emoji's picture at.
const SIDE: u32 = 96;

/// An animated emoji's picture, by its id.
pub type Moving = fn(&str) -> image::Handle;

/// The fake's still for an emoji: a ring of the colour of its id.
pub fn still(id: &str) -> image::Handle {
    pictures::badge(id, SIDE)
}

fn custom(name: &str, id: &str, animated: bool, usable: bool, moving: Moving) -> Emoji {
    let picture = if animated { moving(id) } else { still(id) };
    Emoji::custom(name, id, Expected::Ready(picture), usable)
}

fn unicode(glyphs: &[&str]) -> Vec<Emoji> {
    glyphs
        .iter()
        .filter_map(|glyph| Emoji::unicode(glyph))
        .collect()
}

/// Windows 11 and Twemoji draw Unicode 15.0 (`NEWEST`, `src/emoji.rs:90`).
fn drawn(emoji: &emojis::Emoji) -> bool {
    emoji.unicode_version() <= emojis::UnicodeVersion::new(15, 0)
}

/// A filler server's emoji, `emoji_0` to `emoji_49` in the order of their
/// names, as the core sorts them; every seventh moves.
fn filler(index: u64, moving: Moving) -> Section {
    let server = FILLER_START + index * 1000;
    let mut numbers: Vec<u64> = (0..FILLER_EMOJI).collect();
    numbers.sort_by_key(|n| format!("emoji_{n}"));
    let emoji: Vec<Emoji> = numbers
        .into_iter()
        .map(|n| {
            let id = (server + 600 + n).to_string();
            custom(&format!("emoji_{n}"), &id, n % 7 == 0, false, moving)
        })
        .collect();
    // Without an icon of its own, its first emoji's picture shows it.
    let picture = emoji[0].picture().cloned().unwrap_or_default();
    Section {
        name: format!("Filler {}", index + 2),
        icon: SectionIcon::Server {
            id: server.to_string(),
            picture,
        },
        emoji,
    }
}

/// Every section, with `frequent` as what she uses most.
fn sections(frequent: &[&str], moving: Moving) -> Vec<Section> {
    let mut sections = vec![
        Section {
            name: tr!("Favorites").into(),
            icon: SectionIcon::Favorites,
            emoji: [custom("talk", EMOJI_TALK, false, true, moving)]
                .into_iter()
                .chain(unicode(&["☠️"]))
                .collect(),
        },
        Section {
            name: tr!("Frequently Used").into(),
            icon: SectionIcon::Frequent,
            emoji: unicode(frequent),
        },
        Section {
            name: "Server".into(),
            icon: SectionIcon::Server {
                id: SERVER.into(),
                picture: pictures::server("Server").map_or(Expected::Waiting, Expected::Ready),
            },
            emoji: vec![
                custom("party", EMOJI_PARTY, true, true, moving),
                custom("talk", EMOJI_TALK, false, true, moving),
            ],
        },
        filler(0, moving),
        filler(1, moving),
    ];
    sections.extend(categories(
        |english| yapfast_lang::text(english).into(),
        drawn,
    ));
    sections
}

/// Add Reaction from a message's menu, its animated emoji from `moving`:
/// what she reacts with on Discord (fire, :100:), then those this device
/// reacts with most.
pub fn reaction_with(moving: Moving) -> EmojiPicker {
    EmojiPicker {
        purpose: Purpose::Reaction,
        sections: sections(&["🔥", "💯", "👍", "❤️", "😂", "🎉"], moving),
        favorites: vec![format!("talk:{EMOJI_TALK}"), "☠️".into()],
        columns: columns(POPUP_WIDTH),
        ..EmojiPicker::default()
    }
}

pub fn reaction() -> EmojiPicker {
    reaction_with(still)
}

/// The composer's emoji button: what she writes with most on Discord (the
/// crying-laughing face, a medium-dark thumbs up), then this device's.
pub fn composer_with(moving: Moving) -> EmojiPicker {
    EmojiPicker {
        purpose: Purpose::Composer,
        sections: sections(&["😂", "👍🏾", "👍", "❤️", "🎉"], moving),
        ..reaction_with(moving)
    }
}

pub fn composer() -> EmojiPicker {
    composer_with(still)
}

/// The custom status's emoji.
pub fn status() -> EmojiPicker {
    EmojiPicker {
        purpose: Purpose::Status,
        ..reaction()
    }
}

/// A search for `query`, with its matches.
pub fn searched(query: &str) -> EmojiPicker {
    let mut picker = reaction();
    picker.query = query.into();
    picker.found = found(&picker.sections, query, drawn);
    picker
}

/// A search with no match.
pub fn nothing() -> EmojiPicker {
    searched("zzzz")
}

/// The pointer over the thumbs up, which the foot names.
pub fn hovered() -> EmojiPicker {
    EmojiPicker {
        hovered: Emoji::unicode("👍").map(Hovered::Emoji),
        ..reaction()
    }
}

/// Just opened, the custom emoji's pictures and the fillers' icons on their
/// way, the pointer over Server's button in the rail.
pub fn waiting() -> EmojiPicker {
    let mut picker = reaction();
    for section in &mut picker.sections {
        if let SectionIcon::Server { id, picture } = &mut section.icon
            && id != SERVER
        {
            *picture = Expected::Waiting;
        }
        for emoji in &mut section.emoji {
            if let Emoji::Custom(custom) = emoji {
                custom.picture = Expected::Waiting;
            }
        }
    }
    picker.hovered = Some(Hovered::Section(2));
    picker
}
