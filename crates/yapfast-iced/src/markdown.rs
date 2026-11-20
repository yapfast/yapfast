//! Message Markdown as Discord's client lays it out, for the timeline and
//! the header's topic: the core's blocks (headings, subtext, quotes, fenced
//! code) apart, and in each the inline styles as runs: bold, italic,
//! strikethrough, inline code, links, mentions in their colour, and
//! spoilers covered until clicked.
//!
//! Discord's rules, not CommonMark's, where they differ: every line break
//! and blank line shows, and what CommonMark would make a heading, a rule
//! or indented code beyond Discord's shows as typed. `__text__` is bold.
//! Custom emoji ([`with_emoji`]) take the room of their picture in the
//! text, which is drawn there.

use std::ops::Range;

use iced::{
    Color, Element, Font, Length, Padding,
    border::Radius,
    font::Style as FontStyle,
    widget::{Space, column, container, row, span, text::Span},
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use yapfast_core::{
    blocks::{self, Block, Kind},
    chat::{ChatMessage, Notice, NoticeKind},
    links,
    mention::{self, Mentioned},
    spoiler,
};

use crate::{
    app::Message,
    fonts::{BOLD, REGULAR},
    piece::Piece,
};

/// A run of text in one style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    /// Where a click goes: a web address, a mention's target, a spoiler.
    pub link: Option<String>,
    /// A mention's colour, `0xrrggbb`.
    pub color: Option<u32>,
    /// A spoiler not yet revealed: covered, its link reveals it.
    pub hidden: bool,
    /// A custom emoji: the address of its picture, and its `:name:`. Its
    /// text is [`EMOJI_MARK`], which keeps its picture's room.
    pub emoji: Option<(String, String)>,
    /// A word a search found, which its result highlights.
    pub highlight: bool,
}

/// Whether a coloured link is a mention drawn on a pill: a person's, a
/// role's or everyone's.
fn pilled(link: &str) -> bool {
    link.starts_with("yapfast:user/")
        || link.starts_with("yapfast:role/")
        || link == "yapfast:everyone"
}

/// What a search's found words are marked with in its results. The
/// client's colour is in a stylesheet Yapfast hasn't read, so this is a
/// warm marker of its own.
pub const SEARCH_HIGHLIGHT: Color = Color::from_rgba(0.98, 0.66, 0.1, 0.32);

/// Marks the words `words` found in `body`, as a search's results do:
/// everywhere but in code blocks, custom emoji and covered spoilers.
pub fn highlight(body: &mut Body, words: &yapfast_core::search_words::Words) {
    if words.is_empty() {
        return;
    }
    for part in &mut body.parts {
        if part.shape == Shape::Code {
            continue;
        }
        let mut runs = Vec::with_capacity(part.runs.len());
        for run in std::mem::take(&mut part.runs) {
            let marks = if run.emoji.is_some() || run.hidden {
                Vec::new()
            } else {
                words.marks(&run.text)
            };
            if marks.is_empty() {
                runs.push(run);
                continue;
            }
            let mut at = 0;
            for mark in marks {
                if mark.start > at {
                    runs.push(Run {
                        text: run.text[at..mark.start].to_owned(),
                        ..run.clone()
                    });
                }
                runs.push(Run {
                    text: run.text[mark.clone()].to_owned(),
                    highlight: true,
                    ..run.clone()
                });
                at = mark.end;
            }
            if at < run.text.len() {
                runs.push(Run {
                    text: run.text[at..].to_owned(),
                    ..run
                });
            }
        }
        part.runs = runs;
    }
}

/// What stands in a custom emoji's place in text: a space an em wide, its
/// span sized to the picture.
pub const EMOJI_MARK: char = '\u{2003}';
const EMOJI_MARK_STR: &str = "\u{2003}";
/// A custom emoji's picture against its text's size, as Discord's 22 px in
/// 16 px text.
pub const EMOJI_SCALE: f32 = 1.375;

/// One piece of text: literal text, or a custom emoji found by its `:name:`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlinePart<'a> {
    Text(&'a str),
    Emoji(&'a yapfast_core::chat::CustomEmoji),
}

/// Splits `text` into literal text and the custom `emoji` named in it, the
/// earliest name again and again.
pub fn split_emoji<'a>(
    text: &'a str,
    emoji: &'a [yapfast_core::chat::CustomEmoji],
) -> Vec<InlinePart<'a>> {
    if text.is_empty() {
        return Vec::new();
    }
    if emoji.is_empty() {
        return vec![InlinePart::Text(text)];
    }
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some((at, found)) = emoji
        .iter()
        .filter(|each| !each.text.is_empty())
        .filter_map(|each| rest.find(&each.text).map(|at| (at, each)))
        .min_by_key(|(at, _)| *at)
    {
        if at > 0 {
            parts.push(InlinePart::Text(&rest[..at]));
        }
        parts.push(InlinePart::Emoji(found));
        rest = &rest[at + found.text.len()..];
    }
    if !rest.is_empty() {
        parts.push(InlinePart::Text(rest));
    }
    parts
}

impl Run {
    fn same_style(&self, other: &Run) -> bool {
        self.bold == other.bold
            && self.italic == other.italic
            && self.strike == other.strike
            && self.code == other.code
            && self.link == other.link
            && self.color == other.color
            && self.hidden == other.hidden
            && self.highlight == other.highlight
            && self.emoji.is_none()
            && other.emoji.is_none()
    }
}

/// What a part is, as Discord sizes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Text,
    /// `#` to `###`.
    Heading(u8),
    Subtext,
    Code,
}

/// One block of a body, with the space Discord's client leaves above it
/// (`margin`) and above it inside a quote's bar (`padding`).
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub shape: Shape,
    pub quoted: bool,
    pub quote_start: bool,
    pub quote_end: bool,
    pub margin: f32,
    pub padding: f32,
    pub runs: Vec<Run>,
}

impl Part {
    /// Its text as it shows.
    pub fn plain(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }

    /// Its custom emoji's names, in order.
    pub fn emoji_names(&self) -> impl Iterator<Item = &str> {
        self.runs
            .iter()
            .filter_map(|run| run.emoji.as_ref().map(|(_, name)| name.as_str()))
    }
}

/// A message's or topic's text, laid out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Body {
    pub parts: Vec<Part>,
    /// Nothing but one to thirty emoji: they show large.
    pub jumbo: bool,
}

impl Body {
    /// Its text as it shows, parts on lines of their own.
    pub fn plain(&self) -> String {
        self.parts
            .iter()
            .map(Part::plain)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// `markdown` laid out: mentions linked in their colour (`link_color` for
/// those without one), spoilers covered but those `revealed` says were
/// clicked, counted through the message `item`.
pub fn parse(
    markdown: &str,
    mentions: &[Mentioned],
    link_color: u32,
    item: &str,
    revealed: impl Fn(usize) -> bool,
) -> Body {
    let split = blocks::split(markdown).unwrap_or_else(|| {
        vec![Block {
            kind: Kind::Text,
            quoted: false,
            runs: std::iter::once(0..markdown.len()).collect(),
        }]
    });
    let mut spoilers = 0;
    let mut parts = Vec::with_capacity(split.len());
    for (index, block) in split.iter().enumerate() {
        let typed = block.text(markdown);
        let runs = if block.kind == Kind::Code {
            vec![Run {
                text: typed.into_owned(),
                code: true,
                ..Run::default()
            }]
        } else {
            // The mentions in this block, at their places in its text.
            let here: Vec<Mentioned> = mentions
                .iter()
                .filter_map(|mention| {
                    let at: Vec<usize> = mention
                        .at
                        .iter()
                        .filter_map(|&at| block.offset(at, mention.text.len()))
                        .collect();
                    (!at.is_empty()).then(|| Mentioned {
                        at,
                        ..mention.clone()
                    })
                })
                .collect();
            let linked = mention::linked(&typed, &here, link_color);
            let marked = spoiler::mark(&linked, item, spoilers, &revealed);
            spoilers += spoiler::count(&linked);
            inline(&links::mark_masked(&links::linkify(&marked)))
        };
        let (margin, padding) = spacing(&split, index);
        parts.push(Part {
            shape: match block.kind {
                Kind::Text => Shape::Text,
                Kind::Heading(level) => Shape::Heading(level),
                Kind::Subtext => Shape::Subtext,
                Kind::Code => Shape::Code,
            },
            quoted: block.quoted,
            quote_start: block.quoted && !(index > 0 && split[index - 1].quoted),
            quote_end: block.quoted && !split.get(index + 1).is_some_and(|next| next.quoted),
            margin,
            padding,
            runs,
        });
    }
    let body = Body {
        jumbo: false,
        parts,
    };
    Body {
        jumbo: only_emoji(&body.plain()),
        ..body
    }
}

/// Each of `emoji` (custom emoji, by their `:name:`) in `body`'s text, out
/// of code, in a run of its own that its picture fills; nothing but emoji
/// shows large, as Discord's client does.
pub fn with_emoji(body: &mut Body, emoji: &[yapfast_core::chat::CustomEmoji]) {
    if emoji.is_empty() {
        return;
    }
    for part in &mut body.parts {
        if part.shape == Shape::Code {
            continue;
        }
        let mut runs = Vec::with_capacity(part.runs.len());
        for run in part.runs.drain(..) {
            if run.code || run.hidden {
                runs.push(run);
                continue;
            }
            let parts = split_emoji(&run.text, emoji);
            if let [InlinePart::Text(_)] = parts[..] {
                runs.push(run);
                continue;
            }
            for part in parts {
                runs.push(match part {
                    InlinePart::Text(text) => Run {
                        text: text.to_owned(),
                        ..run.clone()
                    },
                    InlinePart::Emoji(found) => Run {
                        text: EMOJI_MARK.to_string(),
                        emoji: Some((found.picture.clone(), found.text.clone())),
                        ..run.clone()
                    },
                });
            }
        }
        part.runs = runs;
    }
    body.jumbo = only_emoji(&body.plain().replace(EMOJI_MARK, "🙂"));
}

/// Each emoji in `body`'s text, out of code, links and spoilers, linked to
/// its card: a custom one of `custom` ([`with_emoji`] put in runs of their
/// own), and each Unicode one the client `drawn` in a run of its own. A
/// shortcode sent unexpanded, such as `:sob:`, shows as its emoji, as
/// Discord's client draws one.
pub fn link_emoji(
    body: &mut Body,
    custom: &[yapfast_core::chat::CustomEmoji],
    drawn: impl Fn(&str) -> bool,
) {
    for part in &mut body.parts {
        if part.shape == Shape::Code {
            continue;
        }
        let mut runs = Vec::with_capacity(part.runs.len());
        for mut run in part.runs.drain(..) {
            if let Some((_, name)) = &run.emoji
                && run.link.is_none()
            {
                run.link = custom
                    .iter()
                    .find(|emoji| &emoji.text == name)
                    .map(|emoji| yapfast_core::emoji::card::custom_link(&emoji.id));
            }
            if run.code || run.hidden || run.link.is_some() || run.emoji.is_some() {
                runs.push(run);
                continue;
            }
            let mut plain = String::new();
            let mut rest = run.text.as_str();
            while let Some(next) = rest.chars().next() {
                let found = if could_be_emoji(next) {
                    Some(emoji_at(rest))
                        .filter(|&length| length > 0)
                        .map(|length| (length, &rest[..length]))
                } else if next == ':' {
                    shortcode_at(rest)
                } else {
                    None
                };
                if let Some((length, emoji)) = found.filter(|(_, emoji)| drawn(emoji)) {
                    if !plain.is_empty() {
                        runs.push(Run {
                            text: std::mem::take(&mut plain),
                            ..run.clone()
                        });
                    }
                    runs.push(Run {
                        text: emoji.to_owned(),
                        link: Some(yapfast_core::emoji::card::unicode_link(emoji)),
                        ..run.clone()
                    });
                    rest = &rest[length..];
                    continue;
                }
                plain.push(next);
                rest = &rest[next.len_utf8()..];
            }
            if !plain.is_empty() {
                runs.push(Run { text: plain, ..run });
            }
        }
        part.runs = runs;
    }
}

/// Whether an emoji could start at `character`: emoji are beyond ASCII,
/// but the digits 0 to 9, `#` and `*` begin keycaps.
fn could_be_emoji(character: char) -> bool {
    !character.is_ascii() || character.is_ascii_digit() || "#*".contains(character)
}

/// The length of the longest emoji `text` starts with, or 0.
fn emoji_at(text: &str) -> usize {
    text.char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take(12)
        .filter(|&end| emojis::get(&text[..end]).is_some())
        .last()
        .unwrap_or(0)
}

/// The emoji `text` names at its start between colons, such as `:sob:`,
/// and how many bytes that name takes.
fn shortcode_at(text: &str) -> Option<(usize, &'static str)> {
    let rest = text.strip_prefix(':')?;
    let end = rest.find(':')?;
    let name = &rest[..end];
    let shaped = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-+".contains(character));
    if !shaped {
        return None;
    }
    emojis::get_by_shortcode(name).map(|emoji| (end + 2, emoji.as_str()))
}

/// Text without Markdown, as typed, its web addresses linked.
pub fn plain(text: &str) -> Body {
    let runs = inline(&links::linkify(&links::escape_markdown(text)));
    Body {
        jumbo: false,
        parts: vec![Part {
            shape: Shape::Text,
            quoted: false,
            quote_start: false,
            quote_end: false,
            margin: 0.0,
            padding: 0.0,
            runs,
        }],
    }
}

/// A notice as Discord's client words it: its author's name in bold in
/// their colour (`text`, the theme's, without one), opening their card;
/// what it tells of, a pin's with links to the message and to the pinned
/// messages; then when, which shows in `muted`.
pub fn notice(message: &ChatMessage, notice: &Notice, text: u32, muted: u32) -> Body {
    use crate::links::{AUTHOR_LINK, JUMP_LINK, PINS_LINK};
    let strong = |words: &str, rgb: u32, link: Option<&str>| {
        let words = format!("**{}**", links::escape_markdown(words));
        let words = match link {
            Some(link) => format!("[{words}](<{link}>)"),
            None => words,
        };
        format!("<font color=\"#{:06x}\">{words}</font>", rgb & 0xff_ffff)
    };
    let sender = yapfast_core::discord::sender_user(&message.sender).unwrap_or(&message.sender);
    let profile = (!sender.is_empty()).then(|| format!("{AUTHOR_LINK}{sender}"));
    let author = strong(
        &message.author,
        message.color.unwrap_or(text),
        profile.as_deref(),
    );
    let what = match &notice.kind {
        NoticeKind::Pinned(item) => {
            let jump = item.as_ref().map(|item| format!("{JUMP_LINK}{item}"));
            format!(
                " pinned {} to this channel. See all {}.",
                strong("a message", text, jump.as_deref()),
                strong("pinned messages", text, Some(PINS_LINK)),
            )
        }
        // A possessive follows the name without a space.
        _ if notice.text.starts_with('\'') => links::escape_markdown(&notice.text),
        _ => format!(" {}", links::escape_markdown(&notice.text)),
    };
    let mut runs = inline(&format!("{author}{what}"));
    runs.push(Run {
        text: format!("  {}", message.time),
        color: Some(muted),
        ..Run::default()
    });
    Body {
        jumbo: false,
        parts: vec![Part {
            shape: Shape::Text,
            quoted: false,
            quote_start: false,
            quote_end: false,
            margin: 0.0,
            padding: 0.0,
            runs,
        }],
    }
}

/// The space above block `index` and inside its quote's bar, as Discord's
/// client spaces them: a heading has 16 px above (less as the first), 8 px
/// below; code 6 px above; a quote 4 px around its bar.
fn spacing(blocks: &[Block], index: usize) -> (f32, f32) {
    const QUOTE: f32 = 4.0;
    let block = &blocks[index];
    let before = index.checked_sub(1).map(|index| &blocks[index]);
    let starts_quote = block.quoted && !before.is_some_and(|before| before.quoted);
    let first = before.is_none() || starts_quote;
    let top = match block.kind {
        Kind::Heading(3) if first => 4.0,
        Kind::Heading(_) if first => 8.0,
        Kind::Heading(_) => 16.0,
        Kind::Code => 6.0,
        Kind::Text | Kind::Subtext => 0.0,
    };
    let below = |kind: Kind| {
        if matches!(kind, Kind::Heading(_)) {
            8.0
        } else {
            0.0
        }
    };
    match before {
        None if block.quoted => (QUOTE, top),
        None => (top, 0.0),
        Some(before) if starts_quote => (QUOTE.max(below(before.kind)), top),
        Some(before) if block.quoted => (0.0, top.max(below(before.kind))),
        Some(before) if before.quoted => (QUOTE.max(top), 0.0),
        Some(before) => (top.max(below(before.kind)), 0.0),
    }
}

/// The style the parser is in.
#[derive(Default)]
struct Style {
    bold: u8,
    italic: u8,
    strike: u8,
    links: Vec<String>,
    colors: Vec<u32>,
}

/// Inline Markdown as runs. Paragraphs and list items keep the line breaks
/// between them as typed; block syntax Discord hasn't shows as typed.
fn inline(markdown: &str) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut style = Style::default();
    // Where the last paragraph or item ended in `markdown`, for the line
    // breaks between it and the next.
    let mut ended: Option<usize> = None;
    // Lists open, each with its next number when ordered.
    let mut lists: Vec<Option<u64>> = Vec::new();
    // Inside block syntax shown as typed, until it ends.
    let mut typed: Option<Range<usize>> = None;
    let push = |runs: &mut Vec<Run>, style: &Style, text: &str, code: bool| {
        if text.is_empty() {
            return;
        }
        let link = style.links.last().cloned();
        let run = Run {
            text: String::new(),
            bold: style.bold > 0,
            italic: style.italic > 0,
            strike: style.strike > 0,
            code,
            hidden: link
                .as_deref()
                .is_some_and(|link| spoiler::target(link).is_some()),
            link,
            color: style.colors.last().copied(),
            emoji: None,
            highlight: false,
        };
        match runs.last_mut() {
            Some(last) if last.same_style(&run) => last.text.push_str(text),
            _ => runs.push(Run {
                text: text.to_owned(),
                ..run
            }),
        }
    };
    // The line breaks typed between the end of one paragraph or item and
    // the start of the next, at least one.
    let breaks = |runs: &mut Vec<Run>, ended: Option<usize>, start: usize| {
        if let Some(end) = ended {
            let count = markdown
                .get(end..start)
                .map_or(1, |gap| gap.matches('\n').count().max(1));
            push(runs, &Style::default(), &"\n".repeat(count), false);
        }
    };
    // Where a block's text ends, before the line breaks it takes in.
    let end =
        |range: &Range<usize>| range.start + markdown[range.clone()].trim_end_matches('\n').len();
    let options = Options::ENABLE_STRIKETHROUGH;
    for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
        if let Some(shown) = &typed {
            if range.end <= shown.end && range != *shown {
                continue;
            }
            if range == *shown && matches!(event, Event::End(_)) {
                typed = None;
                ended = Some(end(&range));
                continue;
            }
        }
        match event {
            Event::Start(
                Tag::Heading { .. }
                | Tag::CodeBlock(_)
                | Tag::BlockQuote(_)
                | Tag::HtmlBlock
                | Tag::Table(_),
            ) => {
                breaks(&mut runs, ended.take(), range.start);
                let text = markdown[range.clone()].trim_end_matches('\n');
                push(&mut runs, &Style::default(), text, false);
                typed = Some(range);
            }
            Event::Rule => {
                breaks(&mut runs, ended.take(), range.start);
                let text = markdown[range.clone()].trim_end_matches('\n');
                push(&mut runs, &Style::default(), text, false);
                ended = Some(end(&range));
            }
            Event::Start(Tag::Paragraph) => {
                breaks(&mut runs, ended.take(), range.start);
            }
            Event::End(TagEnd::Paragraph) => ended = Some(end(&range)),
            Event::Start(Tag::List(start)) => {
                breaks(&mut runs, ended.take(), range.start);
                lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                lists.pop();
                ended = Some(end(&range));
            }
            Event::Start(Tag::Item) => {
                breaks(&mut runs, ended.take(), range.start);
                let depth = lists.len().saturating_sub(1);
                let marker = match lists.last_mut() {
                    Some(Some(number)) => {
                        *number += 1;
                        format!("{}. ", *number - 1)
                    }
                    _ => "• ".to_owned(),
                };
                push(
                    &mut runs,
                    &Style::default(),
                    &format!("{}{marker}", "    ".repeat(depth)),
                    false,
                );
            }
            Event::End(TagEnd::Item) => {
                ended = Some(end(&range));
            }
            Event::Start(Tag::Strong) => style.bold += 1,
            Event::End(TagEnd::Strong) => style.bold = style.bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => style.italic += 1,
            Event::End(TagEnd::Emphasis) => style.italic = style.italic.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => style.strike += 1,
            Event::End(TagEnd::Strikethrough) => style.strike = style.strike.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => style.links.push(dest_url.into_string()),
            Event::End(TagEnd::Link) => {
                style.links.pop();
            }
            // An image's text shows as typed, as Discord has none.
            Event::Start(Tag::Image { .. }) => {
                push(&mut runs, &style, &markdown[range.clone()], false);
                typed = Some(range);
            }
            Event::Text(text) => push(&mut runs, &style, &text, false),
            Event::Code(code) => push(&mut runs, &style, &code, true),
            Event::SoftBreak | Event::HardBreak => push(&mut runs, &style, "\n", false),
            Event::InlineHtml(html) | Event::Html(html) => {
                if let Some(color) = font_color(&html) {
                    style.colors.push(color);
                } else if html.trim().eq_ignore_ascii_case("</font>") {
                    style.colors.pop();
                } else {
                    push(&mut runs, &style, &html, false);
                }
            }
            _ => {}
        }
    }
    runs
}

/// The colour of a `<font color="#rrggbb">` tag.
fn font_color(html: &str) -> Option<u32> {
    let hex = html
        .trim()
        .strip_prefix("<font color=\"#")?
        .strip_suffix("\">")?;
    u32::from_str_radix(hex, 16).ok()
}

/// Whether `text` is one to thirty emoji and nothing else but spaces, which
/// Discord's client shows large.
pub fn only_emoji(text: &str) -> bool {
    let mut count = 0;
    for word in text.split_whitespace() {
        let mut rest = word;
        while !rest.is_empty() {
            // The longest emoji the rest starts with, such as a family.
            let end = emoji_at(rest);
            if end == 0 {
                return false;
            }
            rest = &rest[end..];
            count += 1;
        }
    }
    (1..=30).contains(&count)
}

// ---------------------------------------------------------------------------
// Drawn

/// How a body shows: its text's size and line height, and its ink.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub size: f32,
    pub line_height: f32,
    pub ink: Color,
}

pub use crate::piece::FIRST_PART;

impl<'c> crate::ctx::Ctx<'c> {
    /// A run as a span: its weight and slant, code in monospace on its
    /// ground, a link or mention in its colour, a covered spoiler blank on
    /// its cover.
    pub fn run_span<'a>(self, run: &'a Run, ink: Color) -> Span<'a, &'a str, Font> {
        let theme = self.look.theme;
        let font = if run.code {
            Font::MONOSPACE
        } else {
            Font {
                style: if run.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
                ..if run.bold { BOLD } else { REGULAR }
            }
        };
        let mut piece = span(run.text.as_str()).font(font).strikethrough(run.strike);
        if let Some(link) = run.link.as_deref() {
            piece = piece.link(link);
        }
        if run.hidden {
            return piece
                .color(theme.sunken)
                .background(theme.sunken)
                .border(crate::style::round(3));
        }
        if let Some(rgb) = run.color {
            let color = crate::style::hex(rgb);
            piece = piece.color(color);
            // A mention of a person, a role or everyone, on a translucent
            // pill of its colour; other coloured text (a notice's names,
            // links and time) shows in the colour alone.
            if run.link.as_deref().is_some_and(pilled) {
                piece = piece
                    .background(Color { a: 0.16, ..color })
                    .border(crate::style::round(4))
                    .padding(Padding {
                        top: -1.5,
                        bottom: -1.5,
                        left: 2.0,
                        right: 2.0,
                    });
            }
        } else if run.link.is_some() {
            piece = piece.color(theme.link);
        } else {
            piece = piece.color(ink);
        }
        if run.code {
            piece = piece
                .background(theme.panel)
                .border(crate::style::round(3))
                .padding(Padding::from([0, 2]));
        }
        if run.highlight {
            piece = piece
                .background(SEARCH_HIGHLIGHT)
                .border(crate::style::round(2));
        }
        piece
    }

    /// A body's parts, one under another: headings and subtext at their
    /// sizes, code in its box, quoted parts behind a bar. Each part is a
    /// piece of the timeline's selection in `row`, from `first`
    /// ([`FIRST_PART`] for a message's own text).
    pub fn body_view(
        self,
        body: &'c Body,
        row: usize,
        first: crate::piece::PartNo,
        look: Look,
    ) -> Element<'c, Message> {
        let theme = self.look.theme;
        let mut parts = column![].width(Length::Fill);
        // Animated custom emoji play in a message's text in the timeline or
        // the thread beside it; elsewhere (a bio, a topic, a search's
        // results) they show their still.
        let plays = self.lanes.row_look(row).is_some();
        for (index, part) in body.parts.iter().enumerate() {
            let number = first.saturating_add(
                crate::piece::PartNo::try_from(index).unwrap_or(crate::piece::PartNo::MAX),
            );
            let ratio = look.line_height / look.size;
            let (size, ink) = match part.shape {
                Shape::Heading(1) => (look.size * 1.5, theme.text),
                Shape::Heading(2) => (look.size * 1.25, theme.text),
                Shape::Heading(_) => (look.size, theme.text),
                Shape::Subtext => (look.size * 0.875, theme.secondary),
                Shape::Code => (look.size * 0.875, theme.text),
                Shape::Text if part.quoted => (look.size, theme.secondary),
                Shape::Text if body.jumbo => (look.size * 2.2, look.ink),
                Shape::Text => (look.size, look.ink),
            };
            let side = (size * EMOJI_SCALE).round();
            let mut pictures = Vec::new();
            let mut moving: Vec<(usize, Box<dyn crate::piece::Moving + 'c>)> = Vec::new();
            let spans: Vec<_> = part
                .runs
                .iter()
                .enumerate()
                .map(|(index, run)| {
                    let span = self.run_span(run, ink);
                    if let Some((url, _)) = &run.emoji {
                        let playing = plays
                            .then(|| self.playing_emoji(url, side as u32, &self.pictures.players))
                            .flatten();
                        if let Some(playing) = playing {
                            moving.push((index, Box::new(playing)));
                        } else if let Some(picture) =
                            self.picture(url, side as u32, side as u32, false)
                        {
                            pictures.push((index, picture));
                        }
                        return span.size(side);
                    }
                    match part.shape {
                        Shape::Heading(_) => span.font(BOLD),
                        Shape::Code => span.background(Color::TRANSPARENT),
                        _ => span,
                    }
                })
                .collect();
            let piece = Piece::new(spans, row, number, Message::Link, Message::Select)
                .unlined(yapfast_core::emoji::card::LINK)
                .item(self.lanes.row_look(row).map(|look| look.item.clone()))
                .pictures(pictures)
                .moving(moving)
                .size(size, (size * ratio).round())
                .color(ink)
                .fill()
                // Text in no timeline's row (a found card's) selects
                // nothing: a press on it is the card's.
                .starts(row != usize::MAX)
                .selection(
                    self.lanes.selected(row, number),
                    self.lanes.timeline.selecting,
                    theme.selection,
                )
                .spot(self.link_spot)
                .link_menu(|link| Message::Timeline(crate::timeline::Event::RowLink(link)));
            let shown: Element<'c, Message> = if part.shape == Shape::Code {
                let (ground, line) = (theme.panel, theme.line);
                container(piece)
                    .padding(size / 2.0)
                    .width(Length::Fill)
                    .style(move |_| container::Style {
                        background: Some(ground.into()),
                        border: crate::style::edge(line, 4.0),
                        ..container::Style::default()
                    })
                    .into()
            } else {
                piece.into()
            };
            let shown = container(shown).padding(Padding {
                top: part.padding,
                left: if part.quoted { QUOTE_INSET } else { 0.0 },
                ..Padding::ZERO
            });
            let shown: Element<'c, Message> = if part.quoted {
                let spine = theme.reply_spine;
                let radius = Radius {
                    top_left: if part.quote_start { 2.0 } else { 0.0 },
                    top_right: if part.quote_start { 2.0 } else { 0.0 },
                    bottom_left: if part.quote_end { 2.0 } else { 0.0 },
                    bottom_right: if part.quote_end { 2.0 } else { 0.0 },
                };
                let bar = container(Space::new().width(QUOTE_BAR).height(Length::Fill))
                    .style(move |_| crate::style::rounded(spine, radius));
                row![bar, shown.width(Length::Fill)]
                    .height(Length::Shrink)
                    .into()
            } else {
                shown.into()
            };
            parts = parts.push(container(shown).padding(Padding {
                top: part.margin,
                ..Padding::ZERO
            }));
        }
        parts.into()
    }

    /// Single-line plain text with inline custom emoji pictures (animating
    /// when `plays` is true), falling back cleanly to `:name:` until each
    /// emoji's picture arrives; prefixed by `prefix` spans if any.
    pub fn inline_emoji_line(
        self,
        prefix: &[&'c str],
        text: &'c str,
        emoji: &'c [yapfast_core::chat::CustomEmoji],
        look: Look,
        emoji_side: u32,
        plays: bool,
    ) -> Element<'c, Message> {
        let mut spans: Vec<Span<'c, &'c str, Font>> = Vec::new();
        let mut pictures = Vec::new();
        let mut moving: Vec<(usize, Box<dyn crate::piece::Moving + 'c>)> = Vec::new();
        for &part in prefix {
            if !part.is_empty() {
                spans.push(span(part).color(look.ink));
            }
        }
        for part in split_emoji(text, emoji) {
            match part {
                InlinePart::Text(text) => spans.push(span(text).color(look.ink)),
                InlinePart::Emoji(custom) => {
                    let index = spans.len();
                    let playing = plays
                        .then(|| {
                            self.playing_emoji(&custom.picture, emoji_side, &self.pictures.players)
                        })
                        .flatten();
                    if let Some(playing) = playing {
                        moving.push((index, Box::new(playing)));
                        spans.push(span(EMOJI_MARK_STR).size(emoji_side as f32).color(look.ink));
                    } else if let Some(picture) =
                        self.picture(&custom.picture, emoji_side, emoji_side, false)
                    {
                        pictures.push((index, picture));
                        spans.push(span(EMOJI_MARK_STR).size(emoji_side as f32).color(look.ink));
                    } else {
                        spans.push(span(custom.text.as_str()).color(look.ink));
                    }
                }
            }
        }
        if spans.is_empty() {
            spans.push(span("").color(look.ink));
        }
        Piece::new(spans, usize::MAX, 0, Message::Link, Message::Select)
            .pictures(pictures)
            .moving(moving)
            .size(look.size, look.line_height)
            .color(look.ink)
            .starts(false)
            .into()
    }
}

/// A quote's bar, and the room its text keeps from it.
const QUOTE_BAR: f32 = 4.0;
const QUOTE_INSET: f32 = 12.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn runs(markdown: &str) -> Vec<Run> {
        parse(markdown, &[], 0x00a8fc, "m", |_| false)
            .parts
            .remove(0)
            .runs
    }

    #[test]
    fn emoji_link_to_their_cards_out_of_code_and_links() {
        let custom = vec![yapfast_core::chat::CustomEmoji {
            text: ":pog:".into(),
            id: "42".into(),
            animated: false,
            usable: true,
            picture: "https://cdn.discordapp.com/emojis/42.png".into(),
        }];
        let mut body = parse(
            "so 😭 :sob: `😭` [😭](https://example.com) :pog: :nope:",
            &[],
            0x00a8fc,
            "m",
            |_| false,
        );
        with_emoji(&mut body, &custom);
        link_emoji(&mut body, &custom, |_| true);
        let runs = &body.parts[0].runs;
        let linked: Vec<(&str, &str)> = runs
            .iter()
            .filter_map(|run| Some((run.text.as_str(), run.link.as_deref()?)))
            .collect();
        assert_eq!(
            linked,
            [
                ("😭", "yapfast:emoji/1f62d"),
                // A shortcode sent unexpanded shows as its emoji.
                ("😭", "yapfast:emoji/1f62d"),
                ("😭", "yapfast:leave/https://example.com"),
                ("\u{2003}", "yapfast:emoji/c/42"),
            ]
        );
        assert!(
            runs.iter().any(|run| run.code && run.text == "😭"),
            "code keeps its emoji as typed"
        );
        assert!(body.plain().ends_with(" :nope:"), "no such shortcode");
        // One the client doesn't draw stays plain text.
        let mut body = parse("😭", &[], 0x00a8fc, "m", |_| false);
        link_emoji(&mut body, &[], |_| false);
        assert!(body.parts[0].runs.iter().all(|run| run.link.is_none()));
    }

    #[test]
    fn inline_styles_come_apart_into_runs() {
        let runs = runs("**bold** _it_ ~~gone~~ `code` __under__");
        let find = |text: &str| runs.iter().find(|run| run.text == text).unwrap().clone();
        assert!(find("bold").bold);
        assert!(find("it").italic);
        assert!(find("gone").strike);
        assert!(find("code").code);
        assert!(find("under").bold);
    }

    #[test]
    fn every_line_break_and_blank_line_shows() {
        let body = parse("one\ntwo\n\n\nthree", &[], 0, "m", |_| false);
        assert_eq!(body.plain(), "one\ntwo\n\n\nthree");
    }

    #[test]
    fn lists_show_bullets_and_numbers() {
        let body = parse("- a\n- b\n\n1. x\n2. y", &[], 0, "m", |_| false);
        assert_eq!(body.plain(), "• a\n• b\n\n1. x\n2. y");
    }

    #[test]
    fn syntax_discord_lacks_shows_as_typed() {
        let body = parse("a\n---\n#### four", &[], 0, "m", |_| false);
        assert_eq!(body.plain(), "a\n---\n#### four");
    }

    #[test]
    fn addresses_link_and_spoilers_cover_until_revealed() {
        let runs = runs("see https://example.com and ||secret||");
        let link = runs.iter().find(|run| run.link.is_some()).unwrap();
        assert_eq!(link.text, "https://example.com");
        let secret = runs.iter().find(|run| run.text == "secret").unwrap();
        assert!(secret.hidden);
        let shown = parse("||secret||", &[], 0, "m", |_| true);
        assert!(!shown.parts[0].runs[0].hidden);
    }

    #[test]
    fn headings_quotes_and_code_are_parts_of_their_own() {
        let body = parse("# Title\n> quoted\n```\nlet x;\n```", &[], 0, "m", |_| {
            false
        });
        let shapes: Vec<_> = body
            .parts
            .iter()
            .map(|part| (part.shape, part.quoted))
            .collect();
        assert_eq!(
            shapes,
            [
                (Shape::Heading(1), false),
                (Shape::Text, true),
                (Shape::Code, false)
            ]
        );
        assert_eq!(body.parts[2].plain(), "let x;");
    }

    #[test]
    fn a_mention_is_a_coloured_link() {
        let mentions = [Mentioned {
            text: "@Bob".into(),
            at: vec![3],
            color: Some(0x123456),
            target: mention::Target::User("1".into()),
        }];
        let body = parse("hi @Bob!", &mentions, 0, "m", |_| false);
        let bob = body.parts[0]
            .runs
            .iter()
            .find(|run| run.text == "@Bob")
            .unwrap();
        assert_eq!(bob.color, Some(0x123456));
        assert!(bob.link.as_deref().is_some_and(pilled), "on a pill");
    }

    #[test]
    fn a_notices_names_links_and_time_are_coloured_but_not_pilled() {
        use yapfast_core::chat::{ChatMessage, Notice, NoticeKind};
        let message = ChatMessage {
            author: "Ann".into(),
            sender: "discord:5".into(),
            time: "12:01".into(),
            ..Default::default()
        };
        let pinned = Notice {
            kind: NoticeKind::Pinned(Some("m1".into())),
            text: String::new(),
        };
        let body = notice(&message, &pinned, 0xffffff, 0x808080);
        let coloured: Vec<_> = body.parts[0]
            .runs
            .iter()
            .filter(|run| run.color.is_some())
            .collect();
        assert!(coloured.len() >= 3, "{coloured:?}");
        assert!(
            coloured
                .iter()
                .all(|run| !run.link.as_deref().is_some_and(pilled)),
            "none on a pill"
        );
    }

    #[test]
    fn emoji_alone_show_large() {
        assert!(only_emoji("🎉 👍🏽"));
        assert!(!only_emoji("🎉 yes"));
    }

    #[test]
    fn split_emoji_separates_text_and_custom_emoji_in_order() {
        let emoji = vec![
            yapfast_core::chat::CustomEmoji {
                text: ":AMkiss:".into(),
                id: "1".into(),
                animated: false,
                usable: true,
                picture: "https://cdn/emojis/1.png".into(),
            },
            yapfast_core::chat::CustomEmoji {
                text: ":party:".into(),
                id: "2".into(),
                animated: true,
                usable: true,
                picture: "https://cdn/emojis/2.gif".into(),
            },
        ];
        assert_eq!(
            split_emoji("hi :AMkiss::party: and :nope:", &emoji),
            vec![
                InlinePart::Text("hi "),
                InlinePart::Emoji(&emoji[0]),
                InlinePart::Emoji(&emoji[1]),
                InlinePart::Text(" and :nope:"),
            ]
        );
        assert!(split_emoji("", &emoji).is_empty());
        assert_eq!(split_emoji("plain", &[]), vec![InlinePart::Text("plain")]);
    }
}
