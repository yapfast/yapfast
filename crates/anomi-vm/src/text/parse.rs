//! Someone's Markdown as a [`Text`], as `src/ui/messages.rs` made Slint's
//! styled text of it: as one block when it's text alone (`message_row`'s
//! `styled`, `markdown:207`), or in the blocks the core splits out, each
//! spaced as Discord's client spaces them (`blocks_of`, `body_blocks`).

use std::borrow::Cow;

use super::build::Builder;
use super::emoji::{emoji_at, only_emoji};
use super::inline;
use super::links::{addresses, linkify};
use super::prepare::{self, Markers};
use super::source::{Kind, Section, Source};
use super::{Block, Level, Link, Style, Text};

/// Someone's Markdown, with its mentions, custom emoji and spoilers, as
/// spans in blocks. A part Slint's Markdown refused shows as typed, its web
/// addresses linked; fenced code shows as typed.
pub fn markdown(source: Source<'_>) -> Text {
    let mut builder = Builder::new();
    let markers = Markers::for_markdown(source.markdown);
    match source.sections.filter(|sections| !sections.is_empty()) {
        Some(sections) => blocks(&source, sections, &markers, &mut builder),
        None => whole(&source, &markers, &mut builder),
    }
    builder.finish()
}

/// Text that isn't Markdown, as typed, with its web addresses linked and
/// its emoji drawn as Twemoji's: a Matrix message's plain body, a name.
pub fn plain(text: &str) -> Text {
    let mut builder = Builder::new();
    builder.block(Block::default());
    plain_into(text, &mut builder);
    builder.finish()
}

/// [`plain`] into the open block.
fn plain_into(text: &str, builder: &mut Builder) {
    let mut copied = 0;
    for url in addresses(text) {
        emoji_into(&text[copied..url.start], builder);
        let link = Link::Url(text[url.clone()].to_owned());
        builder.link(&text[url.clone()], Style::default(), link, None);
        copied = url.end;
    }
    emoji_into(&text[copied..], builder);
}

/// `text` with its emoji apart, opening no cards.
fn emoji_into(text: &str, builder: &mut Builder) {
    let plain = Style::default();
    let mut copied = 0;
    let mut at = 0;
    while let Some(next) = text[at..].chars().next() {
        let length = emoji_at(&text[at..]);
        if length == 0 {
            at += next.len_utf8();
            continue;
        }
        builder.text(&text[copied..at], plain);
        builder.emoji(&text[at..at + length], false);
        at += length;
        copied = at;
    }
    builder.text(&text[copied..], plain);
}

/// Each place a mention is in `section`'s text, with its index among the
/// source's mentions; the whole Markdown's when there's no section.
fn mentions_in(source: &Source<'_>, section: Option<&Section>) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for (index, mention) in source.mentions.iter().enumerate() {
        for &at in &mention.at {
            let at = match section {
                Some(section) => section.offset(at, mention.text.len()),
                None => Some(at),
            };
            found.extend(at.map(|at| (at, index)));
        }
    }
    found
}

/// Text alone, as one block: a message of emoji alone is large.
fn whole(source: &Source<'_>, markers: &Markers, builder: &mut Builder) {
    let mentioned = prepare::mentions(
        source.markdown,
        mentions_in(source, None),
        source.mentions,
        markers,
    );
    let jumbo = source.cards && only_emoji(&mentioned, source.emoji);
    let (marked, _) = prepare::mark_spoilers(mentioned, source.spoilers, 0, markers);
    builder.block(Block::default());
    styled(&marked, source.markdown, source, markers, builder);
    builder.set_jumbo(jumbo);
}

/// `prepared` as spans, its rules escaped, blank lines kept and addresses
/// linked; or `typed` as typed, when Slint's Markdown would refuse it.
fn styled(
    prepared: &str,
    typed: &str,
    source: &Source<'_>,
    markers: &Markers,
    builder: &mut Builder,
) {
    let body = prepare::no_rules(prepared);
    let body = prepare::keep_blank_lines(&body);
    let body = linkify(&body);
    if inline::supported(&body) {
        inline::spans(&body, source, markers, builder);
    } else {
        plain_into(typed.trim_matches('\n'), builder);
    }
}

/// The space above a block of `kind`, and below it, which the next block's
/// takes in, as margins between them do; a first heading's is less.
fn above(kind: Kind, first: bool) -> f32 {
    match kind {
        Kind::Heading(3) if first => 4.0,
        Kind::Heading(_) if first => 8.0,
        Kind::Heading(_) => 16.0,
        Kind::Code => 6.0,
        Kind::Text | Kind::Subtext => 0.0,
    }
}

fn below(kind: Kind) -> f32 {
    if matches!(kind, Kind::Heading(_)) {
        8.0
    } else {
        0.0
    }
}

/// A quote has 4px above and below its bar.
const QUOTE: f32 = 4.0;

/// The core's `sections`, each with its mentions, emoji and spoilers;
/// spoilers count on through the message, as clicking one reveals it.
fn blocks(source: &Source<'_>, sections: &[Section], markers: &Markers, builder: &mut Builder) {
    let mut spoilers = 0;
    for (index, section) in sections.iter().enumerate() {
        let before = index.checked_sub(1).map(|index| &sections[index]);
        let starts_quote = section.quoted && !before.is_some_and(|before| before.quoted);
        let top = above(section.kind, before.is_none() || starts_quote);
        let (margin, padding) = match before {
            None if section.quoted => (QUOTE, top),
            None => (top, 0.0),
            Some(before) if starts_quote => (QUOTE.max(below(before.kind)), top),
            Some(before) if section.quoted => (0.0, top.max(below(before.kind))),
            Some(before) if before.quoted => (QUOTE.max(top), 0.0),
            Some(before) => (top.max(below(before.kind)), 0.0),
        };
        builder.block(Block {
            level: match section.kind {
                Kind::Text => Level::Text,
                Kind::Heading(level) => Level::Heading(level.clamp(1, 3)),
                Kind::Subtext => Level::Subtext,
                Kind::Code => Level::Code,
            },
            quoted: section.quoted,
            quote_start: starts_quote,
            quote_end: section.quoted && !sections.get(index + 1).is_some_and(|next| next.quoted),
            margin,
            padding,
            ..Block::default()
        });
        let typed = section.text(source.markdown);
        if section.kind == Kind::Code {
            builder.text(&typed, Style::default());
            continue;
        }
        let found = mentions_in(source, Some(section));
        let mentioned = prepare::mentions(&typed, found, source.mentions, markers);
        let (marked, count) = prepare::mark_spoilers(mentioned, source.spoilers, spoilers, markers);
        spoilers += count;
        let prepared: Cow<'_, str> = match section.kind {
            Kind::Heading(_) => format!("**{marked}**").into(),
            // Discord's client shows every line of text, blank or not.
            Kind::Text => prepare::every_line(&marked).into_owned().into(),
            _ => marked,
        };
        styled(&prepared, &typed, source, markers, builder);
    }
}
