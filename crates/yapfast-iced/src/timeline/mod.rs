//! The conversation and its timeline, as the Slint client's: messages as
//! virtualized rows grouped as Discord groups them, a new day's date and
//! the "New" divider above the first unread message, the history's edge
//! (earlier messages loading, or the conversation's start and Discord's
//! welcome), a mention's marigold tint, and the bars that jump to the
//! first unread message and back to the present. One message's content is
//! `message/`'s. Here: where a row is and what it holds; `rows.rs` the
//! rows' looks and estimated heights, `view.rs` the timeline drawn,
//! `scroll.rs` where it's scrolled to, `clocks.rs` the times it shows.
//!
//! Rows are placed at heights worked out ahead ([`row_height`]) until
//! they're built: then `widgets::Rows` measures them and the app keeps the
//! real heights. The scrollable is anchored to the bottom: it keeps the
//! distance from the end, so it follows new messages at the end and rows
//! stay put as history loads or rows grow above; rows that grow or come
//! below while someone reads earlier ones are made up for here.

use std::{sync::Arc, time::Duration};

use iced::{Color, Task};
use yapfast_core::chat::ChatMessage;

use crate::{
    app::{App, Message},
    theme::rgb,
};

use rows::top_space;

mod clocks;
mod rows;
mod scroll;
mod view;

pub use clocks::{Clock, Clocks};
pub use rows::{
    Edge, EmbedText, Intro, Laid, Marks, RowLook, body_of, row_height, row_looks, same_text,
};

/// The timeline's scrollable, for the app's scroll tasks.
pub const TIMELINE: &str = "timeline";
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

impl App {
    /// Row `row`'s message, in the channel's timeline or the thread's
    /// beside it ([`ASIDE`]).
    pub fn row_message(&self, row: usize) -> Option<&Arc<ChatMessage>> {
        match row.checked_sub(ASIDE) {
            Some(index) => self.aside.as_ref()?.messages.get(index),
            None => self.messages.get(row),
        }
    }

    /// The row holding the message `item` now: in the channel's timeline,
    /// or the thread's beside it, numbered from [`ASIDE`]. What a click on
    /// a row sends names its message, and is placed here when it's handled,
    /// as a batch handled first may have moved every row.
    pub fn row_of(&self, item: &str) -> Option<usize> {
        self.messages
            .iter()
            .rposition(|message| message.item == item)
            .or_else(|| {
                let aside = self.aside.as_ref()?;
                let index = aside
                    .messages
                    .iter()
                    .rposition(|message| message.item == item)?;
                Some(ASIDE + index)
            })
    }

    /// Row `row`'s message's item, shared, for what its parts send.
    pub fn row_item(&self, row: usize) -> Arc<str> {
        self.row_look(row)
            .map_or_else(|| Arc::from(""), |look| look.item.clone())
    }

    /// Row `row`'s look, as [`App::row_message`] finds its message.
    pub fn row_look(&self, row: usize) -> Option<&RowLook> {
        match row.checked_sub(ASIDE) {
            Some(index) => self.aside.as_ref()?.looks.get(index),
            None => self.looks.get(row),
        }
    }

    /// Where row `row`'s timeline is in the window, and the row's top in
    /// it: among the rows, under the space a short history is held down
    /// by, less the view's.
    pub fn lane_place(&self, row: usize) -> (LanePlace, f32) {
        match (row.checked_sub(ASIDE), self.aside.as_ref()) {
            (Some(index), Some(aside)) => (
                LanePlace {
                    left: self.window.width - aside.width,
                    width: aside.width,
                    top: crate::header::HEIGHT + 1.0 + crate::thread::HEAD + 1.0,
                },
                aside.tops.get(index).copied().unwrap_or(0.0)
                    + (aside.height - aside.total_height()).max(0.0)
                    - aside.view_top(),
            ),
            _ => (
                LanePlace {
                    left: self.chat_left(),
                    width: self.width,
                    top: crate::header::HEIGHT + 1.0,
                },
                self.tops.get(row).copied().unwrap_or(0.0)
                    + (self.timeline_height - self.total_height()).max(0.0)
                    - self.view_top(),
            ),
        }
    }

    /// Where row `row`'s message starts in the window, under its head (the
    /// history's edge over the first, a divider) and the space above it:
    /// the top of its hover bar's row and its avatar's padding.
    pub fn row_body_top(&self, row: usize) -> f32 {
        let (lane, top) = self.lane_place(row);
        let edge = match row.checked_sub(ASIDE) {
            Some(0) if self.aside.as_ref().is_some_and(|aside| aside.ready) => EDGE,
            Some(_) => 0.0,
            None if row == 0 => self.edge.height(),
            None => 0.0,
        };
        lane.top + top + edge + self.row_look(row).map_or(0.0, top_space)
    }

    /// A theme switch: the text laid out in the old theme's inks (its
    /// mentions', a notice's) is laid out again in the new one's, in the
    /// channel's timeline and the thread beside, the rows keeping their
    /// heights. A switch that keeps those inks costs nothing.
    pub fn recolour(&mut self, old: &crate::theme::Theme) {
        let theme = &self.theme;
        if (theme.mention, theme.text, theme.muted) == (old.mention, old.text, old.muted) {
            return;
        }
        let (mention, ink, muted) = (rgb(theme.mention), rgb(theme.text), rgb(theme.muted));
        let revealed = &self.revealed;
        let lay = |message: &ChatMessage| {
            Arc::new(body_of(message, mention, ink, muted, |spoiler| {
                revealed.contains(&format!("{}/{spoiler}", message.item))
            }))
        };
        for (look, message) in self.looks.iter_mut().zip(self.messages.iter()) {
            look.body = lay(message);
        }
        if let Some(aside) = &mut self.aside {
            for (look, message) in aside.looks.iter_mut().zip(aside.messages.iter()) {
                look.body = lay(message);
            }
        }
    }

    /// A spoiler over a message's attachment was clicked: it shows.
    pub fn reveal(&mut self, item: &str, attachment: usize) -> Task<Message> {
        self.revealed
            .insert(crate::message::reveal_key(item, attachment));
        Task::none()
    }

    /// A long message shows whole, or folds again.
    pub fn unfold(&mut self, item: &str) -> Task<Message> {
        let key = crate::message::fold_key(item);
        if !self.revealed.remove(&key) {
            self.revealed.insert(key);
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
