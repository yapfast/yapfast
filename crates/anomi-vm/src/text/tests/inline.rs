//! Inline Markdown: styles, lists and lines, links and emoji.

use iced::widget::image;

use super::super::source::CustomEmoji;
use super::*;

#[test]
fn styles_show_as_discord_sets_them() {
    assert_eq!(
        shown(&message(
            "Message 54 with **bold**, `code` and a list:\n- one\n- two"
        )),
        "Message 54 with **bold**, `code` and a list:\n• one\n• two"
    );
    assert_eq!(
        shown(&message("*it* __under__ ~~gone~~ ***both***")),
        "*it* __under__ ~~gone~~ ***both***"
    );
}

#[test]
fn every_line_break_shows_and_blank_lines_stay() {
    assert_eq!(shown(&message("one\ntwo")), "one\ntwo");
    assert_eq!(
        shown(&message("one\n\n\nthree")),
        "one\n\u{a0}\n\u{a0}\nthree"
    );
}

#[test]
fn lists_nest_with_their_bullets_and_numbers() {
    assert_eq!(
        shown(&message("1. first\n2. second\n   - inner\n     - deeper")),
        "1. first\n2. second\n    ◦ inner\n        ▪ deeper"
    );
}

#[test]
fn rules_and_html_show_as_typed() {
    assert_eq!(shown(&message("above\n---")), "above\n---");
    assert_eq!(
        shown(&message("a <font color=red>red</font> b")),
        "a <font color=red>red</font> b"
    );
}

#[test]
fn what_slint_refused_shows_as_typed_with_its_links() {
    // A picture, which Slint's Markdown refused: the block shows as typed.
    let text = message("look ![cat](https://example.com/cat.png) **at**");
    assert_eq!(
        shown(&text),
        "look ![cat]([https://example.com/cat.png](Url(\"https://example.com/cat.png\"))) **at**"
    );
}

#[test]
fn bare_addresses_link_and_masked_links_ask_before_leaving() {
    assert_eq!(
        shown(&message(
            "see https://example.org/a. and [example.org](https://example.org/)"
        )),
        "see [https://example.org/a](Url(\"https://example.org/a\")). and [example.org](Url(\"https://example.org/\"))"
    );
    assert_eq!(
        shown(&message("[discord.com](https://evil.example)")),
        "[discord.com](Leave(\"https://evil.example\"))"
    );
    assert_eq!(
        shown(&message("`https://example.org` stays code")),
        "`https://example.org` stays code"
    );
}

#[test]
fn a_links_styles_share_its_one_link() {
    let text = message("[go **here**](https://example.org/x)");
    let (_, spans) = text.blocks().next().expect("a block");
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].link, spans[1].link);
    assert!(spans[1].style.bold);
}

#[test]
fn emoji_are_apart_and_open_their_cards() {
    let text = message("Message 55: 👍 sounds good :sob: `🙂`");
    assert_eq!(shown(&text), "Message 55: {👍} sounds good {😭} `🙂`");
    let (_, spans) = text.blocks().next().expect("a block");
    let emoji = spans
        .iter()
        .find(|span| span.look == Look::Emoji)
        .expect("an emoji");
    assert_eq!(
        emoji.link.and_then(|link| text.link(link)),
        Some(&Link::Emoji("👍".into()))
    );
    // An embed's open nothing.
    let embed = markdown(Source::new("👍"));
    let (_, spans) = embed.blocks().next().expect("a block");
    assert_eq!(spans[0].link, None);
}

#[test]
fn a_servers_emoji_is_its_picture_once_it_arrived() {
    let still = image::Handle::from_rgba(1, 1, vec![0; 4]);
    let emoji = [
        CustomEmoji {
            text: ":talk:".into(),
            id: "11".into(),
            still: Some(still.clone()),
        },
        CustomEmoji {
            text: ":party:".into(),
            id: "12".into(),
            still: None,
        },
    ];
    let text = markdown(Source {
        emoji: &emoji,
        cards: true,
        ..Source::new("Ship it :talk: today :party:")
    });
    assert_eq!(shown(&text), "Ship it {:talk:} today :party:");
    assert_eq!(text.pictures().len(), 1);
    assert_eq!(text.pictures()[0].still, still);
    assert!(!text.jumbo());
    let jumbo = markdown(Source {
        emoji: &emoji,
        cards: true,
        ..Source::new(":talk: 👍")
    });
    assert!(jumbo.jumbo());
    assert_eq!(jumbo.read(), ":talk: 👍");
}

#[test]
fn thirty_emoji_are_large_and_more_are_not() {
    assert!(message(&"👍".repeat(30)).jumbo());
    assert!(!message(&"👍".repeat(31)).jumbo());
    assert!(!message("👍 ok").jumbo());
    assert!(!message("").jumbo());
}

#[test]
fn plain_text_links_its_addresses_and_nothing_else() {
    let text = plain("**not bold** https://example.com\n\nkept");
    assert_eq!(
        shown(&text),
        "**not bold** [https://example.com](Url(\"https://example.com\"))\n\nkept"
    );
}

#[test]
fn each_emoji_is_a_span_of_its_own_where_none_opens_a_card() {
    // A keycap draws only as its one glyph, so two side by side stay two.
    assert_eq!(shown(&plain("1️⃣2️⃣")), "{1️⃣}{2️⃣}");
    let embed = markdown(Source::new("🇩🇰🇩🇰 ok"));
    assert_eq!(shown(&embed), "{🇩🇰}{🇩🇰} ok");
}

#[test]
fn a_builders_alike_spans_join() {
    let mut builder = Builder::new();
    builder
        .text("a", Style::default())
        .text("b", Style::default())
        .link("@Bob", Style::default(), Link::User("2".into()), None);
    let text = builder.finish();
    assert_eq!(shown(&text), "ab@[@Bob]");
    assert_eq!(text.blocks().next().map(|(_, spans)| spans.len()), Some(2));
    assert_eq!(text.clone(), text);
    assert!(text.same(&text.clone()));
    assert_eq!(Text::bold("x"), Text::bold("x"));
}

#[test]
fn a_notice_is_built_in_its_inks() {
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    let red = iced::Color::from_rgb8(0xe7, 0x4c, 0x3c);
    let mut builder = Builder::new();
    builder
        .link("Bob", bold, Link::Author("2".into()), Some(Ink::Own(red)))
        .text(" pinned ", Style::default())
        .link("a message", bold, Link::Jump("d:9".into()), Some(Ink::Text))
        .text(" to this channel.  ", Style::default())
        .colored("01:10", Style::default(), Ink::Muted);
    let text = builder.finish();
    assert_eq!(
        shown(&text),
        "[**Bob**](Author(\"2\")) pinned [**a message**](Jump(\"d:9\")) to this channel.  01:10"
    );
    let inks: Vec<Option<Ink>> = text
        .blocks()
        .flat_map(|(_, spans)| spans)
        .map(|span| span.color)
        .collect();
    assert_eq!(
        inks,
        [
            Some(Ink::Own(red)),
            None,
            Some(Ink::Text),
            None,
            Some(Ink::Muted)
        ]
    );
    assert!(
        text.blocks()
            .flat_map(|(_, spans)| spans)
            .filter(|span| span.link.is_some())
            .all(Span::underlines)
    );
}

#[test]
fn a_translated_sentence_takes_its_parts_where_its_language_puts_them() {
    let filled = |template: &str| {
        let mut builder = Builder::new();
        builder.template(template, Style::default(), |builder, place| {
            let (words, link) = match place {
                0 => ("a message", Link::Jump("9".into())),
                _ => ("the pins", Link::Pins),
            };
            builder.link(words, Style::default(), link, None);
        });
        shown(&builder.finish())
    };
    assert_eq!(
        filled(" pinned {} here. See {}."),
        " pinned [a message](Jump(\"9\")) here. See [the pins](Pins)."
    );
    assert_eq!(
        filled("{1}, then {0}"),
        "[the pins](Pins), then [a message](Jump(\"9\"))"
    );
    assert_eq!(filled("{{x}} {n} }"), "{x} {n} }");
}
