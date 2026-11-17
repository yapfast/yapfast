//! Samples of the composer's suggestions for tests and shots, after the fake
//! Discord's data: task C15's. Bob and Status Bot for "hello @Bo" in
//! `composer/reply_attachment_mention`; the emoji whose name starts "par"
//! for `composer/emoji_autocomplete` (Server's own :party: first, then
//! Unicode's); and a longer list with roles, a group and channels for the
//! tests.

use iced::Color;
use yapfast_lang::tr;

use crate::Vm;
use crate::chat::emoji_picker::Emoji;
use crate::chat::suggestions::{Cell, Mark, Mention, Offer, Suggestions};
use crate::kind::Expected;

use super::super::{avatar, pictures};
use super::composer as fixture;

/// Bob's role colour, the blue of the fake's Mods role.
const MODS: Color = Color::from_rgb(
    0x20 as f32 / 255.0,
    0x9c as f32 / 255.0,
    0xe8 as f32 / 255.0,
);
const PARTY: &str = "1400000000000000301";

fn person(name: &str, tint: u32, color: Option<Color>, detail: &str, heading: &str) -> Mention {
    let mut face = pictures::pictured(avatar(&name[..1], tint), name);
    // No avatar in the fake: Discord's default for his id.
    if name == "Status Bot" {
        face.picture = Some(pictures::picture("/embed/avatars/2.png"));
    }
    Mention {
        name: name.into(),
        mark: Mark::Person(face),
        color,
        detail: detail.into(),
        heading: heading.into(),
    }
}

/// "hello @Bo": Bob in his role's colour with his username, then the app.
pub fn members() -> Suggestions {
    Suggestions {
        offer: Offer::Mentions {
            rows: vec![
                person("Bob", 0xc06080, Some(MODS), "bob", tr!("MEMBERS")),
                person("Status Bot", 0x5865f2, None, "", ""),
            ],
            index: 0,
        },
    }
}

/// `@`: members, then roles under their heading, @everyone, and a second row
/// highlighted.
pub fn everything() -> Suggestions {
    let group =
        |name: &str, sigil: &str, color: Option<Color>, detail: &str, heading: &str| Mention {
            name: name.into(),
            mark: Mark::Group(sigil.into()),
            color,
            detail: detail.into(),
            heading: heading.into(),
        };
    Suggestions {
        offer: Offer::Mentions {
            rows: vec![
                person("Alice", 0x8060c0, None, "alice", tr!("MEMBERS")),
                person("Bob", 0xc06080, Some(MODS), "bob", ""),
                group("Mods", "@", Some(MODS), "", tr!("ROLES")),
                group("everyone", "@", None, tr!("Everyone in this channel"), ""),
            ],
            index: 1,
        },
    }
}

/// `#`: a text channel and a voice one.
pub fn channels() -> Suggestions {
    Suggestions {
        offer: Offer::Mentions {
            rows: vec![
                Mention {
                    name: "general".into(),
                    mark: Mark::Group("#".into()),
                    color: None,
                    detail: String::new(),
                    heading: tr!("CHANNELS").into(),
                },
                Mention {
                    name: "Lounge".into(),
                    mark: Mark::Voice,
                    color: None,
                    detail: String::new(),
                    heading: String::new(),
                },
            ],
            index: 0,
        },
    }
}

/// `:par`: the nine the app's `emoji::suggestions` ranks first, Server's
/// :party: leading, and Discord's own after.
pub fn emoji() -> Suggestions {
    let cell = |glyph: &str, name: &str| Cell {
        emoji: Emoji::unicode(glyph).expect("a Unicode emoji"),
        name: format!(":{name}:"),
    };
    let mut cells = vec![Cell {
        emoji: Emoji::custom(
            "party",
            "1400000000000000050",
            Expected::Ready(pictures::badge(PARTY, 48)),
            true,
        ),
        name: ":party:".into(),
    }];
    cells.extend([
        cell("\u{1f973}", "partying_face"),
        cell("\u{1f99c}", "parrot"),
        cell("\u{1fa82}", "parachute"),
        cell("\u{26c5}", "partly_sunny"),
        cell("\u{26f1}\u{fe0f}", "parasol_on_ground"),
        cell("\u{303d}\u{fe0f}", "part_alternation_mark"),
        cell("\u{1f17f}\u{fe0f}", "parking"),
        cell("\u{1f1f5}\u{1f1fe}", "paraguay"),
        cell("\u{1f496}", "sparkling_heart"),
    ]);
    Suggestions {
        offer: Offer::Emoji {
            query: "par".into(),
            cells,
            index: 0,
        },
    }
}

/// `composer/reply_attachment_mention.png`: the reply and its file with
/// the members over them (C14's shot of that name).
pub fn mention() -> Vm {
    fixture::reply_attachment_mention()
}

/// `composer/emoji_autocomplete.png`: the same, typing ":par".
pub fn emoji_autocomplete() -> Vm {
    let mut vm = fixture::reply_attachment_mention();
    vm.chat.composer.draft = ":par".into();
    vm.chat.composer.suggestions = emoji();
    vm
}
