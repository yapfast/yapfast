//! Making a [`Text`] span by span: what the parser does, and what a view-model
//! does for text that isn't someone's Markdown, as a notice (its author in
//! bold in their colour, opening their card) or an embed's bold title.

use std::sync::Arc;

use iced::widget::image;

use super::{Block, Ink, Inner, Level, Link, Look, Picture, Span, Style, Text};

/// Makes a [`Text`]: spans go into the block begun last, the first begun
/// for them when none was. Spans alike that follow each other join into one.
#[derive(Debug, Default)]
pub struct Builder {
    inner: Inner,
    open: bool,
}

/// What a span pushed carries besides its characters.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dress {
    pub style: Style,
    pub look: Look,
    pub color: Option<Ink>,
    pub link: Option<u32>,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begins a block, which the spans pushed after it fill.
    pub fn block(&mut self, block: Block) -> &mut Self {
        let at = self.inner.spans.len() as u32;
        self.inner.blocks.push(Block {
            first: at,
            last: at,
            ..block
        });
        self.open = true;
        self
    }

    /// Plain characters in a style.
    pub fn text(&mut self, chars: &str, style: Style) -> &mut Self {
        self.push(
            chars,
            Dress {
                style,
                ..Dress::default()
            },
        )
    }

    /// Characters in a colour of their own, as a notice's time is muted.
    pub fn colored(&mut self, chars: &str, style: Style, color: Ink) -> &mut Self {
        self.push(
            chars,
            Dress {
                style,
                color: Some(color),
                ..Dress::default()
            },
        )
    }

    /// Characters that open `link`, in `color` or the link colour. A link to
    /// a person, a role or everyone is a mention, on its pill.
    pub fn link(&mut self, chars: &str, style: Style, link: Link, color: Option<Ink>) -> &mut Self {
        let look = match link {
            Link::User(_) | Link::Role(_) | Link::Everyone => Look::Mention,
            Link::Channel(_) => Look::Channel,
            _ => Look::Text,
        };
        let link = Some(self.add_link(link));
        self.push(
            chars,
            Dress {
                style,
                look,
                color,
                link,
            },
        )
    }

    /// An emoji of Unicode's own, drawn as Twemoji's picture, opening its
    /// card when `card`.
    pub fn emoji(&mut self, emoji: &str, card: bool) -> &mut Self {
        let link = card.then(|| self.add_link(Link::Emoji(emoji.to_owned())));
        self.push(
            emoji,
            Dress {
                look: Look::Emoji,
                link,
                ..Dress::default()
            },
        )
    }

    /// A server's emoji: `still` in an emoji's place, `name` (`:talk:`) its
    /// text when copied, opening its card by `id` when given.
    pub fn picture(&mut self, name: &str, still: image::Handle, id: Option<&str>) -> &mut Self {
        let index = self.add_picture(name, still);
        let link = id.map(|id| self.add_link(Link::CustomEmoji(id.to_owned())));
        self.push(
            name,
            Dress {
                look: Look::Picture(index),
                link,
                ..Dress::default()
            },
        )
    }

    /// A translated sentence in `style`, what `part` pushes standing in each
    /// of its places, given the place's index: `{}` counts on from the place
    /// before, `{1}` names one, and `{{` and `}}` are braces, as
    /// `yapfast_lang::fill` reads them. A notice's sentence with its links in
    /// it, in whatever order the language puts them.
    pub fn template(
        &mut self,
        template: &str,
        style: Style,
        mut part: impl FnMut(&mut Self, usize),
    ) -> &mut Self {
        let mut next = 0;
        let mut rest = template;
        while let Some(at) = rest.find(['{', '}']) {
            self.text(&rest[..at], style);
            let brace = &rest[at..at + 1];
            rest = &rest[at + 1..];
            if let Some(after) = rest.strip_prefix(brace) {
                self.text(brace, style);
                rest = after;
                continue;
            }
            let place = rest
                .split_once('}')
                .filter(|(name, _)| brace == "{" && name.chars().all(|c| c.is_ascii_digit()));
            let Some((name, after)) = place else {
                self.text(brace, style);
                continue;
            };
            rest = after;
            let index = name.parse().unwrap_or_else(|_| {
                next += 1;
                next - 1
            });
            part(self, index);
        }
        self.text(rest, style);
        self
    }

    /// Adds a picture a span may show, and gives its index.
    pub fn add_picture(&mut self, name: &str, still: image::Handle) -> u32 {
        self.inner.pictures.push(Picture {
            name: name.to_owned(),
            still,
        });
        (self.inner.pictures.len() - 1) as u32
    }

    /// Adds a link the spans after may share, and gives its index.
    pub fn add_link(&mut self, link: Link) -> u32 {
        self.inner.links.push(link);
        (self.inner.links.len() - 1) as u32
    }

    /// Replaces the link at `index`, as a masked link's once its text is read.
    pub(super) fn set_link(&mut self, index: u32, link: Link) {
        if let Some(slot) = self.inner.links.get_mut(index as usize) {
            *slot = link;
        }
    }

    /// Pushes characters dressed as `dress`, joining the span before when it
    /// is dressed alike. An emoji is a span of its own each time, Unicode's
    /// as a server's: Twemoji draws a keycap or a flag only as its one glyph.
    pub fn push(&mut self, chars: &str, dress: Dress) -> &mut Self {
        if chars.is_empty() {
            return self;
        }
        if !self.open {
            self.block(Block::default());
        }
        let start = self.inner.chars.len() as u32;
        self.inner.chars.push_str(chars);
        let end = self.inner.chars.len() as u32;
        let block = self.inner.blocks.last_mut().expect("a block is open");
        let joins = block.last > block.first
            && !matches!(dress.look, Look::Picture(_) | Look::Emoji)
            && self.inner.spans.last().is_some_and(|last| {
                last.end == start
                    && last.style == dress.style
                    && last.look == dress.look
                    && last.color == dress.color
                    && last.link == dress.link
            });
        if joins {
            if let Some(last) = self.inner.spans.last_mut() {
                last.end = end;
            }
        } else {
            self.inner.spans.push(Span {
                start,
                end,
                style: dress.style,
                look: dress.look,
                color: dress.color,
                link: dress.link,
            });
            block.last += 1;
        }
        self
    }

    pub(super) fn set_jumbo(&mut self, jumbo: bool) {
        self.inner.jumbo = jumbo;
    }

    /// The text made. A block left empty stays, as an empty line.
    pub fn finish(mut self) -> Text {
        self.inner.chars.shrink_to_fit();
        self.inner.spans.shrink_to_fit();
        self.inner.blocks.shrink_to_fit();
        Text(Arc::new(self.inner))
    }
}

impl Text {
    /// Characters in one style, in one block.
    pub fn styled(chars: &str, style: Style) -> Self {
        let mut builder = Builder::new();
        builder.text(chars, style);
        builder.finish()
    }

    /// Characters in bold, as typed: an embed's author, a field's name.
    pub fn bold(chars: &str) -> Self {
        Self::styled(
            chars,
            Style {
                bold: true,
                ..Style::default()
            },
        )
    }
}

impl Block {
    /// A block of text at its level, with no space above it.
    pub fn new(level: Level) -> Self {
        Self {
            level,
            ..Self::default()
        }
    }
}
