//! Markdown's inline parts as spans: what Slint's Markdown drew of a
//! message (`parse_interpolated` in `i-slint-common`'s `styled_text.rs`),
//! with what Yapfast's patches and rewrites added to it. Bold, italic,
//! strikethrough, `__underline__` (Discord's), inline code, links and
//! lists; each paragraph, item and line break a line of its own; a list's
//! bullets `•`, `◦` and `▪` by depth, four spaces a level; mentions and
//! hidden spoilers from their markers; a masked link asking before it
//! leaves; emoji outside code and links as Twemoji's, a server's as its
//! picture, a `:shortcode:` as its emoji. Anything else Slint's Markdown
//! refused, so a block with it shows as typed instead ([`supported`]).

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};

use super::build::{Builder, Dress};
use super::emoji::{custom_at, emoji_at, shortcode_at};
use super::links::{openable, shows};
use super::prepare::{Marker, Markers};
use super::source::Source;
use super::{Ink, Link, Look, Style};

fn parser(markdown: &str) -> Parser<'_> {
    Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH)
}

/// Whether every part of `markdown` is one this draws; Slint's Markdown
/// refused the rest (headings, quotes, code blocks, pictures, HTML, rules).
pub fn supported(markdown: &str) -> bool {
    parser(markdown).all(|event| match event {
        Event::Start(tag) => matches!(
            tag,
            Tag::Paragraph
                | Tag::Item
                | Tag::List(_)
                | Tag::Strong
                | Tag::Emphasis
                | Tag::Strikethrough
                | Tag::Link { .. }
        ),
        Event::End(tag) => matches!(
            tag,
            TagEnd::Paragraph
                | TagEnd::Item
                | TagEnd::List(_)
                | TagEnd::Strong
                | TagEnd::Emphasis
                | TagEnd::Strikethrough
                | TagEnd::Link
        ),
        Event::Text(_) | Event::Code(_) | Event::SoftBreak | Event::HardBreak => true,
        _ => false,
    })
}

/// A style or link open around what comes next.
#[derive(Clone, Copy, PartialEq)]
enum Open {
    Bold,
    Italic,
    Underline,
    Strike,
    Link,
}

/// A link open around what comes next.
struct OpenLink {
    index: u32,
    look: Look,
    /// A mention's own colour, its role's or its person's.
    color: Option<Ink>,
    /// Its destination and the text read so far, while it may be masked.
    masked: Option<(String, String)>,
}

/// Puts the spans of `markdown`, made ready by `prepare`, into `builder`'s
/// open block. Call it only on what [`supported`] accepts.
pub fn spans(markdown: &str, source: &Source<'_>, markers: &Markers, builder: &mut Builder) {
    let mut walk = Walk {
        source,
        markers,
        builder,
        open: Vec::new(),
        link: None,
        lists: Vec::new(),
        started: false,
    };
    for (event, range) in parser(markdown).into_offset_iter() {
        let depth = walk.lists.len().saturating_sub(1);
        match event {
            Event::SoftBreak | Event::HardBreak | Event::Start(Tag::Paragraph) => {
                walk.line(depth, None)
            }
            Event::Start(Tag::Item) => {
                let bullet = match walk.lists.last_mut() {
                    Some(Some(number)) => {
                        *number += 1;
                        format!("{}. ", *number - 1)
                    }
                    _ => ["• ", "◦ ", "▪ "][depth % 3].to_owned(),
                };
                walk.line(depth, Some(&bullet));
            }
            Event::Start(Tag::List(first)) => walk.lists.push(first),
            Event::End(TagEnd::List(_)) => {
                walk.lists.pop();
            }
            Event::Start(Tag::Strong) if markdown[range.start..].starts_with("__") => {
                walk.open.push(Open::Underline)
            }
            Event::Start(Tag::Strong) => walk.open.push(Open::Bold),
            Event::Start(Tag::Emphasis) => walk.open.push(Open::Italic),
            Event::Start(Tag::Strikethrough) => walk.open.push(Open::Strike),
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => walk.open_link(&dest_url, link_type, &markdown[range]),
            Event::End(TagEnd::Link) => walk.close_link(),
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough) => {
                walk.open.pop();
            }
            Event::Text(text) => walk.text(&text, false),
            Event::Code(text) => walk.text(&text, true),
            _ => {}
        }
    }
}

struct Walk<'a, 'b> {
    source: &'a Source<'b>,
    markers: &'a Markers,
    builder: &'a mut Builder,
    open: Vec<Open>,
    link: Option<OpenLink>,
    /// Each list's next number, or none for bullets.
    lists: Vec<Option<u64>>,
    /// A line was begun, so the next begins after a line break.
    started: bool,
}

impl Walk<'_, '_> {
    /// Begins a line, indented four spaces a level of list and after its
    /// bullet, if any, neither of which takes the styles open around it.
    fn line(&mut self, depth: usize, bullet: Option<&str>) {
        let plain = Style::default();
        if self.started {
            self.builder.text("\n", plain);
        }
        self.started = true;
        for _ in 0..depth {
            self.builder.text("    ", plain);
        }
        if let Some(bullet) = bullet {
            self.builder.text(bullet, plain);
        }
    }

    fn style(&self, code: bool) -> Style {
        let mut style = Style {
            code,
            ..Style::default()
        };
        for open in &self.open {
            match open {
                Open::Bold => style.bold = true,
                Open::Italic => style.italic = true,
                Open::Underline => style.underline = true,
                Open::Strike => style.strike = true,
                Open::Link => {}
            }
        }
        style
    }

    /// Opens the link to `dest` whose Markdown is `source`: a mention or a
    /// spoiler when it's a marker, for a mention the source has.
    fn open_link(&mut self, dest: &str, link_type: LinkType, source: &str) {
        let mut color = None;
        let marker = self.markers.read(dest, source);
        let mention = match marker {
            Some(Marker::Mention(index)) => self.source.mentions.get(index),
            _ => None,
        };
        let (link, look, masked) = match (marker, mention) {
            (_, Some(mention)) => {
                color = mention.color.map(Ink::Own);
                let look = match mention.target {
                    Link::Channel(_) => Look::Channel,
                    _ => Look::Mention,
                };
                (mention.target.clone(), look, None)
            }
            (Some(Marker::Spoiler(index)), _) => (Link::Spoiler(index), Look::Spoiler, None),
            _ => {
                let maybe_masked = source.starts_with('[')
                    && !matches!(link_type, LinkType::Autolink | LinkType::Email)
                    && openable(dest);
                let masked = maybe_masked.then(|| (dest.to_owned(), String::new()));
                (Link::Url(dest.to_owned()), Look::Text, masked)
            }
        };
        let index = self.builder.add_link(link);
        self.link = Some(OpenLink {
            index,
            look,
            color,
            masked,
        });
        self.open.push(Open::Link);
    }

    fn close_link(&mut self) {
        if let Some(position) = self.open.iter().rposition(|open| *open == Open::Link) {
            self.open.remove(position);
        }
        let Some(link) = self.link.take() else {
            return;
        };
        if let Some((dest, text)) = link.masked
            && !shows(&text, &dest)
        {
            self.builder.set_link(link.index, Link::Leave(dest));
        }
    }

    fn text(&mut self, text: &str, code: bool) {
        let style = self.style(code);
        if let Some(link) = self.link.as_mut() {
            if let Some((_, read)) = link.masked.as_mut() {
                read.push_str(text);
            }
            let dress = Dress {
                style,
                look: link.look,
                color: link.color,
                link: Some(link.index),
            };
            self.builder.push(text, dress);
            return;
        }
        if code {
            self.builder.text(text, style);
            return;
        }
        self.emoji(text, style);
    }

    /// Pushes `text` with its emoji apart: a server's as its picture,
    /// Unicode's as Twemoji's, a shortcode as its emoji.
    fn emoji(&mut self, text: &str, style: Style) {
        let cards = self.source.cards;
        let mut copied = 0;
        let mut at = 0;
        while let Some(next) = text[at..].chars().next() {
            let rest = &text[at..];
            let found = if next == ':' {
                custom_at(rest, self.source.emoji)
                    .map(|custom| (custom.text.len(), Found::Custom(custom)))
                    .or_else(|| {
                        shortcode_at(rest).map(|(length, emoji)| (length, Found::Unicode(emoji)))
                    })
            } else {
                match emoji_at(rest) {
                    0 => None,
                    length => Some((length, Found::Unicode(&rest[..length]))),
                }
            };
            let Some((length, found)) = found else {
                at += next.len_utf8();
                continue;
            };
            self.builder.text(&text[copied..at], style);
            match found {
                Found::Unicode(emoji) => {
                    self.builder.emoji(emoji, cards);
                }
                Found::Custom(custom) => {
                    if let Some(still) = &custom.still {
                        let id = cards.then_some(custom.id.as_str());
                        self.builder.picture(&custom.text, still.clone(), id);
                    }
                }
            }
            at += length;
            copied = at;
        }
        self.builder.text(&text[copied..], style);
    }
}

/// An emoji found in text.
enum Found<'a> {
    Unicode(&'a str),
    Custom(&'a super::source::CustomEmoji),
}
