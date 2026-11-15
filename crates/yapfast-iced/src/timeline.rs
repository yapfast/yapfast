//! The conversation and its timeline, as the Slint client's: messages as
//! virtualized rows grouped as Discord groups them, a new day's date and
//! the "New" divider above the first unread message, the history's edge
//! (earlier messages loading, or the conversation's start and Discord's
//! welcome), a mention's marigold tint, and the bars that jump to the
//! first unread message and back to the present. One message's content is
//! `message.rs`'s.
//!
//! Rows are placed at heights worked out ahead ([`row_height`]) until
//! they're built: then `widgets::Rows` measures them and the app keeps the
//! real heights. The scrollable is anchored to the bottom: it keeps the
//! distance from the end, so it follows new messages at the end and rows
//! stay put as history loads or rows grow above; rows that grow or come
//! below while someone reads earlier ones are made up for here.

use std::{sync::Arc, time::Duration};

use iced::{
    Alignment, Color, Element, Length, Padding, Task,
    widget::{
        Space, button, column, container, mouse_area, operation, row, scrollable,
        scrollable::AbsoluteOffset, stack, text,
    },
};
use yapfast_core::{
    application::{Action, AppState, Navigation},
    chat::{self, ChatMessage, Placement},
    network::Network,
};

use crate::{
    app::{App, Message, Messages, key},
    controls::bar,
    fonts::{BOLD, MEDIUM, SEMIBOLD},
    markdown::{self, Body},
    media::Gallery,
    pictures::{Face, Icon},
    style::{rounded, scrollbar},
    theme::rgb,
    widgets::{Corner, RightPress, Rows, Watch},
};

/// The timeline's scrollable, for the app's scroll tasks.
pub const TIMELINE: &str = "timeline";
/// Rows built beyond each edge of a list's viewport.
pub const MARGIN: f32 = 300.0;
/// How long the pictures rows want wait to be asked for together, as the
/// Slint client's `PictureStore` batches them.
pub const ASK_EVERY: Duration = Duration::from_millis(50);
/// How long a message jumped to stays tinted.
const JUMP_LIT: Duration = Duration::from_secs(2);

/// The Slint client's Discord look: 15 px text on lines 1.3 times that,
/// the author's line 20 px and 2 more under it, 40 px avatars in a 72 px
/// gutter (16 px either side), 48 px kept right of the text for the row's
/// actions.
pub const AVATAR: f32 = 40.0;

pub const LINE_HEIGHT: f32 = 19.5;
pub const AUTHOR_LINE: f32 = 20.0;
pub const AUTHOR_GAP: f32 = 2.0;
pub const GUTTER: f32 = 72.0;
pub const END_PAD: f32 = 48.0;
/// The space above a group's first message, a date or "New" divider's
/// row, and other messages.
pub const GROUP_GAP: f32 = 16.0;
const DIVIDED_GAP: f32 = 12.0;
const ROW_GAP: f32 = 1.0;
/// The divider's line and what's on it, and the space under the last row.
const DIVIDER: f32 = 26.0;
const TAIL: f32 = 20.0;
/// A message's content is padded 2 px above and below.
pub const CONTENT_PAD: f32 = 2.0;
/// The history's edge: its button's line, Discord's welcome blocks.
pub const EDGE: f32 = 42.0;
const INTRO_SERVER: f32 = 167.0;
const INTRO_CHANNEL: f32 = 203.0;
/// The jump bars, as the Slint client's `UnreadBar` and `PresentBar`.
const UNREAD_BAR: f32 = 32.0;
const PRESENT_BAR: f32 = 36.0;
/// A message that calls on this account: marigold, as `ui/theme.slint`'s
/// `mentioned` and `mentioned-hover`, with a 2 px edge.
const MENTIONED: Color = Color::from_rgba8(0xf8, 0xa5, 0x00, 0x1f as f32 / 255.0);
const MENTIONED_HOVER: Color = Color::from_rgba8(0xf8, 0xa5, 0x00, 0x29 as f32 / 255.0);
const MENTION_EDGE: f32 = 2.0;
/// A file's card's height, and an embed's text, for estimates.
const FILE_CARD: f32 = 58.0;
const EMBED: f32 = 90.0;
/// An embed's pictures' room, for estimates.
const EMBED_ROOM: f32 = 400.0;
/// A message's thread's chip, for estimates.
const THREAD_CHIP: f32 = 44.0;
/// The body font's average advance at 15 px, for wrapping estimates.
const CHAR: f32 = 7.1;

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
    pub first: bool,
    pub day: Option<String>,
    pub unread_start: bool,
    pub highlighted: bool,
    pub last: bool,
    pub body: Arc<Laid>,
    /// Its message's local date, worked out once.
    pub date: Option<chrono::NaiveDate>,
    /// When it was sent, on the clock Messages' settings chose.
    pub time: String,
}

/// The clock message times are shown on: 12-hour, and with seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    pub twelve_hour: bool,
    pub seconds: bool,
}

impl Clock {
    /// When a message was sent, as its author line says it: the core's own
    /// words for one without a time.
    pub fn sent(self, message: &ChatMessage, now: u64) -> String {
        if message.timestamp == 0 {
            return message.time.clone();
        }
        yapfast_core::clock::sent(message.timestamp, now, self.twelve_hour, self.seconds)
    }
}

/// A message's text laid out: its body, which it derefs to, and its
/// embeds' Markdown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Laid {
    pub body: Body,
    pub embeds: Vec<EmbedText>,
    /// Each of its apps' cards' text rows.
    pub cards: Vec<Vec<Body>>,
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

    fn divided(&self) -> bool {
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
            let (laid, date, time) = match old(index) {
                Some(look) => (look.body.clone(), look.date, look.time.clone()),
                None => (
                    body(index),
                    yapfast_core::clock::date(message.timestamp),
                    clock.sent(message, now),
                ),
            };
            let new_day = date.is_some() && (before.is_none() || before_date != date);
            before_date = date;
            // The author shows again below the "New" divider.
            let unread_start = marks.first_unread == Some(message.item.as_str());
            RowLook {
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
            crate::cards::laid(card, |text| {
                let mut body = markdown::parse(text, &[], mention, &message.item, |_| true);
                markdown::with_emoji(&mut body, &message.emoji);
                body
            })
        })
        .collect();
    Laid {
        body,
        embeds,
        cards,
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
}

impl Edge {
    /// The open room's, as the Slint client's `show_header` works out the
    /// welcome: a server's for its first channel, else the channel's own.
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

    fn welcome(&self) -> bool {
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
        .map(crate::cards::estimate)
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
        content += 30.0;
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
fn top_space(look: &RowLook) -> f32 {
    if look.divided() {
        DIVIDED_GAP + DIVIDER
    } else if look.first {
        GROUP_GAP
    } else {
        ROW_GAP
    }
}

fn bottom_space(look: &RowLook) -> f32 {
    if look.last { TAIL } else { ROW_GAP }
}

/// The rows of the thread beside the channel are numbered from here, so a
/// row's number says which timeline holds it: the channel's below, the
/// thread's from here up.
pub const ASIDE: usize = 1 << 24;

/// Where a timeline is in the window: its left edge, its width, and where
/// its rows' first would start in the window's height.
#[derive(Clone, Copy, Debug)]
pub struct LanePlace {
    pub left: f32,
    pub width: f32,
    pub top: f32,
}

/// What a link in the timeline's text leads to.
pub const AUTHOR_LINK: &str = "yapfast:author/";
pub const JUMP_LINK: &str = "yapfast:jump/";
pub const PINS_LINK: &str = "yapfast:pins";

impl App {
    /// Row `row`'s message, in the channel's timeline or the thread's
    /// beside it ([`ASIDE`]).
    pub fn row_message(&self, row: usize) -> Option<&Arc<ChatMessage>> {
        match row.checked_sub(ASIDE) {
            Some(index) => self.aside.as_ref()?.messages.get(index),
            None => self.messages.get(row),
        }
    }

    /// Row `row`'s look, as [`App::row_message`] finds its message.
    pub fn row_look(&self, row: usize) -> Option<&RowLook> {
        match row.checked_sub(ASIDE) {
            Some(index) => self.aside.as_ref()?.looks.get(index),
            None => self.looks.get(row),
        }
    }

    /// Where row `row`'s timeline is in the window, and its top among the
    /// rows, less the view's.
    pub fn lane_place(&self, row: usize) -> (LanePlace, f32) {
        match (row.checked_sub(ASIDE), self.aside.as_ref()) {
            (Some(index), Some(aside)) => (
                LanePlace {
                    left: self.window.width - aside.width,
                    width: aside.width,
                    top: crate::header::HEIGHT + 1.0 + crate::thread::HEAD + 1.0,
                },
                aside.tops.get(index).copied().unwrap_or(0.0) - aside.view_top(),
            ),
            _ => (
                LanePlace {
                    left: crate::rail::WIDTH + crate::sidebar::WIDTH,
                    width: self.width,
                    top: crate::header::HEIGHT + 1.0,
                },
                self.tops.get(row).copied().unwrap_or(0.0) - self.view_top(),
            ),
        }
    }

    /// The preview's note, Matrix's verification and the core's notice,
    /// as the Slint client stacks them over the conversation or the call.
    pub fn banners(&self) -> impl Iterator<Item = Element<'_, Message>> {
        [
            self.preview_banner(),
            self.verify_banner(),
            self.notice_banner(),
        ]
        .into_iter()
        .flatten()
    }

    /// The conversation column: the header, the timeline and the composer,
    /// with a server's member list beside them; or a voice channel's call.
    pub fn conversation(&self) -> Element<'_, Message> {
        if self.voice_panel() {
            return self.call_view();
        }
        let outline = self.theme.outline_chat;
        let line = bar(outline, Length::Fill, 1);
        if let Some(screen) = self.dm_screen() {
            let width = self.window.width - crate::rail::WIDTH - crate::sidebar::WIDTH;
            let page = match screen {
                yapfast_core::application::DmScreen::Friends => self.friends_screen(width),
                yapfast_core::application::DmScreen::Requests => self.requests_screen(),
            };
            return column![self.friends_header(screen), line, page]
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }
        if let Some(screen) = self.browse_channels() {
            return screen;
        }
        let mut chat = column(self.banners())
            .width(Length::Fill)
            .height(Length::Fill);
        // A DM's call shows above its messages.
        if let Some(call) = self.dm_call_view() {
            chat = chat.push(call);
        }
        let chat = match self.forum_view() {
            Some(forum) => chat.push(forum),
            None => chat.push(self.timeline()).push(self.composer_view()),
        };
        let mut body = row![chat];
        if let Some(search) = self.search_view() {
            body = body.push(search);
        } else if let Some(aside) = self.aside_view() {
            body = body.push(aside);
        } else if self.members_shown() {
            body = body.push(self.members_view());
        } else if self.panel_shown()
            && let Some(panel) = self.profile_panel_view()
        {
            body = body.push(panel);
        }
        column![self.header_view(), line, body.height(Length::Fill)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// The conversation column's right edge: the member list's left.
    pub fn chat_right(&self) -> f32 {
        self.window.width
            - if self.search_shown() {
                crate::search::WIDTH
            } else if self.aside_shown() {
                self.aside_width()
            } else if self.members_shown() {
                crate::members::WIDTH
            } else if self.panel_shown() {
                crate::profile::PANEL_WIDTH
            } else {
                0.0
            }
    }

    fn timeline(&self) -> Element<'_, Message> {
        if self.messages.is_empty() {
            return self.conversation_start();
        }
        let (first, last) = self.in_view(MARGIN);
        // While someone reads earlier ones, the row at the view's top: it
        // and those after it keep their tops as they're measured.
        let pivot = (!self.at_end).then(|| {
            let top = self.view_top();
            (first..last)
                .position(|index| self.tops[index + 1] > top)
                .unwrap_or(last - first)
        });
        let mut rows =
            Rows::new(self.total_height()).measured(Message::Measured, self.found.clone(), pivot);
        // A short history is held down, its newest message over the composer.
        let lead = (self.timeline_height - self.total_height()).max(0.0);
        for index in first..last {
            rows.push(
                key(&self.messages[index].item),
                self.tops[index],
                self.heights[index],
                self.timeline_row(index),
            );
        }
        let list = scrollable(column![Space::new().height(lead), rows])
            .id(TIMELINE)
            .anchor_bottom()
            .on_scroll(Message::Timeline)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(scrollbar(&self.theme));
        // A press on no text clears the selection; letting go ends a drag.
        let list = Watch::new(
            list,
            self.selection.is_some().then_some(Message::SelectClear),
            self.selecting.then_some(Message::SelectEnd),
        );
        let mut layers = stack![list].width(Length::Fill).height(Length::Fill);
        if let Some(typing) = self.typing_view() {
            layers = layers.push(typing);
        }
        if let Some(bar) = self.unread_bar() {
            layers = layers.push(
                container(bar)
                    .width(Length::Fill)
                    .padding(Padding::ZERO.top(12))
                    .align_x(Alignment::Center),
            );
        }
        if self.client.state().detached {
            layers = layers.push(
                container(self.present_bar())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding([8, 16])
                    .align_y(Alignment::End),
            );
        }
        layers.into()
    }

    /// One row: the history's edge above the first, a divider above a new
    /// day or the first unread message, then the message on its tint.
    pub fn timeline_row(&self, index: usize) -> Element<'_, Message> {
        let (Some(message), Some(look)) = (self.row_message(index), self.row_look(index)) else {
            return Space::new().into();
        };
        let theme = &self.theme;
        let aside = index >= ASIDE;
        let edge = if aside {
            index == ASIDE && self.aside.as_ref().is_some_and(|aside| aside.ready)
        } else {
            index == 0 && self.edge.ready
        };
        let head = edge || look.divided();
        let mut whole = column![].width(Length::Fill);
        if head {
            let mut above = column![].width(Length::Fill);
            if edge && aside {
                above = above.push(self.aside_edge());
            } else if edge {
                above = above.push(self.history_edge());
            }
            if look.divided() {
                above = above.push(self.divider(look));
            }
            whole = whole.push(container(above).padding(
                Padding::ZERO.top(top_space(look) - if look.divided() { DIVIDER } else { 0.0 }),
            ));
        }
        let pointed = self.pointed.message.as_deref() == Some(message.item.as_str());
        let jumped = self.jump_lit.as_deref() == Some(message.item.as_str());
        let edited = self.editing.as_deref() == Some(message.item.as_str());
        let ground = if edited {
            // Discord's edit tint (#29292d on its dark theme).
            Some(crate::theme::mix(theme.active, theme.chat, 0.79))
        } else if jumped {
            Some(Color {
                a: 0.14,
                ..theme.accent
            })
        } else if look.highlighted {
            Some(if pointed { MENTIONED_HOVER } else { MENTIONED })
        } else if pointed {
            Some(theme.row_hover)
        } else {
            None
        };
        let content = container(self.message_content(index, look))
            .width(Length::Fill)
            .padding(Padding {
                top: if head { 0.0 } else { top_space(look) },
                bottom: ROW_GAP,
                ..Padding::ZERO
            })
            .style(move |_| container::Style {
                background: ground.map(Into::into),
                ..container::Style::default()
            });
        let mut content = stack![content];
        if look.highlighted {
            content = content.push(bar(theme.accent, MENTION_EDGE, Length::Fill));
        }
        // The hover bar floats over the row's top edge, so it never moves
        // the timeline.
        if pointed && let Some(actions) = self.hover_bar(index) {
            let body = if head { 0.0 } else { top_space(look) };
            content = content.push(Corner::new(actions, 16.0, body - 16.0));
        }
        // A right click opens the message's menu where the pointer is.
        let (lane, _) = self.lane_place(index);
        let origin = iced::Vector::new(lane.left, lane.top);
        let content = RightPress::new(content, move |at| Message::RowMenu(index, at + origin));
        whole = whole.push(
            mouse_area(content)
                .on_enter(Message::PointRow(message.item.clone(), true))
                .on_exit(Message::PointRow(message.item.clone(), false)),
        );
        if look.last {
            whole = whole.push(Space::new().height(TAIL - ROW_GAP));
        }
        // A window of older messages ends with what's after it: newer
        // messages loading, or a way to load them.
        if !aside && index + 1 == self.messages.len() && self.edge.foot() > 0.0 {
            let edge = &self.edge;
            whole = whole.push(
                container(self.edge_button(
                    edge.loading_later,
                    edge.later_failed,
                    true,
                    Message::Later,
                ))
                .height(edge.foot())
                .padding(Padding {
                    top: 4.0,
                    bottom: 8.0,
                    ..Padding::ZERO
                })
                .center_x(Length::Fill)
                .align_y(Alignment::Center),
            );
        }
        whole.into()
    }

    /// A new day's date, and the "New" divider in marigold, in the middle
    /// of one line, together when both are here.
    fn divider<'a>(&'a self, look: &'a RowLook) -> Element<'a, Message> {
        let theme = &self.theme;
        let ink = if look.unread_start {
            theme.accent
        } else {
            theme.line
        };
        let line = || {
            container(bar(ink, Length::Fill, 1))
                .width(Length::Fill)
                .center_y(Length::Fill)
        };
        let mut middle = row![line()].spacing(10).align_y(Alignment::Center);
        if let Some(day) = &look.day {
            middle = middle.push(
                text(day.as_str())
                    .size(12)
                    .font(SEMIBOLD)
                    .color(theme.muted),
            );
        }
        if look.day.is_some() && look.unread_start {
            middle = middle.push(text("·").size(13).font(BOLD).color(theme.muted));
        }
        if look.unread_start {
            let (accent, on_accent) = (theme.accent, theme.on_accent);
            middle = middle.push(
                container(text("New").size(12).font(BOLD).color(on_accent))
                    .padding([0, 5])
                    .height(18)
                    .align_y(Alignment::Center)
                    .style(move |_| rounded(accent, 4)),
            );
        }
        container(middle.push(line()))
            .padding(Padding {
                left: 16.0,
                right: 16.0,
                bottom: 6.0,
                top: 0.0,
            })
            .height(DIVIDER)
            .into()
    }

    /// The history's edge, as the Slint client's `HistoryEdge`: Discord's
    /// welcome at a server channel's start, a line at another's, else the
    /// Earlier messages button, loading or failed.
    fn history_edge(&self) -> Element<'_, Message> {
        if self.edge.welcome() {
            return self.welcome();
        }
        self.edge_line(&self.edge, Message::Earlier)
    }

    /// The history's edge of a timeline whose start has no welcome: where
    /// it starts, or Earlier messages (`earlier`), loading, or failed.
    pub fn edge_line(&self, edge: &Edge, earlier: Message) -> Element<'_, Message> {
        let theme = &self.theme;
        let shown: Element<'_, Message> = if edge.start {
            let words = if edge.direct {
                format!("This is the beginning of your messages with {}.", edge.name)
            } else if edge.thread {
                format!("This is the start of the {} thread.", edge.name)
            } else {
                format!("This is the start of #{}.", edge.name)
            };
            text(words).size(14).color(theme.muted).into()
        } else {
            self.edge_button(edge.loading, edge.failed, false, earlier)
        };
        container(shown)
            .width(Length::Fill)
            .height(EDGE)
            .padding(Padding {
                top: 4.0,
                bottom: 8.0,
                ..Padding::ZERO
            })
            .center_x(Length::Fill)
            .align_y(Alignment::Center)
            .into()
    }

    /// Earlier messages, or with `newer` Newer messages, as a pill that
    /// loads them (`load`): loading, or failed and offering a retry.
    fn edge_button(
        &self,
        loading: bool,
        failed: bool,
        newer: bool,
        load: Message,
    ) -> Element<'_, Message> {
        let theme = &self.theme;
        let which = if newer { "newer" } else { "earlier" };
        let (icon, ink, words) = if loading {
            (
                Icon::History,
                theme.secondary,
                format!("Loading {which} messages..."),
            )
        } else if failed {
            (
                Icon::TriangleAlert,
                theme.danger,
                format!("Couldn't load {which} messages. Retry"),
            )
        } else if newer {
            (Icon::History, theme.secondary, "Newer messages".to_owned())
        } else {
            (
                Icon::History,
                theme.secondary,
                "Earlier messages".to_owned(),
            )
        };
        let face = row![
            self.glyph(icon, 16, ink),
            text(words).size(14).font(MEDIUM).color(theme.secondary)
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let enabled = !loading && !self.client.state().preview;
        let (rest, lit) = (theme.raised, theme.active);
        button(
            container(face)
                .padding(Padding {
                    left: 12.0,
                    right: 14.0,
                    ..Padding::ZERO
                })
                .height(30)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press_maybe(enabled.then_some(load))
        .style(move |_, status| button::Style {
            background: Some(
                if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    lit
                } else {
                    rest
                }
                .into(),
            ),
            border: iced::Border::default().rounded(15),
            ..button::Style::default()
        })
        .into()
    }

    /// Discord's welcome: a server's in two lines over its line of text, a
    /// channel's under a hash in a circle.
    fn welcome(&self) -> Element<'_, Message> {
        let edge = &self.edge;
        let theme = &self.theme;
        let title = |words: String, weight| {
            text(words)
                .size(32)
                .font(weight)
                .color(theme.text)
                .height(40)
                .wrapping(text::Wrapping::None)
        };
        let mut block = column![]
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .padding(Padding {
                top: 47.0,
                bottom: 12.0,
                ..Padding::ZERO
            });
        let note = match &edge.intro {
            Intro::Server(name) => {
                block = block
                    .push(title("Welcome to".to_owned(), SEMIBOLD))
                    .push(title(name.clone(), SEMIBOLD));
                text("This is the beginning of this server.").size(14)
            }
            _ => {
                let field = theme.field;
                block = block
                    .push(
                        container(self.glyph(Icon::Hash, 40, theme.text))
                            .center(68)
                            .style(move |_| rounded(field, 34)),
                    )
                    .push(Space::new().height(8))
                    .push(title(format!("Welcome to #{}!", edge.name), BOLD));
                text(format!("This is the start of the #{} channel.", edge.name)).size(16)
            }
        };
        block
            .push(Space::new().height(8))
            .push(note.color(theme.secondary).height(20))
            .height(self.edge.height())
            .into()
    }

    /// An empty conversation's opening, as the Slint client's
    /// `ConversationStart`.
    fn conversation_start(&self) -> Element<'_, Message> {
        let edge = &self.edge;
        let theme = &self.theme;
        let mut start = column![].spacing(8).padding(24).width(Length::Fill);
        if self.client.state().selected.is_some() && edge.ready {
            if edge.direct {
                start = start.push(self.face(Face {
                    url: self.header.avatar.as_deref(),
                    name: &self.header.name,
                    key: None,
                    size: 72.0,
                    opacity: 1.0,
                    decoration: None,
                    play: false,
                    dot: None,
                }));
            } else {
                let raised = theme.raised;
                let icon = self.header.icon.unwrap_or(Icon::Hash);
                start = start.push(
                    container(self.glyph(icon, 36, theme.text))
                        .center(68)
                        .style(move |_| rounded(raised, 34)),
                );
            }
        }
        let (title, size) = if !edge.ready {
            ("Loading conversation...".to_owned(), 18)
        } else if edge.direct || edge.thread {
            (edge.name.clone(), 28)
        } else {
            (format!("Welcome to #{}", edge.name), 28)
        };
        start = start.push(text(title).size(size).font(BOLD).color(theme.text));
        if edge.ready {
            let words = if edge.direct {
                format!("This is the beginning of your messages with {}.", edge.name)
            } else if edge.thread {
                format!(
                    "This is the start of the {} thread. No messages here yet.",
                    edge.name
                )
            } else {
                format!("This is the start of #{}. No messages here yet.", edge.name)
            };
            start = start.push(text(words).color(theme.secondary));
        }
        container(start)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End)
            .into()
    }

    /// While the "New" divider is out of view, until the newest message is
    /// read, a bar that jumps to it, saying how much is new.
    fn unread_bar(&self) -> Option<Element<'_, Message>> {
        let state = self.client.state();
        if state.caught_up {
            return None;
        }
        let index = self.unread_index()?;
        let view_top = (self.total_height() - self.timeline_height - self.from_end).max(0.0);
        let view_bottom = view_top + self.timeline_height;
        let top = self.tops[index];
        let above = top < view_top - 20.0;
        if !above && top <= view_bottom {
            return None;
        }
        let theme = &self.theme;
        let label = unread_label(&self.messages[index..], state.detached);
        let chevron = if above {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        };
        let face = row![
            text(label).size(14).font(BOLD).color(theme.on_accent),
            self.glyph(chevron, 16, theme.on_accent)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let (rest, lit) = (theme.accent, theme.accent_hover);
        Some(
            button(
                container(face)
                    .padding(Padding {
                        left: 14.0,
                        right: 10.0,
                        ..Padding::ZERO
                    })
                    .height(UNREAD_BAR)
                    .align_y(Alignment::Center),
            )
            .padding(0)
            .on_press(Message::JumpUnread)
            .style(move |_, status| button::Style {
                background: Some(
                    if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                        lit
                    } else {
                        rest
                    }
                    .into(),
                ),
                border: iced::Border::default().rounded(UNREAD_BAR / 2.0),
                shadow: bar_shadow(),
                ..button::Style::default()
            })
            .into(),
        )
    }

    /// A window of older messages says so over its end, with the way back.
    fn present_bar(&self) -> Element<'_, Message> {
        let theme = &self.theme;
        let face = row![
            text("You're viewing older messages")
                .size(14)
                .font(MEDIUM)
                .color(theme.secondary)
                .width(Length::Fill),
            text("Jump to present")
                .size(14)
                .font(BOLD)
                .color(theme.accent),
            self.glyph(Icon::ChevronDown, 16, theme.accent),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let (ground, edge) = (theme.raised, theme.edge);
        button(
            container(face)
                .padding(Padding {
                    left: 14.0,
                    right: 12.0,
                    ..Padding::ZERO
                })
                .height(PRESENT_BAR)
                .align_y(Alignment::Center),
        )
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::JumpPresent)
        .style(move |_, _| button::Style {
            background: Some(ground.into()),
            border: iced::Border {
                color: edge,
                width: 1.0,
                radius: 8.0.into(),
            },
            shadow: bar_shadow(),
            ..button::Style::default()
        })
        .into()
    }

    /// Where the "New" divider is, when its message is loaded.
    pub fn unread_index(&self) -> Option<usize> {
        let item = self.client.state().first_unread.as_deref()?;
        self.messages
            .iter()
            .position(|message| message.item == item)
    }

    /// Heights the timeline's rows found at their last layout, which the
    /// layout already made room for (`Rows::measured`): the rows keep them.
    /// Taken in before each message, so the view built next places rows
    /// where they were drawn.
    pub fn take_found(&mut self) {
        let found = std::mem::take(&mut *self.found.borrow_mut());
        if found.is_empty() {
            return;
        }
        let (first, last) = self.in_view(MARGIN);
        let mut changed = false;
        for (row, height) in found {
            if let Some(index) = (first..last).find(|&index| key(&self.messages[index].item) == row)
                && (self.heights[index] - height).abs() > 0.5
            {
                self.heights[index] = height;
                changed = true;
            }
        }
        if changed {
            self.place_rows();
        }
    }

    /// The rows built in and below the view of someone reading earlier
    /// ones report the heights they took: the rows keep them, and the view
    /// stays on what's read ([`App::keep_reading`]).
    pub fn measured(&mut self, heights: Vec<(u64, f32)>) -> Task<Message> {
        let (first, last) = self.in_view(MARGIN);
        let reading = self.reading();
        let mut changed = false;
        for (row, height) in heights {
            let Some(index) = (first..last).find(|&index| key(&self.messages[index].item) == row)
            else {
                continue;
            };
            if (self.heights[index] - height).abs() <= 0.5 {
                continue;
            }
            self.heights[index] = height;
            changed = true;
        }
        if !changed {
            return Task::none();
        }
        self.place_rows();
        Task::batch([self.keep_reading(reading), self.report()])
    }

    /// While someone reads earlier messages, the row at the view's top and
    /// how far into it the view starts; at the end, none (the scrollable,
    /// anchored to the bottom, follows the end).
    pub fn reading(&self) -> Option<(String, f32)> {
        if self.at_end || self.messages.is_empty() {
            return None;
        }
        let top = self.view_top();
        let index = self.tops[1..]
            .partition_point(|end| *end <= top)
            .min(self.messages.len() - 1);
        Some((self.messages[index].item.clone(), top - self.tops[index]))
    }

    /// Puts the row [`App::reading`] found back where it was in the view,
    /// whatever came, went or changed height above or below it, as the
    /// Slint client and Discord keep what's read in place. The scrollable
    /// is moved by the difference rather than to a place, so moves asked
    /// for before it reported where it is add up.
    pub fn keep_reading(&mut self, reading: Option<(String, f32)>) -> Task<Message> {
        let Some((item, into)) = reading else {
            return Task::none();
        };
        let Some(index) = self
            .messages
            .iter()
            .position(|message| message.item == item)
        else {
            return Task::none();
        };
        let top = self.tops[index] + into;
        let from_end = (self.total_height() - self.timeline_height - top).max(0.0);
        let by = from_end - self.from_end;
        if by.abs() <= 0.5 {
            return Task::none();
        }
        self.from_end = from_end;
        operation::scroll_by(TIMELINE, AbsoluteOffset { x: 0.0, y: by })
    }

    /// Where the view's top is among the rows.
    pub fn view_top(&self) -> f32 {
        (self.total_height() - self.timeline_height - self.from_end).max(0.0)
    }

    /// Goes to the newest message.
    pub fn go_to_end(&mut self) -> Task<Message> {
        self.from_end = 0.0;
        self.at_end = true;
        operation::scroll_to(
            TIMELINE,
            AbsoluteOffset {
                x: None,
                y: Some(0.0),
            },
        )
    }

    /// Brings row `index` into view: its top at the view's top, or a third
    /// of the way down.
    pub fn scroll_to_row(&mut self, index: usize, third: bool) -> Task<Message> {
        let lead = if third {
            self.timeline_height / 3.0
        } else {
            0.0
        };
        let top = self.tops[index] - lead;
        self.from_end = (self.total_height() - self.timeline_height - top).max(0.0);
        self.at_end = self.from_end <= 2.0;
        operation::scroll_to(
            TIMELINE,
            AbsoluteOffset {
                x: None,
                y: Some(self.from_end),
            },
        )
    }

    /// Where the core asked the timeline to go, once it has the message:
    /// a jump (tinted a moment) or the first unread message.
    pub fn follow_jumps(&mut self) -> Task<Message> {
        let state = self.client.state();
        let jump = state.jump.clone();
        let unread_jump = state.unread_jump;
        let mut tasks = Vec::new();
        if jump != self.jumped
            && let Some((item, _)) = &jump
            && let Some(index) = self
                .messages
                .iter()
                .position(|message| &message.item == item)
        {
            self.jumped = jump.clone();
            self.jump_lit = Some(item.clone());
            tasks.push(self.scroll_to_row(index, true));
            tasks.push(crate::after(JUMP_LIT, Message::JumpFaded(item.clone())));
        }
        if unread_jump != self.unread_jumped
            && let Some(index) = self.unread_index()
        {
            self.unread_jumped = unread_jump;
            tasks.push(self.scroll_to_row(index, false));
        }
        Task::batch(tasks)
    }

    /// A link in the timeline's text was clicked: a spoiler shows, a
    /// mention or author opens their card, a jump goes to its message, a
    /// web address opens in the browser (a masked one once asked).
    pub fn follow_link(&mut self, link: &str, at: iced::Point) -> Task<Message> {
        if let Some((item, index)) = yapfast_core::spoiler::target(link) {
            self.revealed.insert(format!("{item}/{index}"));
            let found =
                |messages: &Messages| messages.iter().position(|message| message.item == item);
            if let Some(row) = found(&self.messages)
                && let Some(body) = self.body_for(row)
            {
                self.looks[row].body = Arc::new(body);
            } else if let Some(index) = self.aside.as_ref().and_then(|aside| found(&aside.messages))
                && let Some(body) = self.body_for(ASIDE + index)
                && let Some(aside) = self.aside.as_mut()
            {
                aside.looks[index].body = Arc::new(body);
            }
            return Task::none();
        }
        // An emoji opens its card under it.
        if let Some(target) = yapfast_core::emoji::card::target(link) {
            return self.show_expression(crate::info_cards::Expression::Emoji(target));
        }
        if let Some(item) = link.strip_prefix(JUMP_LINK) {
            self.client.dispatch(Action::Jump(item.to_owned()));
            return self.sync();
        }
        if link == PINS_LINK {
            self.client.dispatch(Action::ShowPins(true));
            return self.sync();
        }
        let user = link
            .strip_prefix(AUTHOR_LINK)
            .map(str::to_owned)
            .or_else(|| match yapfast_core::mention::target(link)? {
                yapfast_core::mention::Target::User(user) => Some(user),
                _ => None,
            });
        if let Some(user) = user {
            return self.open_card(user, iced::Point::new(at.x + 8.0, at.y));
        }
        if let Some(yapfast_core::mention::Target::Role(role)) = yapfast_core::mention::target(link)
        {
            return self.show_role(role, at);
        }
        if let Some(yapfast_core::mention::Target::Channel(room)) =
            yapfast_core::mention::target(link)
        {
            // As the Slint client's: its server or thread opens, and a voice
            // channel isn't joined.
            self.client.dispatch(Action::Open(room));
            return self.sync();
        }
        // Discord's own links open their place here, when this client can
        // show it, and in the browser otherwise, as the Slint client's do:
        // an invite shows its server first and joins only when asked.
        if let Some(action) = self.discord_link(link, at) {
            return match action {
                Ok(action) => {
                    self.client.dispatch(action);
                    self.sync()
                }
                Err(task) => task,
            };
        }
        // A masked link, whose words aren't where it goes, asks first.
        if let Some(target) = yapfast_core::links::masked(link) {
            self.leaving = Some(target.to_owned());
            return operation::focus(crate::app::NOBODY);
        }
        if yapfast_core::links::openable(link) {
            crate::browser::open(link);
        }
        Task::none()
    }

    /// What one of Discord's own links does here: an action for the core,
    /// or a profile's card at the words clicked; none for a link the core
    /// can't open now, which opens in the browser.
    fn discord_link(
        &mut self,
        link: &str,
        at: iced::Point,
    ) -> Option<Result<Action, Task<Message>>> {
        use yapfast_core::discord::{self, DiscordRoute};
        let route = discord::discord_route(yapfast_core::links::masked(link).unwrap_or(link))
            .filter(|route| self.client.state().opens_discord_route(route))?;
        Some(Ok(match route {
            DiscordRoute::Invite { code } => Action::OpenInviteLink(code),
            DiscordRoute::DirectMessages => {
                Action::Navigate(yapfast_core::application::Navigation::DirectMessages)
            }
            DiscordRoute::Server { guild } => Action::Navigate(
                yapfast_core::application::Navigation::Server(discord::server_id(&guild)),
            ),
            DiscordRoute::Channel { guild, channel } => {
                Action::Open(discord::room_id(&guild, &channel))
            }
            DiscordRoute::Message {
                guild,
                channel,
                message,
            } => Action::JumpInRoom {
                room: discord::room_id(&guild, &channel),
                item: format!("d:{message}"),
            },
            DiscordRoute::User { user } => {
                return Some(Err(self.open_card(user, iced::Point::new(at.x + 8.0, at.y))));
            }
            DiscordRoute::Authorize { query } => Action::OpenOAuthLink(query),
            DiscordRoute::Gift { code } => Action::OpenGiftLink(code),
            DiscordRoute::Event { guild, event } => Action::OpenEventLink { guild, event },
        }))
    }

    /// A spoiler over a message's attachment was clicked: it shows.
    pub fn reveal(&mut self, row: usize, attachment: usize) -> Task<Message> {
        if let Some(message) = self.row_message(row) {
            self.revealed
                .insert(crate::message::reveal_key(&message.item, attachment));
        }
        Task::none()
    }

    /// A long message shows whole, or folds again.
    pub fn unfold(&mut self, row: usize) -> Task<Message> {
        if let Some(message) = self.row_message(row) {
            let key = crate::message::fold_key(&message.item);
            if !self.revealed.remove(&key) {
                self.revealed.insert(key);
            }
        }
        Task::none()
    }

    /// Row `index`'s text laid out, as it shows now.
    pub fn body_for(&self, index: usize) -> Option<Laid> {
        let message = self.row_message(index)?;
        let revealed = &self.revealed;
        Some(body_of(
            message,
            rgb(self.theme.mention),
            rgb(self.theme.text),
            rgb(self.theme.muted),
            |spoiler| revealed.contains(&format!("{}/{spoiler}", message.item)),
        ))
    }
}

/// The jump bars' shadow, as Slint's: 12 px blur, 4 px down.
fn bar_shadow() -> iced::Shadow {
    iced::Shadow {
        color: Color::from_rgba8(0, 0, 0, 0x60 as f32 / 255.0),
        offset: iced::Vector::new(0.0, 4.0),
        blur_radius: 12.0,
    }
}

/// What the unread bar says, as the Slint client's `unread_label`: how many
/// new messages (more when the window is older ones), since when.
pub fn unread_label(unread: &[Arc<ChatMessage>], more: bool) -> String {
    let count = unread.iter().filter(|message| !message.own).count();
    let messages = if count == 1 && !more {
        "message"
    } else {
        "messages"
    };
    let count = if more {
        format!("{count}+")
    } else {
        count.to_string()
    };
    let Some(first) = unread.first().filter(|first| first.time.contains(':')) else {
        return format!("{count} new {messages}");
    };
    let on = yapfast_core::clock::day(first.timestamp);
    if on.is_empty() || on == yapfast_core::clock::day(yapfast_core::clock::now_ms() / 1000) {
        format!("{count} new {messages} since {}", first.time)
    } else {
        format!("{count} new {messages} since {} on {on}", first.time)
    }
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
