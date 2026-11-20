//! What each row is before it's drawn: its look (grouping, the dividers
//! above it, its time, its body laid out), the history's edge over the
//! first, and the height a row is placed at until it's measured.

use std::sync::Arc;

use yapfast_core::{
    application::{AppState, Navigation},
    chat::{self, ChatMessage, Placement},
    network::Network,
};

use crate::{
    markdown::{self, Body},
    media::Gallery,
};

use super::{
    AUTHOR_GAP, AUTHOR_LINE, AVATAR, CHAR, CONTENT_PAD, Clock, DIVIDED_GAP, DIVIDER, EDGE, EMBED,
    EMBED_ROOM, END_PAD, FILE_CARD, GROUP_GAP, GUTTER, INTRO_CHANNEL, INTRO_DIRECT, INTRO_SERVER,
    LINE_HEIGHT, ROW_GAP, TAIL, THREAD_CHIP,
};

/// Whether `message` continues the group of the one `before` it, as
/// Discord groups them: the same person within seven minutes (timestamps
/// are seconds), not replying or answering a command, and no notice.
pub fn continues(before: &ChatMessage, message: &ChatMessage) -> bool {
    if before.notice.is_some() || message.notice.is_some() {
        return false;
    }
    fn sender(message: &ChatMessage) -> &str {
        if message.sender.is_empty() {
            &message.author
        } else {
            &message.sender
        }
    }
    let close = before.timestamp == 0
        || message.timestamp == 0
        || message.timestamp.abs_diff(before.timestamp) <= 7 * 60;
    message.reply.is_none() && message.used.is_none() && sender(before) == sender(message) && close
}

/// What a row shows beyond its message: whether it starts a group, a new
/// day's date, the "New" divider, the marigold tint, whether it's the
/// last, and its text laid out.
#[derive(Clone, Debug)]
pub struct RowLook {
    /// Its message's item, shared, so what a click on the row sends names
    /// the message whatever place it holds by the time it's handled.
    pub item: Arc<str>,
    pub first: bool,
    pub day: Option<String>,
    pub unread_start: bool,
    pub highlighted: bool,
    pub last: bool,
    pub body: Arc<Laid>,
    /// Its message's local date, worked out once.
    pub date: Option<chrono::NaiveDate>,
    /// When it was sent, on the clock Messages' settings chose, shared
    /// with the look it was worked out for.
    pub time: Arc<str>,
}

/// A message's text laid out: its body, which it derefs to, and its
/// embeds' Markdown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Laid {
    pub body: Body,
    pub embeds: Vec<EmbedText>,
    /// Each of its apps' cards' text rows.
    pub cards: Vec<Vec<Body>>,
    /// The reply quote's single-line snippet, whitespace collapsed.
    pub reply: String,
}

impl std::ops::Deref for Laid {
    type Target = Body;

    fn deref(&self) -> &Body {
        &self.body
    }
}

/// An embed's Markdown: its description and each field's name and value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmbedText {
    pub description: Body,
    pub fields: Vec<(Body, Body)>,
}

impl RowLook {
    /// Whether it's laid out as `other` is, so a height measured for one
    /// fits the other.
    pub fn same_place(&self, other: &RowLook) -> bool {
        self.first == other.first
            && self.day == other.day
            && self.unread_start == other.unread_start
            && self.last == other.last
            && Arc::ptr_eq(&self.body, &other.body)
    }

    pub(super) fn divided(&self) -> bool {
        self.day.is_some() || self.unread_start
    }
}

/// What a timeline's rows are marked by: the last message read (those
/// after it that call on this account are tinted), and the first unread,
/// over which the "New" divider shows.
#[derive(Clone, Copy, Debug, Default)]
pub struct Marks<'a> {
    pub seen: Option<&'a str>,
    pub first_unread: Option<&'a str>,
}

/// The rows' looks: a message's text laid out and its date are those of
/// its `old` look, or made (`body`).
pub fn row_looks<'a>(
    messages: &[Arc<ChatMessage>],
    marks: Marks<'_>,
    clock: Clock,
    old: impl Fn(usize) -> Option<&'a RowLook>,
    mut body: impl FnMut(usize) -> Arc<Laid>,
) -> Vec<RowLook> {
    let first_new = chat::first_new(messages, marks.seen);
    let now = yapfast_core::clock::now_ms() / 1000;
    let mut before_date = None;
    messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let before = index.checked_sub(1).map(|before| &*messages[before]);
            let (item, laid, date, time) = match old(index) {
                Some(look) => (
                    look.item.clone(),
                    look.body.clone(),
                    look.date,
                    look.time.clone(),
                ),
                None => (
                    Arc::from(message.item.as_str()),
                    body(index),
                    yapfast_core::clock::date(message.timestamp),
                    clock.sent(message, now).into(),
                ),
            };
            let new_day = date.is_some() && (before.is_none() || before_date != date);
            before_date = date;
            // The author shows again below the "New" divider.
            let unread_start = marks.first_unread == Some(message.item.as_str());
            RowLook {
                item,
                first: new_day
                    || unread_start
                    || before.is_none_or(|before| !continues(before, message)),
                day: new_day.then(|| yapfast_core::clock::day(message.timestamp)),
                unread_start,
                highlighted: message.highlighted(index >= first_new),
                last: index + 1 == messages.len(),
                body: laid,
                date,
                time,
            }
        })
        .collect()
}

/// Whether [`body_of`] lays two forms of a message out the same: its text,
/// mentions, emoji, embeds' and cards' text. A notice's words name people,
/// so a notice is the same only when it's equal.
pub fn same_text(a: &ChatMessage, b: &ChatMessage) -> bool {
    if a.notice.is_some() || b.notice.is_some() {
        return a == b;
    }
    a.item == b.item
        && a.formatted == b.formatted
        && a.body == b.body
        && a.mentions == b.mentions
        && a.emoji == b.emoji
        && a.reply.as_ref().map(|reply| reply.body.as_str())
            == b.reply.as_ref().map(|reply| reply.body.as_str())
        && a.cards == b.cards
        && a.embeds.len() == b.embeds.len()
        && a.embeds
            .iter()
            .zip(&b.embeds)
            .all(|(a, b)| a.description == b.description && a.fields == b.fields)
}

/// A message's text laid out: a notice's words, plain text as typed, or
/// its Markdown with mentions in `mention` (`0xrrggbb`) and its spoilers
/// covered but those `revealed`; and its embeds' Markdown.
pub fn body_of(
    message: &ChatMessage,
    mention: u32,
    text: u32,
    muted: u32,
    revealed: impl Fn(usize) -> bool,
) -> Laid {
    let body = if let Some(notice) = &message.notice {
        markdown::notice(message, notice, text, muted)
    } else if !message.formatted {
        markdown::plain(&message.body)
    } else {
        let mut body = markdown::parse(
            &message.body,
            &message.mentions,
            mention,
            &message.item,
            &revealed,
        );
        markdown::with_emoji(&mut body, &message.emoji);
        markdown::link_emoji(&mut body, &message.emoji, crate::picker::drawn);
        body
    };
    let laid = |markdown: &str| markdown::parse(markdown, &[], mention, &message.item, |_| true);
    let embeds = message
        .embeds
        .iter()
        .map(|embed| EmbedText {
            description: laid(&embed.description),
            fields: embed
                .fields
                .iter()
                .map(|field| (laid(&field.name), laid(&field.value)))
                .collect(),
        })
        .collect();
    let cards = message
        .cards
        .iter()
        .map(|card| {
            crate::components::laid(card, |text| {
                let mut body = markdown::parse(text, &[], mention, &message.item, |_| true);
                markdown::with_emoji(&mut body, &message.emoji);
                body
            })
        })
        .collect();
    let reply = message
        .reply
        .as_ref()
        .map(|reply| reply.body.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    Laid {
        body,
        embeds,
        cards,
        reply,
    }
}

/// What the history's edge shows, at the timeline's first row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edge {
    pub ready: bool,
    pub loading: bool,
    pub failed: bool,
    pub start: bool,
    /// The timeline is a window of older messages: newer ones follow its
    /// last row, loading or failed to.
    pub detached: bool,
    pub loading_later: bool,
    pub later_failed: bool,
    pub direct: bool,
    pub thread: bool,
    pub name: String,
    pub intro: Intro,
}

/// Discord's welcome at the start of a server channel's history.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Intro {
    #[default]
    None,
    /// The server's first text channel: welcome to the server.
    Server(String),
    Channel,
    /// A one-to-one DM's: the other person, with what is shared.
    Direct,
}

impl Edge {
    /// The open room's, with its welcome: a server's for its first
    /// channel, else the channel's own.
    pub fn of(state: &AppState) -> Self {
        let room = state.selected_room();
        let intro = match (room, &state.navigation) {
            (Some(room), Navigation::Server(id))
                if room.network == Network::Discord
                    && !room.direct
                    && !room.voice
                    && room.parent.is_none() =>
            {
                let server = state.servers.iter().find(|server| &server.id == id);
                let first = server.is_some_and(|server| {
                    server
                        .channels
                        .iter()
                        .filter_map(|channel| state.rooms.iter().find(|room| &room.id == channel))
                        .find(|room| !room.voice && room.parent.is_none() && !room.forum)
                        .is_some_and(|first| first.id == room.id)
                });
                match server {
                    Some(server) if first => Intro::Server(server.name.clone()),
                    _ => Intro::Channel,
                }
            }
            // A one-to-one DM opens with the other person.
            (Some(room), _)
                if room.network == Network::Discord
                    && room.direct
                    && room.person.is_some()
                    && room.capabilities.dm_calls =>
            {
                Intro::Direct
            }
            _ => Intro::None,
        };
        Edge {
            ready: state.room_ready,
            loading: state.loading_history,
            failed: state.history_failed,
            start: state.history_start,
            detached: state.detached,
            loading_later: state.loading_later,
            later_failed: state.later_failed,
            direct: room.is_some_and(|room| room.direct),
            thread: room.is_some_and(|room| room.parent.is_some()),
            name: room.map(|room| room.name.clone()).unwrap_or_default(),
            intro,
        }
    }

    pub(super) fn welcome(&self) -> bool {
        self.start && self.intro != Intro::None
    }

    /// The height under the last row's message: Newer messages, while the
    /// timeline is a window of older ones.
    pub fn foot(&self) -> f32 {
        if self.ready && self.detached {
            EDGE
        } else {
            0.0
        }
    }

    /// Its height above the first row's message.
    pub fn height(&self) -> f32 {
        match &self.intro {
            _ if !self.ready => 0.0,
            Intro::Server(_) if self.start => INTRO_SERVER,
            Intro::Channel if self.start => INTRO_CHANNEL,
            Intro::Direct if self.start => INTRO_DIRECT,
            _ => EDGE,
        }
    }
}

/// A row's height before it's measured: the lines its text wraps to,
/// estimated by character count at the timeline's width, its pictures and
/// reactions, and the space and dividers above it.
pub fn row_height(message: &ChatMessage, look: &RowLook, width: f32, edge: f32) -> f32 {
    let text_width = (width - GUTTER - END_PAD).max(100.0);
    let per_line = (text_width / CHAR).floor().max(1.0) as usize;
    let text = look.body.plain();
    let lines: usize = if text.is_empty() {
        0
    } else {
        text.split('\n')
            .map(|line| line.chars().count().div_ceil(per_line).max(1))
            .sum()
    };
    let mut content = lines as f32 * LINE_HEIGHT;
    if look.body.jumbo {
        content *= 2.2;
    }
    let header = message.reply.is_some() || message.used.is_some();
    if header {
        content += LINE_HEIGHT;
    }
    if look.first && message.notice.is_none() {
        content += AUTHOR_LINE + AUTHOR_GAP;
    }
    let gallery = Gallery::of(&message.attachments, Placement::Message, text_width);
    if !gallery.is_empty() {
        content += gallery.size.height + 4.0;
    }
    if message
        .attachments
        .iter()
        .any(|attachment| attachment.placement == Placement::Sticker)
    {
        content += crate::message::STICKER + 4.0;
    }
    let files = crate::message::files(message, Placement::Message).count();
    content += files as f32 * (FILE_CARD + 4.0);
    if files > 0 && message.voice.is_some() {
        content += crate::playback::VOICE_CARD - FILE_CARD - 4.0;
    }
    if let Some(poll) = &message.poll {
        content += crate::polls::estimate(poll);
    }
    content += message
        .cards
        .iter()
        .map(crate::components::estimate)
        .sum::<f32>();
    for place in 0..message.embeds.len() {
        let gallery = Gallery::of(&message.attachments, Placement::Embed(place), EMBED_ROOM);
        content += EMBED
            + if gallery.is_empty() {
                0.0
            } else {
                gallery.size.height + 8.0
            };
    }
    if !message.reactions.is_empty() {
        // The chips, and the 4 above them.
        content += 34.0;
    }
    if message.thread.is_some() {
        content += THREAD_CHIP;
    }
    if look.first && message.notice.is_none() {
        content = content.max(if header { 64.0 } else { AVATAR });
    }
    // Whole pixels, as rows are measured, so rows sit on whole pixels.
    (top_space(look) + edge + content + CONTENT_PAD * 2.0 + bottom_space(look)).round()
}

/// The space above a row's message, its dividers' included.
pub(super) fn top_space(look: &RowLook) -> f32 {
    if look.divided() {
        DIVIDED_GAP + DIVIDER
    } else if look.first {
        GROUP_GAP
    } else {
        ROW_GAP
    }
}

pub(super) fn bottom_space(look: &RowLook) -> f32 {
    if look.last { TAIL } else { ROW_GAP }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(sender: &str, timestamp: u64) -> ChatMessage {
        ChatMessage {
            sender: sender.into(),
            author: sender.into(),
            timestamp,
            ..ChatMessage::default()
        }
    }

    #[test]
    fn a_group_holds_one_persons_messages_for_seven_minutes() {
        let first = message("ann", 1_000_000);
        assert!(continues(&first, &message("ann", 1_000_000 + 6 * 60)));
        assert!(!continues(&first, &message("ann", 1_000_000 + 8 * 60)));
        assert!(!continues(&first, &message("bob", 1_000_010)));
    }
}
