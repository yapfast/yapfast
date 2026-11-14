//! The conversation and its timeline: messages as virtualized rows, each an
//! author line (unless it continues the one before), its text as styled
//! runs, picture previews (GIFs playing), and reaction chips with who
//! reacted in their tooltips.
//!
//! Rows are laid out at heights worked out up front ([`row_height`]), so
//! `widgets::Rows` builds only those near the viewport. The scrollable is
//! anchored to the bottom: it keeps the distance from the end, so it
//! follows new messages at the end and rows stay put as history loads
//! above; rows added below while someone reads earlier ones are made up for
//! in `sync`.

use std::time::Duration;

use iced::{
    Alignment, Color, Element, Font, Length, Padding, Point, Rectangle, Size, Vector,
    font::Style as FontStyle,
    widget::{Space, column, container, mouse_area, row, scrollable, span, text, tooltip},
};
use yapfast_core::chat::{self, Attachment, ChatMessage};

use crate::{
    app::{App, Message, key},
    fonts::{BOLD, ITALIC, MEDIUM, REGULAR, SEMIBOLD, shown_name, style_font},
    header::{self, tip_shadow},
    looks::{self, Chip},
    pictures::Face,
    rail,
    select::Piece,
    sidebar,
    style::{LINE, MUTED, SELECTED, SELECTION, TEXT, TOOLTIP, fill, hex, rounded, scrollbar},
    widgets::{Animated, Pic, RightPress, Rows, Watch},
};

/// The timeline's scrollable, for the app's scroll tasks.
pub const TIMELINE: &str = "timeline";
/// Rows built beyond each edge of a list's viewport.
pub const MARGIN: f32 = 300.0;
/// How long the pictures rows want wait to be asked for together, as the
/// Slint client's `PictureStore` batches them.
pub const ASK_EVERY: Duration = Duration::from_millis(50);

/// Discord's numbers: 16 px body text on 22 px lines, 40 px avatars in a
/// 72 px gutter, rows padded 2 px apart and 17 px above a new group.
pub const AVATAR: f32 = 40.0;
pub const LINE_HEIGHT: f32 = 22.0;
pub const FONT_SIZE: f32 = 16.0;
pub const GUTTER: f32 = 72.0;
pub const GROUP_GAP: f32 = 17.0;
pub const ROW_PAD: f32 = 2.0;
/// The body font's average advance at 16 px, for wrapping estimates.
const CHAR: f32 = 7.6;

/// Whether a message continues the one before it, as Discord groups them:
/// the same author within seven minutes, and no reply.
pub fn continues(messages: &[std::sync::Arc<ChatMessage>], index: usize) -> bool {
    let Some(previous) = index.checked_sub(1).and_then(|before| messages.get(before)) else {
        return false;
    };
    let message = &messages[index];
    message.reply.is_none()
        && previous.sender == message.sender
        && message.timestamp.saturating_sub(previous.timestamp) < 7 * 60 * 1000
}

/// The size a picture attachment's preview shows at: its picture's, else
/// the size the sender gave, fitted to the preview box as the core decodes
/// to it. None for other files.
pub fn preview_size(attachment: &Attachment) -> Option<(f32, f32)> {
    let (width, height) = attachment
        .preview
        .as_ref()
        .map(|picture| (picture.width, picture.height))
        .or_else(|| {
            attachment.dimensions.map(|(width, height)| {
                let scale = f64::min(
                    1.0,
                    f64::min(
                        f64::from(chat::PREVIEW_WIDTH) / f64::from(width.max(1)),
                        f64::from(chat::PREVIEW_HEIGHT) / f64::from(height.max(1)),
                    ),
                );
                (
                    (f64::from(width) * scale) as u32,
                    (f64::from(height) * scale) as u32,
                )
            })
        })?;
    attachment.image.then_some((width as f32, height as f32))
}

/// A message row's height: the lines its text wraps to, estimated by
/// character count at the timeline's width, plus its pictures and
/// reactions.
pub fn row_height(message: &ChatMessage, continuation: bool, width: f32) -> f32 {
    let text_width = (width - GUTTER - 16.0).max(100.0);
    let per_line = (text_width / CHAR).floor().max(1.0) as usize;
    let lines: usize = if message.body.is_empty() {
        0
    } else {
        message
            .body
            .lines()
            .map(|line| line.chars().count().div_ceil(per_line).max(1))
            .sum()
    };
    let mut height = ROW_PAD * 2.0 + lines as f32 * LINE_HEIGHT;
    if !continuation {
        height += GROUP_GAP + LINE_HEIGHT;
    }
    for attachment in &message.attachments {
        height += match preview_size(attachment) {
            Some((_, picture_height)) => picture_height + 8.0,
            None => LINE_HEIGHT + 8.0,
        };
    }
    if !message.reactions.is_empty() {
        height += 30.0;
    }
    if continuation {
        height
    } else {
        height.max(ROW_PAD * 2.0 + GROUP_GAP + AVATAR)
    }
}

/// A run of message text in one style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

/// A message's text in styled runs: `**bold**`, `*italic*` or `_italic_`,
/// `` `code` ``, and `- ` list lines as bullets. The fixture's Markdown,
/// not all of Discord's, which M1 brings.
pub fn runs(text: &str) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let (mut bold, mut italic, mut code) = (false, false, false);
    let mut current = String::new();
    let flush = |runs: &mut Vec<Run>, current: &mut String, bold, italic, code| {
        if !current.is_empty() {
            runs.push(Run {
                text: std::mem::take(current),
                bold,
                italic,
                code,
            });
        }
    };
    for (number, line) in text.split('\n').enumerate() {
        if number > 0 {
            current.push('\n');
        }
        let line = match line.strip_prefix("- ") {
            Some(rest) => {
                current.push_str("• ");
                rest
            }
            None => line,
        };
        let mut chars = line.chars().peekable();
        while let Some(character) = chars.next() {
            match character {
                '`' => {
                    flush(&mut runs, &mut current, bold, italic, code);
                    code = !code;
                }
                '*' if !code && chars.peek() == Some(&'*') => {
                    chars.next();
                    flush(&mut runs, &mut current, bold, italic, code);
                    bold = !bold;
                }
                '*' | '_' if !code => {
                    flush(&mut runs, &mut current, bold, italic, code);
                    italic = !italic;
                }
                other => current.push(other),
            }
        }
    }
    flush(&mut runs, &mut current, bold, italic, code);
    runs
}

/// A run's font: monospace for code, else DM Sans in its weight and slant.
pub fn run_font(run: &Run) -> Font {
    if run.code {
        Font::MONOSPACE
    } else if run.bold {
        Font {
            style: if run.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            ..BOLD
        }
    } else if run.italic {
        ITALIC
    } else {
        REGULAR
    }
}

/// A message's text as it shows: its runs' text, Markdown taken out.
pub fn body_text(message: &ChatMessage) -> String {
    runs(&message.body)
        .into_iter()
        .map(|run| run.text)
        .collect()
}

/// Code's colour in a message.
const CODE: u32 = 0xebc88c;
/// A reaction chip's ground when this account reacted.
const MINE: u32 = 0x3c4270;

impl App {
    /// The conversation column: the header, the timeline and the composer,
    /// with a server's member list beside them; or a voice channel's call.
    pub fn conversation(&self) -> Element<'_, Message> {
        if self.voice_selected() {
            return self.call_view();
        }
        let line = container(Space::new())
            .width(Length::Fill)
            .height(1)
            .style(|_| fill(LINE));
        let chat = column![self.timeline(), self.composer_view()]
            .width(Length::Fill)
            .height(Length::Fill);
        let body = if self.server && !self.hide_members {
            row![chat, self.members_view()]
        } else {
            row![chat]
        };
        column![self.header_view(), line, body.height(Length::Fill)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// The conversation column's right edge: the member list's left.
    pub fn chat_right(&self) -> f32 {
        self.window.width
            - if self.server && !self.hide_members {
                crate::members::WIDTH
            } else {
                0.0
            }
    }

    fn timeline(&self) -> Element<'_, Message> {
        let (first, last) = self.in_view(MARGIN);
        let mut rows = Rows::new(self.total_height());
        for index in first..last {
            let row = self.message_row(index);
            rows.push(
                key(&self.messages[index].item),
                self.tops[index],
                self.heights[index],
                row,
            );
        }
        let list = scrollable(rows)
            .id(TIMELINE)
            .anchor_bottom()
            .on_scroll(Message::Timeline)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(scrollbar);
        // A press on no text clears the selection; letting go ends a drag.
        Watch::new(
            list,
            self.selection.is_some().then_some(Message::SelectClear),
            self.selecting.then_some(Message::SelectEnd),
        )
        .into()
    }

    /// A message's author line, as the Slint client's with its Discord
    /// look: the name in its style's font (only the font, in a server) and
    /// role colour, the server tag chip, APP for a bot, the time.
    fn author_line<'a>(&'a self, index: usize, message: &'a ChatMessage) -> Element<'a, Message> {
        let cosmetics = self.client.state().cosmetics;
        let style = looks::name_look(
            cosmetics.style(message.style.as_deref()),
            &message.author,
            self.in_server,
        );
        let color = message.color.map_or(hex(TEXT), hex);
        // Out of a server a whole style shows; its first colour stands in
        // for the effect in a timeline row.
        let color = match &style {
            Some(style) if !self.in_server => style.colors.first().map_or(color, |rgb| hex(*rgb)),
            _ => color,
        };
        let name = Piece::new(
            vec![
                span(shown_name(&message.author, style.as_ref()).into_owned())
                    .font(style_font(style.as_ref(), 500)),
            ],
            index,
            0,
        )
        .color(color)
        .starts(false)
        .selection(self.selected(index, 0), self.selecting, SELECTION);
        let mut line = row![mouse_area(name).on_press(Message::OpenAuthor(index))]
            .spacing(8)
            .align_y(Alignment::Center);
        if let Some(tag) = cosmetics.tag(message.tag.as_ref()) {
            let size = FONT_SIZE * 0.72;
            line = line.push(self.tag_chip(
                &tag.text,
                tag.badge.as_deref(),
                Chip {
                    size,
                    font: SEMIBOLD,
                    height: size * 1.3333,
                    pad: 4.0,
                    radius: 4.0,
                    fill: Color::from_rgba8(0x97, 0x97, 0x9f, 0.2),
                    ink: hex(TEXT),
                },
                1.0,
            ));
        }
        if message.bot {
            line = line.push(self.bot_tag(1.0));
        }
        let time = Piece::new(vec![span(message.time.as_str())], index, 1)
            .size(12.0, 16.0)
            .color(hex(MUTED))
            .selection(self.selected(index, 1), self.selecting, SELECTION);
        line.push(time).height(LINE_HEIGHT).into()
    }

    fn message_row(&self, index: usize) -> Element<'_, Message> {
        let message = &self.messages[index];
        let continuation = continues(&self.messages, index);
        let mut content = column![].spacing(4).width(Length::Fill);
        if !continuation {
            content = content.push(self.author_line(index, message));
        }
        if !message.body.is_empty() {
            let spans: Vec<_> = runs(&message.body)
                .into_iter()
                .map(|run| {
                    let font = run_font(&run);
                    let code = run.code;
                    let span = span(run.text).font(font);
                    if code { span.color(hex(CODE)) } else { span }
                })
                .collect();
            content = content.push(
                Piece::new(spans, index, 2)
                    .size(FONT_SIZE, LINE_HEIGHT)
                    .color(hex(TEXT))
                    .fill()
                    .selection(self.selected(index, 2), self.selecting, SELECTION),
            );
        }
        for attachment in &message.attachments {
            content = content.push(self.attachment(attachment));
        }
        if !message.reactions.is_empty() {
            content = content.push(self.reactions(message));
        }
        let gutter = container(if continuation {
            Element::from(Space::new())
        } else {
            let decoration = self
                .client
                .state()
                .cosmetics
                .decoration(message.decoration.as_deref());
            let face = self.face(Face {
                url: message.avatar.as_deref(),
                name: &message.author,
                size: AVATAR,
                opacity: 1.0,
                decoration,
                play: self.pointed_message.as_ref() == Some(&message.item),
                dot: None,
            });
            mouse_area(face).on_press(Message::OpenAuthor(index)).into()
        })
        .width(GUTTER)
        .align_x(Alignment::Center);
        let top = if continuation {
            ROW_PAD
        } else {
            ROW_PAD + GROUP_GAP
        };
        let body = container(row![gutter, content])
            .width(Length::Fill)
            .height(self.heights[index])
            .padding(Padding {
                top,
                right: 16.0,
                bottom: ROW_PAD,
                left: 0.0,
            })
            .clip(true);
        // A right click opens the message's menu where the pointer is.
        let origin = Vector::new(rail::WIDTH + sidebar::WIDTH, header::HEIGHT + 1.0);
        let body = RightPress::new(body, move |at| Message::RowMenu(index, at + origin));
        // Only a message with an author line has anything to play.
        if continuation {
            return body.into();
        }
        mouse_area(body)
            .on_enter(Message::PointRow(message.item.clone(), true))
            .on_exit(Message::PointRow(message.item.clone(), false))
            .into()
    }

    /// An attachment: a picture's preview with round corners (a GIF
    /// playing), its room kept while it decodes; else the file's name.
    fn attachment<'a>(&'a self, attachment: &'a Attachment) -> Element<'a, Message> {
        let Some((width, height)) = preview_size(attachment) else {
            return text(format!("📎 {}", attachment.name))
                .color(hex(MUTED))
                .into();
        };
        let size = Size::new(width, height);
        let inner: Element<'_, Message> = match attachment.preview.as_ref() {
            Some(picture) if picture.animation.is_some() => {
                let animated = Animated::new(
                    picture.clone(),
                    size,
                    Rectangle::with_size(size),
                    false,
                    &self.playing,
                )
                .radius(8.0);
                self.moving.borrow_mut().insert(animated.key());
                animated.into()
            }
            Some(picture) if picture.width > 0 && picture.height > 0 => {
                let handle = self
                    .previews
                    .borrow_mut()
                    .get(picture, crate::pictures::shared);
                Pic::new(handle, size).radius(8.0).into()
            }
            _ => Space::new().width(width).height(height).into(),
        };
        container(inner)
            .width(width)
            .height(height)
            .style(|_| rounded(hex(SELECTED), 8))
            .into()
    }

    /// A message's reaction chips. Each has its tooltip from the start, so
    /// it knows the pointer is on the chip when the names come: empty until
    /// then.
    fn reactions<'a>(&'a self, message: &'a ChatMessage) -> Element<'a, Message> {
        let mut chips = row![].spacing(4);
        for reaction in &message.reactions {
            let ground = hex(if reaction.mine { MINE } else { SELECTED });
            let chip = container(text(format!("{} {}", reaction.key, reaction.count)))
                .padding([2, 8])
                .style(move |_| rounded(ground, 8));
            let chip = mouse_area(chip)
                .on_enter(Message::PointReaction(
                    message.item.clone(),
                    reaction.key.clone(),
                    true,
                ))
                .on_exit(Message::PointReaction(
                    message.item.clone(),
                    reaction.key.clone(),
                    false,
                ));
            let words = self
                .reaction_tip(&message.item, &reaction.key)
                .unwrap_or_else(|| Space::new().width(0).height(0).into());
            chips = chips.push(
                tooltip(chip, words, tooltip::Position::Top)
                    .gap(6)
                    .delay(Duration::from_millis(500))
                    .style(|_| container::Style::default()),
            );
        }
        chips.into()
    }

    /// A reaction's tooltip, once the core has the names, for the chip the
    /// pointer stays on.
    fn reaction_tip(&self, item: &str, key: &str) -> Option<Element<'_, Message>> {
        let (tip_of, reaction) = self.client.state().reaction_tip()?;
        if tip_of.item != item || tip_of.key != key {
            return None;
        }
        let names = tip_of.names.as_deref().unwrap_or_default();
        let words = reaction_words(&emoji_shortcode(key), names, reaction.count);
        let picture = emojis::get(key)
            .map(emojis::Emoji::as_str)
            .and_then(|key| self.emoji_picture(key, 32));
        let face: Element<'_, Message> = match picture {
            Some(picture) => Pic::new(picture, Size::new(32.0, 32.0)).into(),
            None => text(key.to_owned()).size(26).into(),
        };
        Some(
            container(
                row![
                    face,
                    text(words)
                        .size(14)
                        .font(MEDIUM)
                        .color(hex(TEXT))
                        .width(Length::Shrink)
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .padding(16)
            .max_width(288)
            .style(|_| container::Style {
                shadow: tip_shadow(),
                ..rounded(hex(TOOLTIP), 6)
            })
            .into(),
        )
    }

    /// A message's avatar or author was clicked: the card opens beside the
    /// avatar, as the Slint client's `beside`.
    pub fn open_author(&mut self, index: usize) -> iced::Task<Message> {
        let Some(message) = self.messages.get(index) else {
            return iced::Task::none();
        };
        let visible_top = (self.total_height() - self.timeline_height - self.from_end).max(0.0);
        let y = header::HEIGHT + 1.0 + self.tops[index] - visible_top + ROW_PAD + GROUP_GAP;
        let x = rail::WIDTH + sidebar::WIDTH + (GUTTER - AVATAR) / 2.0;
        let width = crate::profile::WIDTH;
        let beside = if x + AVATAR + 8.0 + width <= self.window.width - 8.0 {
            x + AVATAR + 8.0
        } else {
            (x - width - 8.0).max(8.0)
        };
        let user = message.sender.clone();
        self.open_card(user, Point::new(beside, y - 8.0))
    }
}

/// What a reaction's tooltip says, as the Slint client's `reaction_tip`
/// words it after Discord's client.
pub fn reaction_words(emoji: &str, names: &[String], count: usize) -> String {
    let others = count.saturating_sub(names.len());
    let more = match others {
        1 => "1 other".to_owned(),
        others => format!("{others} others"),
    };
    let people = match (names, others) {
        ([one], 0) => one.clone(),
        ([one], _) => format!("{one} and {more}"),
        ([one, two], 0) => format!("{one} and {two}"),
        ([one, two], _) => format!("{one}, {two}, and {more}"),
        ([one, two, three], 0) => format!("{one}, {two}, and {three}"),
        ([one, two, three, ..], _) => format!("{one}, {two}, {three}, and {more}"),
        _ => format!("{count} people"),
    };
    format!("{emoji} reacted by {people}")
}

/// A reaction's emoji as Discord's client names it: `:name:` for a custom
/// one, the first shortcode that's a word for a Unicode one.
pub fn emoji_shortcode(key: &str) -> String {
    if key.starts_with(':') && key.ends_with(':') && key.len() > 2 {
        return key.to_owned();
    }
    let codes = || {
        emojis::get(key)
            .into_iter()
            .flat_map(emojis::Emoji::shortcodes)
    };
    codes()
        .find(|code| code.starts_with(|c: char| c.is_ascii_alphanumeric()))
        .or_else(|| codes().next())
        .map_or_else(|| key.to_owned(), |code| format!(":{code}:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reactions_tooltip_names_up_to_three_then_counts_the_others() {
        let names = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            reaction_words(":tada:", &names(&["Alice"]), 1),
            ":tada: reacted by Alice"
        );
        assert_eq!(
            reaction_words(":tada:", &names(&["Alice", "Bob", "Carol"]), 4),
            ":tada: reacted by Alice, Bob, Carol, and 1 other"
        );
        assert_eq!(emoji_shortcode("🎉"), ":tada:");
    }

    #[test]
    fn markdown_runs_carry_their_style_and_bullets() {
        let runs = runs("**bold** and `code`\n- item");
        assert_eq!(runs[0].text, "bold");
        assert!(runs[0].bold);
        assert!(runs.iter().any(|run| run.code && run.text == "code"));
        assert!(runs.last().unwrap().text.ends_with("• item"));
    }
}
