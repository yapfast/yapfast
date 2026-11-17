//! Samples of text as spans, after the fake Discord's messages
//! (`testing/fake_discord.ts`): its history (`sample`), the welcome, the
//! showcase (`postShowcase`: spoilers, custom emoji, emoji alone) and the
//! embeds (`postEmbeds`: a heading and code in a description, a quote in a
//! field). Parsed as the app parses a message, so tests and shots see what
//! the paragraph will. Task F08's.

use std::sync::OnceLock;

use iced::Color;
use iced::widget::image;
use yapfast_lang::tr;

use crate::text::{
    self, Builder, CustomEmoji, Ink, Kind, Link, Mention, Section, Source, Spoilers, Style, Text,
};

/// The fake's `EMOJI_TALK` and `EMOJI_PARTY`.
pub const TALK: &str = "1400000000000000300";
pub const PARTY: &str = "1400000000000000301";

/// The `index`th message of the fake's history: four kinds in turn.
pub fn history(index: usize) -> Text {
    let markdown = match index % 4 {
        0 => format!("Message {index}: short one"),
        1 => format!(
            "Message {index}: a longer line about the plan for the weekend, so it wraps across the timeline at ordinary window widths"
        ),
        2 => format!("Message {index} with **bold**, `code` and a list:\n- one\n- two"),
        _ => format!("Message {index}: 👍 sounds good"),
    };
    message(&markdown, &[], &[])
}

/// The first message in #general.
pub fn welcome() -> Text {
    message(
        "Welcome to **Yapfast**, with `code` and a [link](https://example.org)",
        &[],
        &[],
    )
}

/// The showcase's spoilers, both covered.
pub fn ending() -> Text {
    message(
        "The ending: ||the butler did it||, and ||**nobody** saw it coming||.",
        &[],
        &[],
    )
}

/// The showcase's custom emoji in a line of text.
pub fn ship_it() -> Text {
    message("Ship it :talk: today :party:", &[], &emoji())
}

/// The showcase's message of emoji alone, which shows large.
pub fn jumbo() -> Text {
    message(":talk::party:", &[], &emoji())
}

/// A message that mentions Alice and Bob, in their colours.
pub fn mentions() -> Text {
    let mentions = [
        Mention {
            text: "@Alice".into(),
            target: Link::User("alice".into()),
            color: None,
            at: vec![4],
        },
        Mention {
            text: "@Bob".into(),
            target: Link::User("bob".into()),
            color: Some(MODS),
            at: vec![16],
        },
    ];
    message("Hey @Alice, ask @Bob ~~later~~ __now__", &mentions, &[])
}

/// The colour of the fake's Mods, Bob's role (3447003).
pub const MODS: Color = Color::from_rgb8(0x34, 0x98, 0xdb);

/// Bob's notice of a pin, as Discord's client words it (`notice_text` in
/// `src/ui/messages.rs`): his name in bold in his role's colour, opening his
/// card; links to the message and to the pins; then when, dimmer.
pub fn pinned() -> Text {
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    let mut builder = Builder::new();
    builder.link(
        "Bob",
        bold,
        Link::Author("bob".into()),
        Some(Ink::Own(MODS)),
    );
    let sentence = tr!(" pinned {} to this channel. See all {}.");
    builder.template(sentence, Style::default(), |builder, place| {
        let (words, link) = match place {
            0 => (tr!("a message"), Link::Jump("welcome".into())),
            _ => (tr!("pinned messages"), Link::Pins),
        };
        builder.link(words, bold, link, Some(Ink::Text));
    });
    builder.colored("  01:10", Style::default(), Ink::Muted);
    builder.finish()
}

/// The title of the fake's article (`articleEmbed`), in bold, opening it.
pub fn bridges_title() -> Text {
    let mut builder = Builder::new();
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    let link = Link::Url("https://example.com/news/bridges".into());
    builder.link(
        "City opens its first bridge for bikes only",
        bold,
        link,
        None,
    );
    builder.finish()
}

/// The description of the fake's article, as an embed's Markdown shows.
pub fn bridges() -> Text {
    text::markdown(Source::new(
        "The 300-metre crossing links the old town to the station, and **cars** are kept off it for good.",
    ))
}

/// Bob's link, which a preview follows.
pub fn article() -> Text {
    message(
        "Have you seen this? https://example.com/news/bridges",
        &[],
        &[],
    )
}

/// The status bot's description: a heading, text and code, as the core
/// splits them.
pub fn status() -> Text {
    let markdown = "## All systems normal\nEverything is running. Ask @Bob if something looks off.\n```\nweb  ok\napi  ok\n```";
    let sections = [
        section(markdown, Kind::Heading(2), false, "All systems normal"),
        section(
            markdown,
            Kind::Text,
            false,
            "Everything is running. Ask @Bob if something looks off.",
        ),
        section(markdown, Kind::Code, false, "web  ok\napi  ok"),
    ];
    text::markdown(Source {
        sections: Some(&sections),
        ..Source::new(markdown)
    })
}

/// The status bot's Notes field: a quote with a link.
pub fn notes() -> Text {
    let markdown = "> Maintenance on Sunday, see https://status.example.com/maintenance";
    let sections = [section(
        markdown,
        Kind::Text,
        true,
        "Maintenance on Sunday, see https://status.example.com/maintenance",
    )];
    text::markdown(Source {
        sections: Some(&sections),
        ..Source::new(markdown)
    })
}

/// A long message, `lines` lines of text, for folding.
pub fn long(lines: usize) -> Text {
    let markdown: Vec<String> = (1..=lines).map(|line| format!("Line {line}")).collect();
    message(&markdown.join("\n"), &[], &[])
}

/// Every message's sample, for a test that draws them all.
pub fn messages() -> Vec<Text> {
    let mut all: Vec<Text> = (51..=55).map(history).collect();
    all.extend([
        welcome(),
        ending(),
        ship_it(),
        jumbo(),
        mentions(),
        article(),
        pinned(),
    ]);
    all
}

/// Every embed's sample: the article's title and description, the status
/// bot's description and its Notes field.
pub fn embeds() -> Vec<Text> {
    vec![bridges_title(), bridges(), status(), notes()]
}

/// A message's Markdown as the app parses it: emoji open their cards and
/// spoilers are covered.
fn message(markdown: &str, mentions: &[Mention], emoji: &[CustomEmoji]) -> Text {
    let covered = |_| false;
    text::markdown(Source {
        markdown,
        mentions,
        emoji,
        spoilers: Spoilers::Covered(&covered),
        sections: None,
        cards: true,
    })
}

/// The section of `kind` whose text is `text` in `markdown`.
fn section(markdown: &str, kind: Kind, quoted: bool, text: &str) -> Section {
    let start = markdown.find(text).unwrap_or_default();
    Section {
        kind,
        quoted,
        runs: std::iter::once(start..start + text.len()).collect(),
    }
}

/// The showcase's two emoji, their pictures arrived.
pub fn emoji() -> [CustomEmoji; 2] {
    [
        CustomEmoji {
            text: ":talk:".into(),
            id: TALK.into(),
            still: Some(badge(TALK)),
        },
        CustomEmoji {
            text: ":party:".into(),
            id: PARTY.into(),
            still: Some(badge(PARTY)),
        },
    ]
}

/// A custom emoji's picture as the fake draws it (`badge`): a ring of a
/// colour picked from its id, darker at its edge, white at its middle,
/// clear around it; made once for each id.
pub fn badge(id: &str) -> image::Handle {
    static TALK_BADGE: OnceLock<image::Handle> = OnceLock::new();
    static PARTY_BADGE: OnceLock<image::Handle> = OnceLock::new();
    let draw = || draw_badge(id, 48);
    match id {
        TALK => TALK_BADGE.get_or_init(draw).clone(),
        PARTY => PARTY_BADGE.get_or_init(draw).clone(),
        _ => draw(),
    }
}

fn draw_badge(id: &str, size: u32) -> image::Handle {
    let hue = crc32(id.as_bytes());
    let color = [(hue >> 16) as u8, (hue >> 8) as u8, hue as u8].map(|c| 96 + (c % 160));
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    let half = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let distance = (x as f32 + 0.5 - half).hypot(y as f32 + 0.5 - half) / half;
            let pixel = if distance > 1.0 {
                [0, 0, 0, 0]
            } else if distance > 0.8 {
                [color[0] >> 1, color[1] >> 1, color[2] >> 1, 255]
            } else if distance < 0.35 {
                [255; 4]
            } else {
                [color[0], color[1], color[2], 255]
            };
            pixels.extend_from_slice(&pixel);
        }
    }
    image::Handle::from_rgba(size, size, pixels)
}

/// CRC-32 (IEEE), as the fake picks a colour with.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
