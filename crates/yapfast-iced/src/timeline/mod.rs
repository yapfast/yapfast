//! The conversation and its timeline: messages as virtualized rows grouped
//! as Discord groups them, a new day's date and the "New" divider above the
//! first unread message, the history's edge (earlier messages loading, or
//! the conversation's start and Discord's welcome), a mention's marigold
//! tint, and the bars that jump to the first unread message and back to
//! the present. One message's content is `message/`'s. Here: where a row is
//! and what it holds; `rows.rs` the rows' looks and estimated heights,
//! `view.rs` the timeline drawn, `scroll.rs` where it's scrolled to,
//! `clocks.rs` the times it shows.
//!
//! Rows are placed at heights worked out ahead ([`row_height`]) until
//! they're built: then `widgets::Rows` measures them and the app keeps the
//! real heights. The scrollable is anchored to the bottom: it keeps the
//! distance from the end, so it follows new messages at the end and rows
//! stay put as history loads or rows grow above; rows that grow or come
//! below while someone reads earlier ones are made up for here.

use std::{sync::Arc, time::Duration};

use iced::{Color, Point, Task, widget::scrollable::Viewport};
use yapfast_core::{application::Action, chat::ChatMessage};

use crate::{
    app::{App, Message},
    theme::rgb,
};

use rows::top_space;

pub mod clocks;
pub mod keys;
mod rows;
pub mod scroll;
pub mod selection;
pub mod view;

pub use clocks::{Clock, Clocks};
pub use rows::{
    Edge, EmbedText, Intro, Laid, Marks, RowLook, body_of, row_height, row_looks, same_text,
};

/// The timeline's scrollable, for the app's scroll tasks.
pub const TIMELINE: &str = "timeline";
/// How long the pictures rows want wait to be asked for together.
pub const ASK_EVERY: Duration = Duration::from_millis(50);
/// How long a message jumped to stays tinted.
const JUMP_LIT: Duration = Duration::from_secs(2);

/// The timeline's Discord look: 15 px text on lines 1.3 times that, the
/// author's line 20 px and 2 more under it, 40 px avatars in a 72 px
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
pub const GROUP_GAP: f32 = 18.0;
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
/// A one-to-one DM's: the other person's avatar, name and handle, and what
/// to do about a stranger.
const INTRO_DIRECT: f32 = 276.0;
/// The banner over the messages while some are unread (it jumps to the
/// first, and marks them read), and the pill over a window's end that goes
/// back to the present.
const UNREAD_BAR: f32 = 34.0;
const PRESENT_BAR: f32 = 50.0;
/// A message that calls on this account or replies to it, measured on
/// Discord's dark theme: its gold (#ea9800) at 8.6% over the chat, 11%
/// under the pointer, with a 2 px edge of the full gold. Fixed colours:
/// Discord's gold doesn't follow the palette.
const MENTIONED: Color = Color::from_rgba8(0xea, 0x98, 0x00, 0x16 as f32 / 255.0);
const MENTIONED_HOVER: Color = Color::from_rgba8(0xea, 0x98, 0x00, 0x1c as f32 / 255.0);
const MENTION_GOLD: Color = Color::from_rgb8(0xea, 0x98, 0x00);
const MENTION_EDGE: f32 = 2.0;
/// The message a reply being written answers: blurple at 17% (21% under the
/// pointer) with a blurple edge, in place of the gold tint.
const REPLY_TARGET: Color = Color::from_rgba8(0x58, 0x65, 0xf2, 0x2b as f32 / 255.0);
const REPLY_TARGET_HOVER: Color = Color::from_rgba8(0x58, 0x65, 0xf2, 0x35 as f32 / 255.0);
const REPLY_EDGE: Color = Color::from_rgb8(0x58, 0x65, 0xf2);
/// The marigold edge down the message the keys are stepping on.
const KEYED_EDGE: f32 = 3.0;
/// A file's card's height, and an embed's text, for estimates.
const FILE_CARD: f32 = 58.0;
const EMBED: f32 = 90.0;
/// An embed's pictures' room, for estimates.
const EMBED_ROOM: f32 = 400.0;
/// A message's thread's chip, for estimates.
const THREAD_CHIP: f32 = 44.0;
/// The body font's average advance at 15 px, for wrapping estimates.
const CHAR: f32 = 7.1;

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

/// The channel's timeline: its rows and what each shows, where it's
/// scrolled, and the selection across its messages and the thread's.
pub struct Timeline {
    pub messages: crate::app::Messages,
    /// Each row's height and, one longer, where each row starts; what each
    /// row shows beyond its message; the history's edge above the first.
    pub heights: Vec<f32>,
    pub tops: Vec<f32>,
    pub looks: Vec<RowLook>,
    pub edge: Edge,
    /// Heights the timeline's last layout found, not yet taken in.
    pub found: crate::widgets::Found,
    /// Spoilers clicked, as `item/index`, while Yapfast runs.
    pub revealed: std::collections::HashSet<String>,
    /// The core's last jump and jump to the first unread message that the
    /// timeline went to, and the message tinted for it.
    pub jumped: Option<(String, u64)>,
    pub unread_jumped: u64,
    pub jump_lit: Option<String>,
    /// The link a right press was on, and where the press was: the menu
    /// that press opens offers it.
    pub pressed_link: Option<(String, Point)>,
    /// The "New" divider's message and the last read, as the looks were
    /// worked out for.
    pub marks_for: (Option<String>, Option<String>),
    /// The history's edge changed since the rows' heights were worked out.
    pub edge_changed: bool,
    /// The viewport: its distance from the end, which the scrollable
    /// (anchored to the bottom) keeps as rows come, its height and width,
    /// and whether it's at the end.
    pub from_end: f32,
    pub height: f32,
    pub width: f32,
    pub at_end: bool,
    /// Where the list is in the window, as it last said and as a press
    /// on it notes: under the header, and under what else is over it (a
    /// banner, a forum post's bar, a DM's call). Empty until either.
    pub place: std::cell::Cell<iced::Rectangle>,
    /// The window's last press as the keys came onto the messages: a later
    /// one off the list takes them away.
    pub keyed_press: iced::Point,
    /// Sent from a window of older messages, or Jump to present: the view
    /// goes to the newest once they've replaced the window.
    pub latest_due: bool,
    /// Where the timeline was read as the window closed to the tray: the
    /// row at the view's top and how far into it. The next window's list
    /// goes back there as it first says where it is.
    pub left_at: Option<(String, f32)>,
    /// The selection, where it started and where it goes to, and whether
    /// the button is still down.
    pub selection: Option<(
        crate::timeline::selection::Sel,
        crate::timeline::selection::Sel,
    )>,
    pub selecting: bool,
    /// The times the rows show, and their timers.
    pub clocks: Clocks,
}

impl Timeline {
    /// The rows in view, and `margin` beyond.
    pub fn in_view(&self, margin: f32) -> (usize, usize) {
        let top = self.view_top();
        let bottom = top + self.height;
        let count = self.heights.len();
        // The first row ending below the top, the last starting above the bottom.
        let first = self.tops[1..].partition_point(|end| *end <= top - margin);
        let last = self.tops[..count].partition_point(|start| *start < bottom + margin);
        (first.min(count), last.max(first).min(count))
    }

    /// Where the "New" divider is, when its message is loaded.
    pub fn unread_index(&self, state: &yapfast_core::application::AppState) -> Option<usize> {
        let item = state.first_unread.as_deref()?;
        self.messages
            .iter()
            .position(|message| message.item == item)
    }

    /// The timeline's whole height.
    pub fn total_height(&self) -> f32 {
        self.tops.last().copied().unwrap_or(0.0)
    }

    /// Where the view's top is among the rows.
    pub fn view_top(&self) -> f32 {
        (self.total_height() - self.height - self.from_end).max(0.0)
    }

    /// An empty timeline in a viewport `height` tall and `width` wide.
    pub fn new(height: f32, width: f32) -> Self {
        Self {
            messages: Default::default(),
            heights: Vec::new(),
            tops: vec![0.0],
            looks: Vec::new(),
            edge: Edge::default(),
            found: Default::default(),
            revealed: Default::default(),
            jumped: None,
            unread_jumped: 0,
            jump_lit: None,
            pressed_link: None,
            marks_for: (None, None),
            edge_changed: false,
            from_end: 0.0,
            height,
            width,
            at_end: true,
            place: Default::default(),
            keyed_press: iced::Point::ORIGIN,
            latest_due: false,
            left_at: None,
            selection: None,
            selecting: false,
            clocks: Clocks::default(),
        }
    }
}

/// The channel's timeline and the thread's beside it, as views and the
/// selection read them: row `row` is the channel's below [`ASIDE`] and the
/// thread's from it up.
#[derive(Clone, Copy)]
pub struct Lanes<'a> {
    pub timeline: &'a Timeline,
    pub aside: Option<&'a crate::thread::Aside>,
    /// The message the keys are stepping on, by its item (`keys.rs`).
    pub keyed: Option<&'a str>,
}

impl<'a> Lanes<'a> {
    /// Row `row`'s message, in the channel's timeline or the thread's
    /// beside it ([`ASIDE`]).
    pub fn row_message(self, row: usize) -> Option<&'a Arc<ChatMessage>> {
        match row.checked_sub(ASIDE) {
            Some(index) => self.aside?.messages.get(index),
            None => self.timeline.messages.get(row),
        }
    }

    /// The row holding the message `item` now: in the channel's timeline,
    /// or the thread's beside it, numbered from [`ASIDE`]. What a click on
    /// a row sends names its message, and is placed here when it's handled,
    /// as a batch handled first may have moved every row.
    pub fn row_of(self, item: &str) -> Option<usize> {
        self.timeline
            .messages
            .iter()
            .rposition(|message| message.item == item)
            .or_else(|| {
                let index = self
                    .aside?
                    .messages
                    .iter()
                    .rposition(|message| message.item == item)?;
                Some(ASIDE + index)
            })
    }

    /// Row `row`'s message's item, shared, for what its parts send.
    pub fn row_item(self, row: usize) -> Arc<str> {
        self.row_look(row)
            .map_or_else(|| Arc::from(""), |look| look.item.clone())
    }

    /// Row `row`'s look, as [`Lanes::row_message`] finds its message.
    pub fn row_look(self, row: usize) -> Option<&'a RowLook> {
        match row.checked_sub(ASIDE) {
            Some(index) => self.aside?.looks.get(index),
            None => self.timeline.looks.get(row),
        }
    }
}

/// The channel's timeline and the thread's, for what reads rows.
pub fn lanes(app: &App) -> Lanes<'_> {
    Lanes {
        timeline: &app.timeline,
        aside: app.aside.as_deref(),
        keyed: app.keyed.as_deref(),
    }
}

pub fn row_message(app: &App, row: usize) -> Option<&Arc<ChatMessage>> {
    lanes(app).row_message(row)
}

pub fn row_of(app: &App, item: &str) -> Option<usize> {
    lanes(app).row_of(item)
}

pub fn row_item(app: &App, row: usize) -> Arc<str> {
    lanes(app).row_item(row)
}

/// What the timeline's scrollable, rows and timers send.
#[derive(Clone, Debug)]
pub enum Event {
    /// The timeline's viewport, as its scrollable reports it.
    Scrolled(Viewport),
    /// The timeline's rows built took other heights than they were given.
    Measured(Vec<(u64, f32)>),
    /// The history's Earlier messages button; the jump bars; a jump's
    /// tint fading.
    Earlier,
    /// Newer messages after a window of older ones.
    Later,
    JumpUnread,
    /// The banner's Mark as read.
    MarkRead,
    JumpPresent,
    JumpFaded(String),
    /// A spoiler's cover over a message's attachment was clicked: the
    /// message by its item, the attachment's index.
    Reveal(Arc<str>, usize),
    /// A long message's Show more or Show less, by its item.
    Unfold(Arc<str>),
    /// A blocked person's message, shown when asked for, by its item.
    ShowBlocked(Arc<str>),
    /// The pointer came onto or left a message, by its item.
    PointRow(Arc<str>, bool),
    /// A message's avatar or author was clicked, by the message's item.
    OpenAuthor(Arc<str>),
    /// A right click on a message, by its item, where the pointer is.
    RowMenu(Arc<str>, Point),
    /// The right press that asks for a row's menu next was on this link.
    RowLink(String),
    /// The pointer came onto a reaction chip or left it, and its tooltip
    /// fell due.
    PointReaction(String, String, bool),
    ReactionDue(String, String),
    /// An app's button that asks the app, on a message by item, by the
    /// button's id.
    UseButton(String, String),
    /// Someone a row names other than its author (who used an app's
    /// command), by their id: their card opens beside the press.
    OpenPerson(String),
    /// An app's slash command a row names, by its name: written in the
    /// composer for its list to find.
    UseCommand(String),
}

/// What each of the timeline's events does.
pub fn update(app: &mut App, event: Event) -> Task<Message> {
    match event {
        Event::Scrolled(viewport) => app.timeline_scrolled(viewport),
        Event::Measured(heights) => scroll::measured(app, heights),
        Event::Earlier => {
            app.client.dispatch(Action::Earlier);
            app.sync()
        }
        Event::Later => {
            app.client.dispatch(Action::Later);
            app.sync()
        }
        Event::JumpUnread => {
            app.client.dispatch(Action::JumpToUnread);
            app.sync()
        }
        Event::MarkRead => {
            if let Some(room) = app.client.state().selected.clone() {
                app.client.dispatch(Action::MarkRead(room));
            }
            app.sync()
        }
        Event::JumpPresent => {
            app.timeline.latest_due = true;
            app.client.dispatch(Action::JumpToPresent);
            app.sync()
        }
        Event::JumpFaded(item) => {
            if app.timeline.jump_lit.as_ref() == Some(&item) {
                app.timeline.jump_lit = None;
            }
            Task::none()
        }
        Event::Reveal(item, attachment) => reveal(app, &item, attachment),
        Event::Unfold(item) => unfold(app, &item),
        Event::ShowBlocked(item) => {
            app.timeline
                .revealed
                .insert(crate::message::blocked_key(&item));
            app.ask()
        }
        Event::PointRow(item, entered) => {
            crate::app::point(&mut app.pointed.message, item, entered);
            app.ask()
        }
        Event::OpenAuthor(item) => match row_of(app, &item) {
            Some(row) => crate::message::open_author(app, row),
            None => Task::none(),
        },
        Event::RowLink(link) => {
            app.timeline.pressed_link = Some((link, app.pressed.get()));
            Task::none()
        }
        Event::RowMenu(item, at) => {
            // The link this very press was on, if any.
            let pressed = app.pressed.get();
            let link = app
                .timeline
                .pressed_link
                .take()
                .filter(|(_, at)| *at == pressed)
                .map(|(link, _)| link);
            match row_of(app, &item) {
                Some(row) => {
                    let task = crate::menus::open_menu(app, row, at);
                    if let Some(link) = link {
                        crate::menus::add_link_rows(app, &link);
                    }
                    task
                }
                None => Task::none(),
            }
        }
        Event::PointReaction(item, key, entered) => app.point_reaction(item, key, entered),
        Event::ReactionDue(item, key) => {
            if app
                .pointed
                .reaction
                .as_ref()
                .is_some_and(|(i, k)| *i == item && *k == key)
            {
                app.client
                    .dispatch(Action::HoverReaction(Some((item, key))));
                return app.sync();
            }
            Task::none()
        }
        Event::UseButton(item, custom_id) => {
            app.client.dispatch(Action::UseButton { item, custom_id });
            app.sync()
        }
        Event::OpenPerson(user) => {
            let at = app.pressed.get();
            app.open_card(user, Point::new(at.x + 8.0, at.y))
        }
        Event::UseCommand(name) => {
            app.client.dispatch(Action::SlashQuery(name));
            let draft = app.client.state().draft().to_owned();
            app.composer.draft.reset(&draft);
            app.composer.draft_known = draft;
            Task::batch([
                app.sync(),
                iced::widget::operation::focus(crate::composer::COMPOSER),
            ])
        }
    }
}

/// A theme switch: the text laid out in the old theme's inks (its
/// mentions', a notice's) is laid out again in the new one's, in the
/// channel's timeline and the thread beside, the rows keeping their
/// heights. A switch that keeps those inks costs nothing.
pub fn recolour(app: &mut App, old: &crate::theme::Theme) {
    let theme = &app.theme;
    if (theme.mention, theme.text, theme.muted) == (old.mention, old.text, old.muted) {
        return;
    }
    let (mention, ink, muted) = (rgb(theme.mention), rgb(theme.text), rgb(theme.muted));
    let revealed = &app.timeline.revealed;
    let lay = |message: &ChatMessage| {
        Arc::new(body_of(message, mention, ink, muted, |spoiler| {
            revealed.contains(&format!("{}/{spoiler}", message.item))
        }))
    };
    for (look, message) in app
        .timeline
        .looks
        .iter_mut()
        .zip(app.timeline.messages.iter())
    {
        look.body = lay(message);
    }
    if let Some(aside) = &mut app.aside {
        for (look, message) in aside.looks.iter_mut().zip(aside.messages.iter()) {
            look.body = lay(message);
        }
    }
}

/// A spoiler over a message's attachment was clicked: it shows.
pub fn reveal(app: &mut App, item: &str, attachment: usize) -> Task<Message> {
    app.timeline
        .revealed
        .insert(crate::message::reveal_key(item, attachment));
    Task::none()
}

/// A long message shows whole, or folds again.
pub fn unfold(app: &mut App, item: &str) -> Task<Message> {
    let key = crate::message::fold_key(item);
    if !app.timeline.revealed.remove(&key) {
        app.timeline.revealed.insert(key);
    }
    Task::none()
}

/// Row `index`'s text laid out, as it shows now.
pub fn body_for(app: &App, index: usize) -> Option<Laid> {
    let message = row_message(app, index)?;
    let revealed = &app.timeline.revealed;
    Some(body_of(
        message,
        rgb(app.theme.mention),
        rgb(app.theme.text),
        rgb(app.theme.muted),
        |spoiler| revealed.contains(&format!("{}/{spoiler}", message.item)),
    ))
}

impl<'a> crate::ctx::Ctx<'a> {
    /// [`Lanes::row_message`].
    pub fn row_message(self, row: usize) -> Option<&'a Arc<ChatMessage>> {
        self.lanes.row_message(row)
    }

    /// [`Lanes::row_item`].
    pub fn row_item(self, row: usize) -> Arc<str> {
        self.lanes.row_item(row)
    }

    /// [`Lanes::row_look`].
    pub fn row_look(self, row: usize) -> Option<&'a RowLook> {
        self.lanes.row_look(row)
    }

    /// [`Lanes::selected`].
    pub fn selected(self, row: usize, part: crate::piece::PartNo) -> crate::piece::Part {
        self.lanes.selected(row, part)
    }

    /// Where row `row`'s timeline is in the window, and the row's top in
    /// it: among the rows, under the space a short history is held down
    /// by, less the view's.
    pub fn lane_place(self, row: usize) -> (LanePlace, f32) {
        match (row.checked_sub(ASIDE), self.lanes.aside) {
            (Some(index), Some(aside)) => (
                LanePlace {
                    left: self.look.window.width - aside.width,
                    width: aside.width,
                    top: crate::header::HEIGHT + 1.0 + crate::thread::HEAD + 1.0,
                },
                aside.tops.get(index).copied().unwrap_or(0.0)
                    + (aside.height - aside.total_height()).max(0.0)
                    - aside.view_top(),
            ),
            _ => (
                LanePlace {
                    left: crate::conversation::navigation(self.layout),
                    width: self.lanes.timeline.width,
                    top: Some(self.lanes.timeline.place.get())
                        .filter(|place| place.height > 0.0)
                        .map_or(crate::header::HEIGHT + 1.0, |place| place.y),
                },
                self.lanes.timeline.tops.get(row).copied().unwrap_or(0.0)
                    + (self.lanes.timeline.height - self.lanes.timeline.total_height()).max(0.0)
                    - self.lanes.timeline.view_top(),
            ),
        }
    }

    /// Where row `row`'s message starts in the window, under its head (the
    /// history's edge over the first, a divider) and the space above it:
    /// the top of its hover bar's row and its avatar's padding.
    pub fn row_body_top(self, row: usize) -> f32 {
        let (lane, top) = self.lane_place(row);
        let edge = match row.checked_sub(ASIDE) {
            Some(0) if self.lanes.aside.is_some_and(|aside| aside.ready) => EDGE,
            Some(_) => 0.0,
            None if row == 0 => self.lanes.timeline.edge.height(),
            None => 0.0,
        };
        lane.top + top + edge + self.row_look(row).map_or(0.0, top_space)
    }
}
