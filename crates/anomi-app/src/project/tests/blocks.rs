//! A message's Markdown in the blocks Discord's client lays out, as its row,
//! an embed and a found message show them: the core splits them
//! (`yapfast_core::blocks`' tests), the text module spaces them
//! (`anomi_vm::text`'s), and these hold the row to both.

use yapfast_core::application::AppState;
use yapfast_core::chat::{ChatMessage, Embed, EmbedField};
use yapfast_core::mention::{Mentioned, Target};

use anomi_vm::text::{Level, Text};

use super::{row, row_revealed};
use crate::project::chat::message;

fn formatted(body: &str) -> ChatMessage {
    ChatMessage {
        item: "d:1".into(),
        body: body.into(),
        formatted: true,
        ..Default::default()
    }
}

/// Each block's level, whether it's quoted, and its characters.
fn blocks(text: &Text) -> Vec<(Level, bool, String)> {
    text.blocks()
        .map(|(block, spans)| {
            let chars: String = spans.iter().map(|span| text.chars(span)).collect();
            (block.level, block.quoted, chars)
        })
        .collect()
}

#[test]
fn a_message_shows_its_headings_quotes_and_code_apart() {
    let body = row(&formatted(
        "# Title\n> quoted **bold**\n> -# small\n```\n<b>code</b>\n```\n-# end",
    ))
    .body;
    let shown = |level, quoted, text: &str| (level, quoted, text.to_owned());
    assert_eq!(
        blocks(&body),
        [
            shown(Level::Heading(1), false, "Title"),
            shown(Level::Text, true, "quoted bold"),
            shown(Level::Subtext, true, "small"),
            shown(Level::Code, false, "<b>code</b>"),
            shown(Level::Subtext, false, "end"),
        ]
    );
    let spaced: Vec<_> = body.blocks().map(|(block, _)| block).collect();
    // The quote's bar runs from its first block to its last.
    assert!(spaced[1].quote_start && !spaced[1].quote_end);
    assert!(!spaced[2].quote_start && spaced[2].quote_end);
    // A first heading sits closer to the top than a later one would.
    assert_eq!((spaced[0].margin, spaced[0].padding), (8.0, 0.0));
    assert_eq!((spaced[1].margin, spaced[1].padding), (8.0, 0.0));
    assert_eq!(spaced[3].margin, 6.0);
    assert_eq!(body.read(), "Title\nquoted bold\nsmall\n<b>code</b>\nend");
}

#[test]
fn text_alone_shows_as_one() {
    let body = row(&formatted("plain **text**\n- and a list")).body;
    assert_eq!(body.blocks().len(), 1);
    assert!(body.read().starts_with("plain text\n"));
}

#[test]
fn text_beside_a_heading_keeps_its_formatting_and_blank_lines() {
    let body = row(&formatted("# Title\n\n**bold**\n---")).body;
    let plain: Vec<String> = blocks(&body).into_iter().map(|(.., text)| text).collect();
    assert_eq!(plain, ["Title", "\u{a0}\nbold\n---"]);
}

#[test]
fn html_in_a_block_shows_as_typed() {
    // Markdown would make the text after the open fence code, which the
    // HTML's escaping skips, but Discord's client shows it as text.
    let body = row(&formatted(
        "```\n# Title\n<font color=\"#ff0000\">red</font>",
    ))
    .body;
    let last = blocks(&body).pop().expect("blocks");
    assert_eq!(last.2, "<font color=\"#ff0000\">red</font>");
}

#[test]
fn a_mention_in_a_quote_still_links() {
    let mut message = formatted("# Hi\n> and @Bob");
    let plain = row(&message).body;
    message.mentions = vec![Mentioned {
        text: "@Bob".into(),
        target: Target::User("2".into()),
        color: None,
        at: vec![message.body.find('@').expect("the mention")],
    }];
    let mentioned = row(&message).body;
    let quote = |text: &Text| format!("{:?}", text.blocks().nth(1).map(|(_, spans)| spans));
    assert_ne!(quote(&plain), quote(&mentioned));
    assert_eq!(blocks(&mentioned)[1].2, "and @Bob");
    assert!(
        mentioned
            .blocks()
            .nth(1)
            .is_some_and(|(_, spans)| spans.iter().any(|span| span.opens())),
        "the mention opens its card"
    );
}

#[test]
fn spoilers_count_on_across_blocks() {
    let message = ChatMessage {
        spoilers: true,
        ..formatted("||a||\n# T ||b||")
    };
    let shown = |revealed: &[&str]| -> Vec<String> {
        let body = row_revealed(&AppState::default(), &message, revealed).body;
        body.blocks()
            .map(|(_, spans)| format!("{spans:?}"))
            .collect()
    };
    let hidden = shown(&[]);
    let second = shown(&["d:1/1"]);
    assert_eq!(hidden[0], second[0], "the first is still hidden");
    assert_ne!(hidden[1], second[1], "the heading's is the second");
}

#[test]
fn an_embed_shows_headings_and_code_in_its_description_and_fields() {
    let field = |name: &str, value: &str| EmbedField {
        name: name.into(),
        value: value.into(),
        inline: false,
    };
    let message = ChatMessage {
        embeds: vec![Embed {
            description: "## Status\n```\nall good\n```".into(),
            fields: vec![field("Plain", "just **text**"), field("Quoted", "> said")],
            ..Default::default()
        }],
        ..formatted("")
    };
    let embeds = row(&message).embeds;
    let card = &embeds.cards[0];
    let description = card.description.as_ref().expect("its description");
    let levels: Vec<Level> = blocks(description)
        .into_iter()
        .map(|block| block.0)
        .collect();
    assert_eq!(levels, [Level::Heading(2), Level::Code]);
    let fields: Vec<_> = card.rows.iter().flatten().collect();
    assert_eq!(fields[0].value.blocks().len(), 1);
    assert_eq!(fields[0].value.read(), "just text");
    assert!(blocks(&fields[1].value)[0].1, "quoted");
}

#[test]
fn a_found_message_shows_its_blocks_with_spoilers_hidden() {
    let message = ChatMessage {
        spoilers: true,
        ..formatted("# Plans\n> meet at ||the park||")
    };
    let found = message::found_text(&AppState::default(), &message, "").expect("its text");
    assert_eq!(found.read(), "Plans\nmeet at (spoiler)");
    assert_eq!(found.blocks().len(), 2);
}
