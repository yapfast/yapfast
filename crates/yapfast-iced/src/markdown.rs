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
/// A code block's coloured span, which iced's own [`Span`] is not.
#[cfg(feature = "plugins")]
use yapfast_plugin_api::code::Span as CodeSpan;

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
    /// What a plugin made of this part of a code block, which the theme
    /// colours: only a fenced block's runs carry one.
    #[cfg(feature = "plugins")]
    pub tone: Option<yapfast_plugin_api::code::Tone>,
    /// How a plugin's mark moves the words, drawn a frame at a time.
    pub motion: Option<Motion>,
    /// A line a plugin shows while the pointer is over the run.
    pub tip: Option<String>,
    /// A picture of a plugin's drawn in a circle.
    pub round: bool,
}

pub use crate::piece::Motion;

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

/// A plugin's style over words a message already says, as the timeline lays it
/// out (`docs/plugins/interface.md`, the render hook). It names words and never
/// offsets, so it styles what the text says however its markdown laid out.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Mark {
    pub text: String,
    pub look: MarkLook,
    /// Where a link mark goes.
    pub url: Option<String>,
    /// A line on hover.
    pub tip: Option<String>,
}

/// Everything plugins lay over a message's text: styles over its words,
/// pictures set beside words or mentions, lines over its links on hover, and
/// emoji hidden or swapped. A build without plugins lays nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Decor {
    pub marks: Vec<Mark>,
    pub pictures: Vec<Picture>,
    pub tips: Vec<Tip>,
    pub swaps: Vec<Swap>,
}

impl Decor {
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
            && self.pictures.is_empty()
            && self.tips.is_empty()
            && self.swaps.is_empty()
    }
}

/// A small picture set beside some words or a mention.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Picture {
    pub url: String,
    pub beside: Beside,
    pub after: bool,
    pub round: bool,
    pub tip: Option<String>,
}

/// What a [`Picture`] goes beside.
#[cfg_attr(
    not(feature = "plugins"),
    allow(dead_code, reason = "only a plugin makes one")
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Beside {
    Words(String),
    /// Every link to this address: a mention's.
    Link(String),
}

/// A line over every run linking to `link`, on hover.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Tip {
    pub link: String,
    pub text: String,
}

/// An emoji, by a custom one's id or `:name:` or as typed, hidden (`with`
/// none) or shown as some text or another custom emoji's picture.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Swap {
    pub emoji: String,
    pub with: Option<SwapWith>,
}

#[cfg_attr(
    not(feature = "plugins"),
    allow(dead_code, reason = "only a plugin makes one")
)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SwapWith {
    Text(String),
    Picture(String),
}

/// How a [`Mark`] draws its words. Only a plugin makes one, so a build
/// without plugins has none.
#[cfg_attr(
    not(feature = "plugins"),
    allow(dead_code, reason = "only a plugin makes a mark")
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkLook {
    Bold,
    Italic,
    Strike,
    Highlight,
    Spoiler,
    Link,
    Wiggle,
    Rainbow,
}

/// Where plugins' spoilers count from in a message, past any it has of its
/// own, so revealing one never reveals the other.
pub const PLUGIN_SPOILERS: usize = 1 << 20;

/// Lays plugins' decor over a message's text: every place a plain run says a
/// mark's words takes its style. A link, a mention, an emoji, a spoiler and a
/// code block are left as they are, so a mark can't dress one up as another,
/// and the words themselves never change. A spoiler mark covers its words
/// through the message `item` until `revealed` says it was clicked.
///
/// Then emoji are swapped, pictures set beside their words or mentions, and
/// tooltips put on the links they name.
pub fn mark(body: &mut Body, decor: &Decor, item: &str, revealed: impl Fn(usize) -> bool) {
    if decor.is_empty() {
        return;
    }
    marks(body, &decor.marks, item, &revealed);
    swap_emoji(body, &decor.swaps);
    set_pictures(body, &decor.pictures);
    for tip in &decor.tips {
        let masked = format!("{}{}", links::LEAVE, tip.link);
        for run in body.parts.iter_mut().flat_map(|part| &mut part.runs) {
            if run
                .link
                .as_deref()
                .is_some_and(|link| link == tip.link || link == masked)
            {
                run.tip = Some(tip.text.clone());
            }
        }
    }
}

fn marks(body: &mut Body, marks: &[Mark], item: &str, revealed: &impl Fn(usize) -> bool) {
    if marks.is_empty() {
        return;
    }
    let mut spoilers = PLUGIN_SPOILERS;
    for part in &mut body.parts {
        if part.shape == Shape::Code {
            continue;
        }
        let mut runs = Vec::with_capacity(part.runs.len());
        for run in std::mem::take(&mut part.runs) {
            if run.code || run.link.is_some() || run.emoji.is_some() || run.hidden {
                runs.push(run);
                continue;
            }
            let mut at = 0;
            loop {
                let rest = &run.text[at..];
                let found = marks
                    .iter()
                    .filter_map(|mark| rest.find(&mark.text).map(|start| (start, mark)))
                    .min_by_key(|(start, _)| *start);
                let Some((start, mark)) = found else {
                    break;
                };
                if start > 0 {
                    runs.push(Run {
                        text: rest[..start].to_owned(),
                        ..run.clone()
                    });
                }
                let mut marked = Run {
                    text: mark.text.clone(),
                    ..run.clone()
                };
                match mark.look {
                    MarkLook::Bold => marked.bold = true,
                    MarkLook::Italic => marked.italic = true,
                    MarkLook::Strike => marked.strike = true,
                    MarkLook::Highlight => marked.highlight = true,
                    MarkLook::Spoiler => {
                        if !revealed(spoilers) {
                            marked.hidden = true;
                            marked.link = Some(spoiler::link(item, spoilers));
                        }
                        spoilers += 1;
                    }
                    MarkLook::Link => {
                        marked.link = mark
                            .url
                            .as_ref()
                            .map(|url| format!("{}{url}", links::LEAVE));
                    }
                    MarkLook::Wiggle => marked.motion = Some(Motion::Wiggle),
                    MarkLook::Rainbow => marked.motion = Some(Motion::Rainbow),
                }
                if !marked.hidden {
                    marked.tip.clone_from(&mark.tip);
                }
                runs.push(marked);
                at += start + mark.text.len();
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

/// Hides or swaps the emoji `swaps` name: a custom emoji's run by its id or
/// `:name:`, a typed emoji wherever plain text or an emoji's link says it.
fn swap_emoji(body: &mut Body, swaps: &[Swap]) {
    if swaps.is_empty() {
        return;
    }
    let card = yapfast_core::emoji::card::LINK;
    let into = |run: &Run, with: &Option<SwapWith>| -> Option<Run> {
        let base = Run {
            link: None,
            emoji: None,
            ..run.clone()
        };
        match with {
            None => None,
            Some(SwapWith::Text(text)) => Some(Run {
                text: text.clone(),
                ..base
            }),
            Some(SwapWith::Picture(url)) => Some(Run {
                text: EMOJI_MARK.to_string(),
                emoji: Some((url.clone(), String::new())),
                ..base
            }),
        }
    };
    for part in &mut body.parts {
        if part.shape == Shape::Code {
            continue;
        }
        let mut runs = Vec::with_capacity(part.runs.len());
        'runs: for run in std::mem::take(&mut part.runs) {
            if run.code || run.hidden {
                runs.push(run);
                continue;
            }
            if let Some((url, name)) = &run.emoji {
                let found = swaps.iter().find(|swap| {
                    swap.emoji == *name
                        || (swap.emoji.bytes().all(|b| b.is_ascii_digit())
                            && url.contains(&format!("/emojis/{}.", swap.emoji)))
                });
                match found {
                    Some(swap) => runs.extend(into(&run, &swap.with)),
                    None => runs.push(run),
                }
                continue;
            }
            if run
                .link
                .as_deref()
                .is_some_and(|link| !link.starts_with(card))
            {
                runs.push(run);
                continue;
            }
            // Typed emoji: each place the text says one, the earliest first.
            let mut at = 0;
            loop {
                let rest = &run.text[at..];
                let found = swaps
                    .iter()
                    .filter(|swap| !swap.emoji.starts_with(':'))
                    .filter(|swap| !swap.emoji.bytes().all(|b| b.is_ascii_digit()))
                    .filter_map(|swap| rest.find(&swap.emoji).map(|start| (start, swap)))
                    .min_by_key(|(start, _)| *start);
                let Some((start, swap)) = found else {
                    if at == 0 {
                        runs.push(run);
                        continue 'runs;
                    }
                    break;
                };
                if start > 0 {
                    runs.push(Run {
                        text: rest[..start].to_owned(),
                        ..run.clone()
                    });
                }
                runs.extend(into(&run, &swap.with));
                at += start + swap.emoji.len();
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

/// Sets each of `pictures` beside every place its words are said in plain
/// text, or beside every run linking where it says (a mention).
fn set_pictures(body: &mut Body, pictures: &[Picture]) {
    if pictures.is_empty() {
        return;
    }
    let picture_run = |picture: &Picture| Run {
        text: EMOJI_MARK.to_string(),
        emoji: Some((picture.url.clone(), String::new())),
        round: picture.round,
        tip: picture.tip.clone(),
        ..Run::default()
    };
    for part in &mut body.parts {
        if part.shape == Shape::Code {
            continue;
        }
        let mut runs = Vec::with_capacity(part.runs.len());
        for run in std::mem::take(&mut part.runs) {
            if run.code || run.hidden || run.emoji.is_some() {
                runs.push(run);
                continue;
            }
            if let Some(link) = run.link.as_deref() {
                let beside: Vec<&Picture> = pictures
                    .iter()
                    .filter(|picture| picture.beside == Beside::Link(link.to_owned()))
                    .collect();
                runs.extend(beside.iter().filter(|p| !p.after).map(|p| picture_run(p)));
                let after: Vec<Run> = beside
                    .iter()
                    .filter(|p| p.after)
                    .map(|p| picture_run(p))
                    .collect();
                runs.push(run);
                runs.extend(after);
                continue;
            }
            let mut at = 0;
            loop {
                let rest = &run.text[at..];
                let found = pictures
                    .iter()
                    .filter_map(|picture| match &picture.beside {
                        Beside::Words(words) if !words.is_empty() => rest
                            .find(words.as_str())
                            .map(|start| (start, words, picture)),
                        _ => None,
                    })
                    .min_by_key(|(start, _, _)| *start);
                let Some((start, words, picture)) = found else {
                    break;
                };
                if start > 0 {
                    runs.push(Run {
                        text: rest[..start].to_owned(),
                        ..run.clone()
                    });
                }
                let said = Run {
                    text: words.clone(),
                    ..run.clone()
                };
                if picture.after {
                    runs.push(said);
                    runs.push(picture_run(picture));
                } else {
                    runs.push(picture_run(picture));
                    runs.push(said);
                }
                at += start + words.len();
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
            && self.motion == other.motion
            && self.tip == other.tip
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
    /// A fenced code block's key, the hash of its language and text, which the
    /// application asked the plugins with: what says whether the tones in its
    /// runs are still the ones they answer for it.
    #[cfg(feature = "plugins")]
    pub block: Option<std::sync::Arc<str>>,
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
    /// The address of every picture over its text: custom emoji and what
    /// plugins set in.
    pub fn pictures(&self) -> impl Iterator<Item = &str> {
        self.parts
            .iter()
            .flat_map(|part| &part.runs)
            .filter_map(|run| run.emoji.as_ref().map(|(url, _)| url.as_str()))
    }

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
            #[cfg(feature = "plugins")]
            block: None,
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

/// Cuts each fenced code block in `body` into the spans a plugin coloured it
/// with, which `colours` answers for by the block's key: the hash of its
/// language and text, the same one the application asked the plugin with.
///
/// A block nobody coloured, and a set of spans that doesn't spell the block,
/// leave it as typed: what a plugin colours never changes what a message says.
/// The tones are kept on the runs and the theme colours them where the block is
/// drawn, so a body laid out once draws in whatever theme is showing.
#[cfg(feature = "plugins")]
pub fn colour_code(
    body: &mut Body,
    markdown: &str,
    colours: impl Fn(&str) -> Option<crate::code::Spans>,
) {
    let mut fences = yapfast_core::plugins::code_block::fences(markdown).into_iter();
    for part in &mut body.parts {
        if part.shape != Shape::Code {
            continue;
        }
        let text = part_text(part);
        // The next fence that says exactly this: a fence the blocks splitter
        // didn't make a block of is passed over, and no later part can match
        // once they run out.
        let Some((language, _)) = fences.by_ref().find(|(_, fence)| *fence == text) else {
            return;
        };
        let block = yapfast_core::plugins::code_block::hash(language.as_deref(), &text);
        let spans = colours(&block);
        // Kept whether or not a plugin coloured it, so a row that has it can
        // tell later whether what it shows is what the plugins answer now.
        part.block = Some(block.into());
        let Some(spans) = spans else {
            continue;
        };
        if spells(&spans) != text {
            continue;
        }
        part.runs = spans
            .iter()
            .map(|span| Run {
                text: span.text.clone(),
                code: true,
                tone: Some(span.tone),
                ..Run::default()
            })
            .collect();
    }
}

/// Whether every code block in `body` still shows the tones the plugins answer
/// for it now: a block coloured since the text was laid out, one whose colours
/// changed, and one whose colours went away are all laid out again.
#[cfg(feature = "plugins")]
pub fn same_code(body: &Body, colours: impl Fn(&str) -> Option<crate::code::Spans>) -> bool {
    body.parts.iter().all(|part| {
        let Some(block) = part.block.as_deref() else {
            return true;
        };
        match colours(block) {
            None => part.runs.iter().all(|run| run.tone.is_none()),
            // What it shows, spelled out: after colouring, the runs are the
            // spans themselves, so their text is the block's.
            Some(spans) => {
                spans.len() == part.runs.len()
                    && spells(&spans) == part.plain()
                    && spans
                        .iter()
                        .zip(&part.runs)
                        .all(|(span, run)| run.tone == Some(span.tone))
            }
        }
    })
}

/// A part's own text, its runs a line each: what a fence writes between its
/// backticks, and what a set of spans must spell.
#[cfg(feature = "plugins")]
fn part_text(part: &Part) -> String {
    match part.runs.as_slice() {
        [run] => run.text.clone(),
        runs => runs
            .iter()
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// What a set of spans spells, which must be the block exactly.
#[cfg(feature = "plugins")]
fn spells(spans: &[CodeSpan]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

/// The ink a code block's span shows in, by the tone a plugin gave it: the
/// theme's own colours, so a theme colours a plugin's code without knowing a
/// tone, as it does a widget cell.
#[cfg(feature = "plugins")]
fn tone_ink(theme: &crate::theme::Theme, tone: yapfast_plugin_api::code::Tone) -> Color {
    use yapfast_plugin_api::code::Tone;
    match tone {
        Tone::Plain => theme.text,
        Tone::Comment => theme.muted,
        Tone::Keyword => theme.accent,
        Tone::String => theme.live,
        Tone::Number => theme.loud,
        Tone::Function => theme.link,
        // Between a keyword's and a number's, so the two stay apart.
        Tone::Type => crate::theme::mix(theme.accent, theme.loud, 0.5),
        Tone::Variable => theme.secondary,
        Tone::Operator | Tone::Punctuation => theme.faint,
        Tone::Error => theme.danger,
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
            #[cfg(feature = "plugins")]
            block: None,
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
            #[cfg(feature = "plugins")]
            block: None,
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
            #[cfg(feature = "plugins")]
            tone: None,
            motion: None,
            tip: None,
            round: false,
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

thread_local! {
    /// Whether every run drawn now is struck through: a kept deleted row
    /// whose plugin asked for it ([`struck`]).
    static STRUCK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Draws `body` with every run struck through when `on`.
pub fn struck<R>(on: bool, body: impl FnOnce() -> R) -> R {
    if !on {
        return body();
    }
    STRUCK.with(|struck| struck.set(true));
    let drawn = body();
    STRUCK.with(|struck| struck.set(false));
    drawn
}

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
        let mut piece = span(run.text.as_str())
            .font(font)
            .strikethrough(run.strike || STRUCK.with(std::cell::Cell::get));
        if let Some(link) = run.link.as_deref() {
            piece = piece.link(link);
        }
        if run.hidden {
            return piece
                .color(theme.sunken)
                .background(theme.sunken)
                .border(crate::style::round(3));
        }
        #[cfg(feature = "plugins")]
        if let Some(tone) = run.tone {
            // A fenced block's part, in its tone's ink; the block's ground is
            // drawn around the whole of it, not by each span.
            return piece.color(tone_ink(theme, tone));
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
            let mut motions = Vec::new();
            let mut tips = Vec::new();
            let mut rounded = Vec::new();
            let spans: Vec<_> = part
                .runs
                .iter()
                .enumerate()
                .map(|(index, run)| {
                    let mut span = self.run_span(run, ink);
                    if let Some(tip) = &run.tip {
                        tips.push((index, tip.as_str()));
                    }
                    if run.round {
                        rounded.push(index);
                    }
                    // Words a plugin moves are drawn a frame at a time by the
                    // piece, over their room, while pictures play; still, they
                    // show as they are.
                    if let Some(motion) = run.motion
                        && plays
                        && self.plays
                    {
                        motions.push((index, motion, span.color.unwrap_or(ink)));
                        span = span.color(Color::TRANSPARENT);
                    }
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
                .motions(motions)
                .tips(
                    tips,
                    crate::piece::TipLook {
                        ground: theme.tooltip,
                        ink: theme.text,
                    },
                )
                .rounded(rounded)
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

    /// A message's body, with its one fenced block coloured by `spans`.
    #[cfg(feature = "plugins")]
    fn coloured(markdown: &str, spans: Option<Vec<CodeSpan>>) -> Body {
        use std::sync::Arc;
        let block = yapfast_core::plugins::code_block::hash(Some("rust"), "let x = 1;");
        let answer = spans.map(Arc::new);
        let mut body = parse(markdown, &[], 0x00a8fc, "m", |_| false);
        colour_code(&mut body, markdown, |key| {
            answer.clone().filter(|_| key == block)
        });
        body
    }

    /// A colouring, and whether it is still the blocks' answer: a block a
    /// plugin coloured shows its tones, one it stopped colouring goes back to
    /// plain, and a set of spans that doesn't spell the block is refused.
    #[cfg(feature = "plugins")]
    #[test]
    fn a_plugin_colours_a_fenced_block_and_a_set_that_doesnt_spell_it_is_refused() {
        use yapfast_plugin_api::code::{Span, Tone};
        let markdown = "before
```rust
let x = 1;
```
";
        let answer = vec![
            Span::new("let", Tone::Keyword),
            Span::new(" x = 1;", Tone::Plain),
        ];
        let body = coloured(markdown, Some(answer.clone()));
        let block = body
            .parts
            .iter()
            .find(|part| part.shape == Shape::Code)
            .expect("the block");
        assert_eq!(
            block.runs.iter().map(|run| run.tone).collect::<Vec<_>>(),
            [Some(Tone::Keyword), Some(Tone::Plain)]
        );
        assert_eq!(
            block.plain(),
            "let x = 1;",
            "the block still says what it said"
        );
        assert!(same_code(&body, |_| Some(std::sync::Arc::new(
            answer.clone()
        ))));

        // Its colours went: the row is laid out again.
        assert!(!same_code(&body, |_| None));
        // A block nobody coloured, that someone colours now, is laid out again
        // too, which is what keeps a row from drawing plain for ever.
        let plain = coloured(markdown, None);
        assert!(same_code(&plain, |_| None));
        assert!(!same_code(&plain, |_| Some(std::sync::Arc::new(
            answer.clone()
        ))));

        // Spans that don't spell the block are not drawn.
        let short = coloured(markdown, Some(vec![Span::new("let", Tone::Keyword)]));
        let block = short
            .parts
            .iter()
            .find(|part| part.shape == Shape::Code)
            .expect("the block");
        assert!(block.runs.iter().all(|run| run.tone.is_none()));
        assert_eq!(block.plain(), "let x = 1;");
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

    fn decor_marks(marks: Vec<(&str, MarkLook, Option<&str>)>) -> Decor {
        Decor {
            marks: marks
                .into_iter()
                .map(|(text, look, url)| Mark {
                    text: text.to_owned(),
                    look,
                    url: url.map(str::to_owned),
                    tip: Some("why".to_owned()),
                })
                .collect(),
            ..Decor::default()
        }
    }

    #[test]
    fn plugin_spoilers_links_and_motions_lay_over_plain_words() {
        let decor = decor_marks(vec![
            ("secret", MarkLook::Spoiler, None),
            ("docs", MarkLook::Link, Some("https://example.com")),
            ("yay", MarkLook::Wiggle, None),
        ]);
        let mut body = parse("a secret, docs and yay", &[], 0, "m", |_| false);
        mark(&mut body, &decor, "m", |_| false);
        let runs = &body.parts[0].runs;
        let secret = runs.iter().find(|run| run.text == "secret").unwrap();
        assert!(
            secret.hidden && secret.tip.is_none(),
            "covered, and says nothing"
        );
        assert_eq!(
            secret.link.as_deref().and_then(spoiler::target),
            Some(("m", PLUGIN_SPOILERS))
        );
        let docs = runs.iter().find(|run| run.text == "docs").unwrap();
        assert_eq!(
            docs.link.as_deref().and_then(links::masked),
            Some("https://example.com")
        );
        assert_eq!(docs.tip.as_deref(), Some("why"));
        let yay = runs.iter().find(|run| run.text == "yay").unwrap();
        assert_eq!(yay.motion, Some(Motion::Wiggle));
        // Revealed, the words show plain.
        let mut shown = parse("a secret", &[], 0, "m", |_| false);
        mark(&mut shown, &decor, "m", |index| index == PLUGIN_SPOILERS);
        assert!(shown.parts[0].runs.iter().all(|run| !run.hidden));
    }

    #[test]
    fn emoji_are_hidden_or_swapped_and_pictures_set_beside_words_and_mentions() {
        let emoji = [yapfast_core::chat::CustomEmoji {
            text: ":kekw:".to_owned(),
            id: "12".to_owned(),
            picture: "https://cdn.discordapp.com/emojis/12.png".to_owned(),
            animated: false,
            usable: true,
        }];
        let mentions = [Mentioned {
            text: "@Ada".to_owned(),
            target: mention::Target::User("7".to_owned()),
            color: None,
            at: vec![0],
        }];
        let mut body = parse("@Ada :kekw: in France \u{2764}", &mentions, 0, "m", |_| {
            false
        });
        with_emoji(&mut body, &emoji);
        let decor = Decor {
            swaps: vec![
                Swap {
                    emoji: "12".to_owned(),
                    with: Some(SwapWith::Text("lol".to_owned())),
                },
                Swap {
                    emoji: "\u{2764}".to_owned(),
                    with: None,
                },
            ],
            pictures: vec![
                Picture {
                    url: "https://cdn.discordapp.com/avatars/7/a.png".to_owned(),
                    beside: Beside::Link(mention::link(&mention::Target::User("7".to_owned()))),
                    after: false,
                    round: true,
                    tip: None,
                },
                Picture {
                    url: "https://cdn.discordapp.com/emojis/9.webp".to_owned(),
                    beside: Beside::Words("France".to_owned()),
                    after: true,
                    round: false,
                    tip: None,
                },
            ],
            tips: vec![Tip {
                link: mention::link(&mention::Target::User("7".to_owned())),
                text: "Ada Lovelace".to_owned(),
            }],
            ..Decor::default()
        };
        mark(&mut body, &decor, "m", |_| false);
        let runs = &body.parts[0].runs;
        assert!(
            runs[0].round && runs[0].emoji.is_some(),
            "the avatar before the mention"
        );
        assert_eq!(runs[1].tip.as_deref(), Some("Ada Lovelace"));
        let plain = body.plain();
        assert!(
            plain.contains("lol") && !plain.contains('\u{2764}'),
            "{plain}"
        );
        let france = runs.iter().position(|run| run.text == "France").unwrap();
        assert!(runs[france + 1].emoji.is_some(), "the flag after the words");
        assert_eq!(body.pictures().count(), 2);
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
