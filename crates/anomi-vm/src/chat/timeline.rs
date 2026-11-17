//! The timeline: the open conversation's messages, what stands above and
//! below them, and what a click or a key on them asks for. The Slint side:
//! `NoConversation`, `HistoryEdge`, `UnreadBar`, `PresentBar` and
//! `ConversationStart` (`ui/messages.slint:2373`, `:2390`, `:2467`, `:2497`,
//! `:2530`), the timeline of `YapfastWindow` (`ui/yapfast.slint:3646`-`:4105`,
//! its properties at `:612`, `:745` and `:1805`-`:1936`), and the glue:
//! `continues`, `day` and `show_timeline` (`src/ui/messages.rs:18`, `:36`,
//! `:95`), `show_messages` (`src/ui/refresh.rs:4371`), `media_viewport`,
//! `earlier`, `later`, the jumps and `reading-changed`
//! (`src/ui/callbacks.rs:707`, `:960`, `:971`, `:1183`).
//!
//! The rows come built: each message as `message::Message` (C02's) with
//! what the timeline marks around it, the group it starts, a new day and
//! the "New" divider ([`group`] says where, for the projection). The
//! screen draws only the rows near the view, follows the newest while it
//! shows, and reports what shows ([`Shown`]); the app keeps the last report
//! here ([`Timeline::shown`]), from which the bar to the "New" divider knows
//! whether the divider is out of view.

pub mod group;

use crate::icon::Icon;
use crate::kind::Avatar;
use crate::menu::Spot;

use super::message::Message;

/// The timeline: what its screen draws. The window holds it as
/// `vm.chat.timeline`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timeline {
    /// The open conversation; none while no place is chosen, when the
    /// timeline says to pick one (`room-selected`).
    pub room: Option<Room>,
    /// Its history has come: until then, with no rows yet, skeletons stand
    /// in for them (`room-ready`).
    pub ready: bool,
    /// The messages loaded, oldest first.
    pub rows: Vec<Row>,
    /// What's above the oldest row (`loading-history`, `history-failed`,
    /// `history-start`).
    pub earlier: Edge,
    /// What's below the newest row of a window of older messages, which new
    /// messages don't go in; none when the rows reach the newest
    /// (`detached`, `loading-later`, `later-failed`).
    pub later: Option<Edge>,
    /// The bar that jumps to the "New" divider while it's out of view.
    pub unread: Option<Unread>,
    /// The message a jump went to, while it's marked.
    pub jump: Option<Jump>,
    /// The keyboard's message and what its keys may do.
    pub keys: Keys,
    /// The message being edited in place, by key: it stays in view as its
    /// field opens and grows (`reveal-edit`, `ui/yapfast.slint:3758`).
    pub editing: Option<u64>,
    /// The compact display (Settings → Messages): groups start 8 px below
    /// the last rather than 16 (`ClientStyle.message-group-gap`).
    pub compact: bool,
    /// Busy, or the local preview: the edges' buttons load nothing
    /// (`!busy && !preview`).
    pub locked: bool,
    /// The rows in view when the list last told, kept by the app from
    /// [`Intent::Shown`]; none before it told.
    pub shown: Option<Span>,
}

/// The open conversation, as the start of its history names it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Room {
    /// The core's room id: another room's list starts afresh.
    pub id: String,
    /// A DM's person or group, a channel's or a thread's name.
    pub name: String,
    pub kind: Kind,
    /// Discord's welcome block, which opens a server channel's history in
    /// place of the one line (`intro-kind`).
    pub intro: Option<Intro>,
}

/// What the conversation is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A DM, with its person's or group's picture.
    Direct(Avatar),
    /// A channel, with its icon: a hash, a speaker, an announcement.
    Channel(Icon),
    /// A thread, named as one rather than as a channel (`room-thread`).
    Thread(Icon),
}

impl Default for Kind {
    fn default() -> Self {
        Self::Channel(Icon::Hash)
    }
}

/// The welcome block over a server channel's first message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intro {
    /// The server's first channel: "Welcome to" over the server's name.
    Server(String),
    /// Any other: a hash in a circle over "Welcome to #name!".
    Channel,
}

/// What stands at an edge of the rows loaded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Edge {
    /// There's more: a button loads it.
    #[default]
    More,
    /// It's on its way.
    Loading,
    /// It didn't come: the button tries again.
    Failed,
    /// The conversation's first message is loaded: its start shows.
    Start,
}

/// A message and what the timeline marks around it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// The message's identity ([`group::key`] of its id), which stays as
    /// rows come and go: the list keeps its place, and a jump finds it, by
    /// this.
    pub key: u64,
    /// It starts a group, so it stands apart from the row above: the
    /// message shows its author (C02's flag, set from the same
    /// [`group::Mark`]).
    pub first: bool,
    /// A new day's date over it, as "3 October 2026".
    pub day: Option<String>,
    /// The "New" divider over it: the first message that came since the
    /// conversation was last read (`unread-start`).
    pub new: bool,
    pub message: Message,
}

/// The bar that says how much is new and jumps to the "New" divider.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Unread {
    /// "5 new messages since 16:55" (`unread_label`).
    pub label: String,
    /// The newest message was read since the divider was placed: no bar.
    pub caught_up: bool,
}

/// The message a jump went to, while it's marked: the app places it with
/// `list::operation::jump_to` and lets it go 2.5 s later, when its light
/// fades (`show_message`, `src/ui/messages.rs:430`-`:460`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Jump {
    pub key: u64,
    /// Lit for a moment, as a reply's click lands on it; the first unread
    /// message is brought to the top unlit.
    pub lit: bool,
}

/// The keyboard: Up and Down step through the messages, letters act on the
/// one stepped to (`timeline-keys`, `ui/yapfast.slint:3654`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    /// The keyboard's message (`active-item`).
    pub active: Option<u64>,
    /// The timeline has the keyboard: its message shows lit with a
    /// marigold edge (`browsing`).
    pub browsing: bool,
    /// What the keys may do to the keyboard's message.
    pub may: May,
    /// Its deletion waits for Enter, or Escape (`confirm-delete`).
    pub confirming: bool,
}

/// What the keys may do to a message; nothing to one deleted or still
/// being sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct May {
    /// Enter, the Menu key or Shift+F10 open its menu.
    pub menu: bool,
    /// `+` opens the reaction picker.
    pub react: bool,
    /// R replies, E edits, Delete deletes, C copies its text, F forwards.
    pub reply: bool,
    pub edit: bool,
    pub delete: bool,
    pub copy: bool,
    pub forward: bool,
    /// Alt+Enter marks it unread, as Discord's key does.
    pub mark_unread: bool,
}

/// The first and last row in view, by index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub first: usize,
    pub last: usize,
}

/// What the list tells as its rows in view change, at most every 50 ms.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    pub span: Span,
    /// The keys of the rows in view and of six rows each side, in order:
    /// the messages to load pictures for and to count as seen.
    pub keys: Vec<u64>,
    /// The key of the first row in view, to come back to.
    pub top: u64,
    /// The newest row's bottom is in view.
    pub at_end: bool,
    /// The list follows its end: rows that come stay in view, which reads
    /// the conversation while the window is in front
    /// (`timeline-at-end`, `reading-latest`).
    pub following: bool,
    /// The list nears its start and earlier messages may load: load them.
    pub earlier: bool,
    /// It nears the end of a window of older messages: load newer ones.
    pub later: bool,
}

/// The timeline: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The rows in view changed (`media-viewport`). The app keeps the span
    /// in [`Timeline::shown`], asks for what shows, and loads more where
    /// [`Shown::earlier`] or [`Shown::later`] says.
    Shown(Shown),
    /// Load earlier messages: the edge's button, or Retry (`earlier`).
    Earlier,
    /// Load newer ones past a window of older messages (`later`).
    Later,
    /// The bar over the messages: to the "New" divider (`jump-to-unread`).
    JumpToUnread,
    /// The bar under a window of older messages: back to the newest
    /// (`jump-to-present`).
    JumpToPresent,
    /// The timeline took the keyboard, or lost it (`browsing`).
    Browsing(bool),
    /// The keyboard stepped to the message with this key.
    Step(u64),
    /// Escape while stepping: back to the composer (`focus-composer`).
    Leave,
    /// A key acting on the keyboard's message, by its key.
    Act(u64, Act),
}

/// What a key does to the keyboard's message.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    /// Its menu, beside it (`keyboard-popup: "menu"`).
    Menu(Spot),
    /// The reaction picker, beside it.
    React(Spot),
    Reply,
    Edit,
    /// Asks before deleting it, as the menu's Delete does.
    Delete,
    Copy,
    Forward,
    MarkUnread,
    /// Enter while its deletion waits: delete it.
    ConfirmDelete,
    /// Escape while it waits: keep it.
    CancelDelete,
}

impl Timeline {
    /// The rows reach the newest message: a window of older ones doesn't.
    pub fn detached(&self) -> bool {
        self.later.is_some()
    }

    /// Earlier messages may load when the list nears its start
    /// (`wants-earlier`, `ui/yapfast.slint:1922`).
    pub fn may_load_earlier(&self) -> bool {
        self.ready && !self.locked && !self.rows.is_empty() && self.earlier == Edge::More
    }

    /// Newer ones may load as a window of older ones nears its end
    /// (`wants-later`, `:1930`).
    pub fn may_load_later(&self) -> bool {
        self.ready && !self.locked && !self.rows.is_empty() && self.later == Some(Edge::More)
    }

    /// The row the "New" divider is over, by index (`unread-index`).
    pub fn unread_index(&self) -> Option<usize> {
        self.rows.iter().position(|row| row.new)
    }

    /// Where the "New" divider is while the bar that jumps to it shows: out
    /// of view above or below, until the newest message is read
    /// (`unread-above`, `unread-below`, `ui/yapfast.slint:1917`).
    pub fn unread_bar(&self) -> Option<Out> {
        self.unread.as_ref().filter(|unread| !unread.caught_up)?;
        let index = self.unread_index()?;
        let span = self.shown?;
        if index < span.first {
            Some(Out::Above)
        } else if index > span.last {
            Some(Out::Below)
        } else {
            None
        }
    }

    /// The index of the row with `key`.
    pub fn index_of(&self, key: u64) -> Option<usize> {
        self.rows.iter().position(|row| row.key == key)
    }

    /// Whether the person reads the conversation as `shown` shows it, the
    /// window in `front`: the newest message in view and followed, or a
    /// jump's message placed with the list at the newest's end, as a short
    /// unread stretch's divider is (`reading-latest`, `ui/yapfast.slint:1818`).
    /// The app tells the core when this changes.
    pub fn reads(&self, shown: &Shown, front: bool) -> bool {
        let at_newest = match self.jump {
            None => shown.following,
            Some(_) => !self.detached() && shown.at_end,
        };
        front && self.room.is_some() && at_newest
    }
}

/// Where something out of view is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Above,
    Below,
}

#[cfg(test)]
mod tests;
