//! What [`markdown`](super::markdown) reads: someone's Markdown and what the
//! core knows of it, as plain data the app fills from the core's
//! `ChatMessage`: its mentions (`yapfast_core::mention::Mentioned`), custom
//! emoji (`yapfast_core::chat::CustomEmoji` with the picture the app holds),
//! which spoilers were revealed, and its blocks
//! (`yapfast_core::blocks::split`, Discord's rules, which stay the core's).

use std::borrow::Cow;
use std::ops::Range;

use iced::Color;
use iced::widget::image;

use super::Link;

/// Someone's Markdown and what goes with it.
#[derive(Clone, Copy)]
pub struct Source<'a> {
    pub markdown: &'a str,
    pub mentions: &'a [Mention],
    pub emoji: &'a [CustomEmoji],
    pub spoilers: Spoilers<'a>,
    /// Its blocks when it has headings, subtext, quotes or fenced code, as
    /// the core splits them; `None` for text alone.
    pub sections: Option<&'a [Section]>,
    /// Emoji open their cards, as a message's do; an embed's don't.
    pub cards: bool,
}

/// What becomes of spoilers, `||like this||`.
#[derive(Clone, Copy)]
pub enum Spoilers<'a> {
    /// Shown as typed, bars and all, as an embed shows them.
    AsTyped,
    /// Covered, but those this says were revealed, by their index counted
    /// through the message.
    Covered(&'a dyn Fn(usize) -> bool),
    /// Each replaced by "(spoiler)", as a pinned or found message's are.
    Hidden,
}

/// A mention in the Markdown, where the message's tag for it was.
#[derive(Clone, Debug, PartialEq)]
pub struct Mention {
    /// Its text in the Markdown, as `@Bob`; a channel's comes escaped.
    pub text: String,
    /// [`Link::User`], [`Link::Role`], [`Link::Channel`] or [`Link::Everyone`].
    pub target: Link,
    /// The role's colour, or the person's name's; else the theme's.
    pub color: Option<Color>,
    /// Where its text starts in the Markdown, a byte offset each time.
    pub at: Vec<usize>,
}

/// One of the message's custom emoji, named in its Markdown as `:talk:`.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomEmoji {
    /// Its name between colons, as the Markdown has it.
    pub text: String,
    pub id: String,
    /// Its picture once it arrived; until then its name shows as typed.
    pub still: Option<image::Handle>,
}

/// A block of the Markdown as the core splits it (`yapfast_core::blocks::Block`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Section {
    pub kind: Kind,
    pub quoted: bool,
    /// Where its text is in the Markdown, without its markers: one run, or
    /// one a line of a quote of `> ` lines, which join with line breaks.
    pub runs: Vec<Range<usize>>,
}

/// What a section is (`yapfast_core::blocks::Kind`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Text,
    /// `#` to `###`, 1 to 3.
    Heading(u8),
    Subtext,
    Code,
}

impl<'a> Source<'a> {
    /// `markdown` alone: no mentions, emoji or spoilers, emoji opening no
    /// cards, as an embed's description is.
    pub fn new(markdown: &'a str) -> Self {
        Self {
            markdown,
            mentions: &[],
            emoji: &[],
            spoilers: Spoilers::AsTyped,
            sections: None,
            cards: false,
        }
    }
}

impl Section {
    /// Its text: a heading's or subtext's without its marker, code's without
    /// its fences (`Block::text` in the core).
    pub fn text<'a>(&self, markdown: &'a str) -> Cow<'a, str> {
        let run = |run: &Range<usize>| markdown.get(run.clone()).unwrap_or_default();
        match self.runs.as_slice() {
            [] => "".into(),
            [only] => run(only).into(),
            runs => runs.iter().map(run).collect::<Vec<_>>().join("\n").into(),
        }
    }

    /// Where `at`, a byte offset in the Markdown, is in [`Section::text`],
    /// when the `len` bytes from it are all in one of its runs.
    pub fn offset(&self, at: usize, len: usize) -> Option<usize> {
        let mut start = 0;
        for run in &self.runs {
            if run.start <= at && at + len <= run.end {
                return Some(start + at - run.start);
            }
            start += run.len() + 1;
        }
        None
    }
}
