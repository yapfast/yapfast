//! Text as spans in blocks: what a message's body, an embed's description
//! or a notice shows, already parsed, which the message paragraph
//! (`anomi_widgets::paragraph`) draws. A span is a run of characters in one
//! style (bold, italic, underline, strike, inline code), a look (plain, a
//! mention on its pill, a hidden spoiler under its cover, an emoji drawn as
//! Twemoji's picture, a server's emoji as its own picture) and the link it
//! opens. A block is a run of spans laid out as Discord's client lays out
//! Markdown: text, a heading, subtext or fenced code, behind a quote's bar
//! when quoted, with the space above it.
//!
//! [`markdown`] parses someone's Discord Markdown into it, with its
//! mentions, custom emoji and spoilers ([`Source`]), as `src/ui/messages.rs`
//! made Slint's styled text (`markdown:207`, `with_mentions:215`,
//! `blocks_of`, `body_blocks`, and `src/emoji_text.rs`,
//! `src/custom_emoji.rs`); [`plain`] links the web addresses of text that
//! isn't Markdown; [`Builder`] makes any other, as a notice's.
//!
//! A [`Text`] is shared: cloning it clones a pointer, and two equal texts
//! compare without reading their characters when they are one. Ports the
//! `TextBlock` struct (`ui/messages.slint:586`). Task F08.

mod build;
mod emoji;
mod inline;
mod links;
mod parse;
mod prepare;
mod source;
mod spoiler;
#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;

use iced::Color;
use iced::widget::image;

pub use build::{Builder, Dress};
pub use links::openable;
pub use parse::{markdown, plain};
pub use source::{CustomEmoji, Kind, Mention, Section, Source, Spoilers};

/// Text as spans in blocks. Cheap to clone; see the module's doc.
#[derive(Clone, Default)]
pub struct Text(Arc<Inner>);

#[derive(Debug, Default, PartialEq)]
struct Inner {
    /// Every span's characters, one after another.
    chars: String,
    spans: Vec<Span>,
    blocks: Vec<Block>,
    links: Vec<Link>,
    pictures: Vec<Picture>,
    jumbo: bool,
}

/// A run of characters drawn alike.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    /// Where its characters are in the text's.
    start: u32,
    end: u32,
    pub style: Style,
    pub look: Look,
    /// A colour of its own, as a role's on a mention or a notice's author;
    /// otherwise its block's, or the link colour on a link.
    pub color: Option<Ink>,
    /// The link it opens, an index into the text's links; the spans of one
    /// link share it, so it underlines whole under the pointer.
    pub link: Option<u32>,
}

/// How a span's characters are set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Inline code: the system's monospace font, a little smaller, on a capsule.
    pub code: bool,
}

/// A colour a span takes: one of its own, as a role's, or one of the
/// theme's, which follows the theme as it changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ink {
    Own(Color),
    /// The theme's text colour, as a notice's links to the message and to
    /// the pins take.
    Text,
    Secondary,
    /// As a notice's time.
    Muted,
}

/// What a span is besides its characters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Look {
    #[default]
    Text,
    /// A mention of a person, a role or everyone: in its colour on a
    /// translucent pill of it, never underlined.
    Mention,
    /// A mentioned channel: in a mention's colour, without the pill.
    Channel,
    /// A spoiler still hidden: an opaque cover over it, which a click on
    /// its link lifts.
    Spoiler,
    /// An emoji of Unicode's own, drawn as Twemoji's picture.
    Emoji,
    /// A server's emoji: the picture at this index of [`Text::picture`],
    /// in the place an emoji takes.
    Picture(u32),
}

/// A block of spans, as `TextBlock` has it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Block {
    pub level: Level,
    /// Behind a quote's bar, whose ends are rounded at the blocks that
    /// start and end it.
    pub quoted: bool,
    pub quote_start: bool,
    pub quote_end: bool,
    /// The space above it, outside any bar.
    pub margin: f32,
    /// The space above it inside a bar.
    pub padding: f32,
    /// Its spans, a range of the text's.
    first: u32,
    last: u32,
}

/// What a block is, as Discord's client lays it out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Level {
    #[default]
    Text,
    /// `#` to `###`: 1 to 3, at 1.5, 1.25 and 1 times the text's size, bold.
    Heading(u8),
    /// `-# `: smaller and dimmer.
    Subtext,
    /// Fenced code: as typed, in the system's monospace font, in its box.
    Code,
}

/// What a click on a span opens.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Link {
    /// A web address, or whatever the Markdown linked to; the app opens
    /// only what [`openable`] allows.
    Url(String),
    /// A link whose text shows another address than where it goes: a click
    /// asks before leaving for it (`links::LEAVE` in the core).
    Leave(String),
    /// A mentioned person, by id.
    User(String),
    /// A mentioned role, by id.
    Role(String),
    /// A mentioned channel, by room id.
    Channel(String),
    /// `@everyone` and `@here`.
    Everyone,
    /// The card of an emoji of Unicode's own, by the emoji.
    Emoji(String),
    /// The card of a server's emoji, by its id.
    CustomEmoji(String),
    /// The message's spoiler at this index, counted through the message.
    Spoiler(usize),
    /// A notice's author, by sender, which opens their card.
    Author(String),
    /// The message a notice tells of, by item.
    Jump(String),
    /// The open channel's pinned messages.
    Pins,
}

/// A server's emoji in text: its still, which plays when it moves
/// (`anomi_widgets::picture`), and its name between colons.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub name: String,
    pub still: image::Handle,
}

impl Text {
    /// Whether it shows nothing.
    pub fn is_empty(&self) -> bool {
        self.0.spans.is_empty()
    }

    /// A message of emoji alone, up to 30, which Discord shows large.
    pub fn jumbo(&self) -> bool {
        self.0.jumbo
    }

    /// Its blocks in order, each with its spans.
    pub fn blocks(&self) -> impl ExactSizeIterator<Item = (&Block, &[Span])> + '_ {
        self.0.blocks.iter().map(|block| {
            let spans = &self.0.spans[block.first as usize..block.last as usize];
            (block, spans)
        })
    }

    /// The characters of one of its spans.
    pub fn chars(&self, span: &Span) -> &str {
        self.0
            .chars
            .get(span.start as usize..span.end as usize)
            .unwrap_or_default()
    }

    /// The link at an index of [`Span::link`].
    pub fn link(&self, index: u32) -> Option<&Link> {
        self.0.links.get(index as usize)
    }

    /// The picture at an index of [`Look::Picture`].
    pub fn picture(&self, index: u32) -> Option<&Picture> {
        self.0.pictures.get(index as usize)
    }

    /// Every picture it shows.
    pub fn pictures(&self) -> &[Picture] {
        &self.0.pictures
    }

    /// Whether `other` is this very text, not only an equal one.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Its characters as read: blocks on lines of their own, a server's
    /// emoji by its name, a hidden spoiler as `(spoiler)`. For a tooltip, a
    /// notification or a screen reader.
    pub fn read(&self) -> String {
        let mut out = String::with_capacity(self.0.chars.len());
        for (index, (_, spans)) in self.blocks().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            let mut hidden = None;
            for span in spans {
                match span.look {
                    Look::Spoiler if hidden == span.link => continue,
                    Look::Spoiler => out.push_str("(spoiler)"),
                    Look::Picture(index) => match self.picture(index) {
                        Some(picture) => out.push_str(&picture.name),
                        None => out.push_str(self.chars(span)),
                    },
                    _ => out.push_str(self.chars(span)),
                }
                hidden = (span.look == Look::Spoiler).then_some(span.link).flatten();
            }
        }
        out
    }
}

impl PartialEq for Text {
    fn eq(&self, other: &Self) -> bool {
        self.same(other) || self.0 == other.0
    }
}

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for (block, spans) in self.blocks() {
            let spans: Vec<(&str, &Span)> =
                spans.iter().map(|span| (self.chars(span), span)).collect();
            list.entry(&(block.level, block.quoted, spans));
        }
        list.finish()
    }
}

impl Span {
    /// Whether the pointer on it shows a hand, and a click opens something.
    pub fn opens(&self) -> bool {
        self.link.is_some()
    }

    /// Whether a link drawn in it is underlined under the pointer: not a
    /// mention, whose pill marks it, nor a picture, which a line would cross,
    /// nor a covered spoiler.
    pub fn underlines(&self) -> bool {
        self.link.is_some() && matches!(self.look, Look::Text | Look::Channel)
    }
}

impl Block {
    /// How many spans it has.
    pub fn len(&self) -> usize {
        (self.last - self.first) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.first == self.last
    }

    /// Its text's size as a share of the paragraph's: `TextBlockView`'s
    /// `default-font-size` (`ui/messages.slint:2080`); `framed` text, inside
    /// a card's container, is smaller, as in Discord's client.
    pub fn scale(&self, framed: bool) -> f32 {
        match self.level {
            Level::Heading(1) => 1.5,
            Level::Heading(2) => 1.25,
            Level::Heading(_) => 1.0,
            Level::Subtext if framed => 0.75,
            Level::Subtext => 0.875,
            Level::Code => 0.875,
            Level::Text if framed => 0.875,
            Level::Text => 1.0,
        }
    }
}
