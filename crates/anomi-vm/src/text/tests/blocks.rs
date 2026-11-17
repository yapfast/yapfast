//! Blocks as the core splits them, spaced as `blocks_of` spaced them, with
//! mentions at their offsets and spoilers counted through the message.

use iced::Color;

use super::super::prepare::{Marker, Markers};
use super::super::source::{Kind, Mention, Section, Spoilers};
use super::*;

/// The section of `kind` whose text is `text` in `markdown`.
fn section(markdown: &str, kind: Kind, quoted: bool, text: &str) -> Section {
    let start = markdown.find(text).expect("the text is in the Markdown");
    Section {
        kind,
        quoted,
        runs: std::iter::once(start..start + text.len()).collect(),
    }
}

#[test]
fn headings_code_and_quotes_are_blocks_spaced_as_discord_spaces_them() {
    let markdown = "# Title\nsome text\n```\nweb  ok\napi  ok\n```\n> quoted **line**\nafter";
    let sections = [
        section(markdown, Kind::Heading(1), false, "Title"),
        section(markdown, Kind::Text, false, "some text"),
        section(markdown, Kind::Code, false, "web  ok\napi  ok"),
        section(markdown, Kind::Text, true, "quoted **line**"),
        section(markdown, Kind::Text, false, "after"),
    ];
    let text = super::super::markdown(Source {
        sections: Some(&sections),
        ..Source::new(markdown)
    });
    assert_eq!(
        shown(&text),
        "H1|**Title**\nT|some text\nC|web  ok\napi  ok\nT>|quoted **line**\nT|after"
    );
    let spacing: Vec<(f32, f32, bool, bool)> = text
        .blocks()
        .map(|(block, _)| {
            (
                block.margin,
                block.padding,
                block.quote_start,
                block.quote_end,
            )
        })
        .collect();
    assert_eq!(
        spacing,
        [
            (8.0, 0.0, false, false),
            (8.0, 0.0, false, false),
            (6.0, 0.0, false, false),
            (4.0, 0.0, true, true),
            (4.0, 0.0, false, false),
        ]
    );
}

#[test]
fn a_quotes_blocks_share_its_bar() {
    let markdown = "> ## Inside\n> text";
    let sections = [
        section(markdown, Kind::Heading(2), true, "Inside"),
        section(markdown, Kind::Text, true, "text"),
    ];
    let text = super::super::markdown(Source {
        sections: Some(&sections),
        ..Source::new(markdown)
    });
    let quotes: Vec<(bool, bool, f32, f32)> = text
        .blocks()
        .map(|(block, _)| {
            (
                block.quote_start,
                block.quote_end,
                block.margin,
                block.padding,
            )
        })
        .collect();
    assert_eq!(quotes, [(true, false, 4.0, 8.0), (false, true, 0.0, 8.0)]);
}

#[test]
fn mentions_show_where_their_tags_were_in_their_colour() {
    let red = Color::from_rgb8(0xe7, 0x4c, 0x3c);
    let mentions = [
        Mention {
            text: "@Bob".into(),
            target: Link::User("2".into()),
            color: Some(red),
            at: vec![4],
        },
        Mention {
            text: "#general".into(),
            target: Link::Channel("c1".into()),
            color: None,
            at: vec![23],
        },
    ];
    let markdown = "Ask @Bob, not @Bob, in #general";
    let text = super::super::markdown(Source {
        mentions: &mentions,
        cards: true,
        ..Source::new(markdown)
    });
    // The second @Bob was typed as text, not tagged.
    assert_eq!(shown(&text), "Ask @[@Bob], not @Bob, in #[#general]");
    let (_, spans) = text.blocks().next().expect("a block");
    assert_eq!(spans[1].color, Some(Ink::Own(red)));
    assert_eq!(
        spans[1].link.and_then(|link| text.link(link)),
        Some(&Link::User("2".into()))
    );
    // In code it's text.
    let code = super::super::markdown(Source {
        mentions: &mentions[..1],
        ..Source::new("`as @Bob`")
    });
    assert_eq!(shown(&code), "`as @Bob`");
}

#[test]
fn spoilers_are_covered_until_revealed_and_count_through_the_message() {
    let markdown = "The ending: ||the butler did it||, and ||**nobody** saw it coming||.";
    let none = |_| false;
    let text = super::super::markdown(Source {
        spoilers: Spoilers::Covered(&none),
        ..Source::new(markdown)
    });
    assert_eq!(
        shown(&text),
        "The ending: ||the butler did it||, and ||nobody saw it coming||."
    );
    let links: Vec<_> = text
        .blocks()
        .flat_map(|(_, spans)| spans)
        .filter(|span| span.look == Look::Spoiler)
        .filter_map(|span| span.link.and_then(|link| text.link(link)).cloned())
        .collect();
    assert_eq!(links, [Link::Spoiler(0), Link::Spoiler(1)]);
    assert_eq!(text.read(), "The ending: (spoiler), and (spoiler).");
    let second = |index| index == 1;
    let revealed = super::super::markdown(Source {
        spoilers: Spoilers::Covered(&second),
        ..Source::new(markdown)
    });
    assert_eq!(
        shown(&revealed),
        "The ending: ||the butler did it||, and **nobody** saw it coming."
    );
    let hidden = super::super::markdown(Source {
        spoilers: Spoilers::Hidden,
        ..Source::new(markdown)
    });
    assert_eq!(shown(&hidden), "The ending: (spoiler), and (spoiler).");
    // The second block's spoiler is the message's second.
    let markdown = "||one||\n# ||two||";
    let sections = [
        section(markdown, Kind::Text, false, "||one||"),
        section(markdown, Kind::Heading(1), false, "||two||"),
    ];
    let split = super::super::markdown(Source {
        sections: Some(&sections),
        spoilers: Spoilers::Covered(&none),
        ..Source::new(markdown)
    });
    let last = split
        .blocks()
        .last()
        .and_then(|(_, spans)| spans.first().copied());
    assert_eq!(
        last.and_then(|span| span.link)
            .and_then(|link| split.link(link)),
        Some(&Link::Spoiler(1))
    );
}

/// The looks of `text`'s spans, and the links they open.
fn looks(text: &Text) -> Vec<(Look, Option<Link>)> {
    text.blocks()
        .flat_map(|(_, spans)| spans)
        .map(|span| {
            (
                span.look,
                span.link.and_then(|link| text.link(link)).cloned(),
            )
        })
        .collect()
}

#[test]
fn no_one_types_a_marker() {
    // A link written to look like the parser's own markers stays a link.
    let typed = looks(&message("[@Bob](<anomi0:m0>) ||x|| [y](anomi1:s0)"));
    assert!(!typed.iter().any(|(look, _)| *look == Look::Mention));
    // Markdown unescapes a destination, which then reads as a marker the
    // Markdown never held: escaped, or as entities.
    let alice = [Mention {
        text: "@Alice".into(),
        target: Link::User("alice".into()),
        color: None,
        at: vec![3],
    }];
    let none = |_| false;
    let markdown = "hi @Alice ||real|| [@Alice](anomi0\\:m0) [@everyone](anomi0&#58;m9) \
                    [x](anomi0\\:s0) [y](&#97;nomi0:s0) [z](<anomi0\\:m0>)";
    let text = super::super::markdown(Source {
        mentions: &alice,
        spoilers: Spoilers::Covered(&none),
        cards: true,
        ..Source::new(markdown)
    });
    let found = looks(&text);
    let count = |wanted: Look| found.iter().filter(|(look, _)| *look == wanted).count();
    // Only the tagged mention and the spoiler typed as one.
    assert_eq!((count(Look::Mention), count(Look::Spoiler)), (1, 1));
    assert!(!found.iter().any(|(_, link)| *link == Some(Link::Everyone)));
    assert!(found.contains(&(Look::Text, Some(Link::Url("anomi0:m9".into())))));
    assert!(found.contains(&(Look::Text, Some(Link::Url("anomi0:s0".into())))));
}

#[test]
fn a_destination_like_a_marker_of_another_kind_is_a_link() {
    // `anomi0\:é` parses to `anomi0:é`, a kind of more than one byte.
    let text = message("[x](anomi0\\:é) [y](anomi0\\:é1)");
    let links: Vec<Option<Link>> = looks(&text).into_iter().map(|(_, link)| link).collect();
    assert!(links.contains(&Some(Link::Url("anomi0:é".into()))));
    // Even written as a marker is.
    let markers = Markers::for_markdown("");
    assert_eq!(markers.read("anomi0:é1", "[x](<anomi0:é1>)"), None);
    assert_eq!(
        markers.read("anomi0:s1", "[x](<anomi0:s1>)"),
        Some(Marker::Spoiler(1))
    );
}
