//! Searching messages: what it shows and what it asks for. Task C21
//! (docs/iced/tasks/chat.md). The Slint side: `HeaderSearch`, `SearchPanel`
//! and `FoundCard` (`ui/header.slint:65`, `:647`, `:158`), whose rows the app
//! made in `show_hits` and `found_row` (`src/ui/refresh.rs`).
//!
//! [`Found`] is a message found, as a search's results and a DM's pinned
//! messages (C22's popover) show it: its author, when, what it says, its
//! first pictures laid out as the timeline's gallery but smaller, its other
//! files named, and a poll it asks, read only.

use iced::Color;

use crate::kind::{Avatar, Expected};
use crate::text::{Link, Text};

/// Searching messages: what its screen draws. The window holds it as
/// `vm.chat.search`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Search {
    /// The field's text (`HeaderSearch.text`).
    pub text: String,
    /// "Search: Server" or the DM's own (`placeholder`).
    pub placeholder: String,
    /// The field has the keys: it opens to its full width (`has-focus`).
    pub focused: bool,
    /// A header with little room shows the magnifier alone until the field
    /// is used (`compact`).
    pub compact: bool,
    /// How many it found, or that it's searching, or why it couldn't.
    pub heading: String,
    /// Said above the results, as why none came.
    pub note: String,
    /// The newest first, a page at a time.
    pub results: Vec<Found>,
    /// A next page can be asked for.
    pub more: bool,
    /// The next page is on its way.
    pub loading: bool,
}

/// A message found (`FoundRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    /// Its id, which keeps its row through a next page.
    pub item: String,
    /// The room it is in, which a jump opens first.
    pub room: String,
    /// Above the first of a run in one channel of a server, the channel's
    /// name.
    pub heading: String,
    pub author: String,
    pub avatar: Avatar,
    /// The author's Discord role colour, when they have one.
    pub color: Option<Color>,
    /// The day and time it was sent.
    pub when: String,
    /// What it says: headings, quotes and code are in the text's blocks.
    pub body: Option<Text>,
    /// Its first pictures, placed in pixels.
    pub images: Vec<Cell>,
    /// The room its pictures take together.
    pub gallery: (f32, f32),
    /// Its other files, named, when it has any.
    pub files: String,
    /// The poll it asks, which can't be voted in from here.
    pub poll: Option<Poll>,
}

/// A picture in a found message's gallery (`ImageCell`), placed in pixels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cell {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// `Waiting` shows a skeleton at the size it will take.
    pub picture: Expected,
    /// It shows blurred under a SPOILER label that a click doesn't lift: it
    /// jumps to the message.
    pub spoiler: bool,
    /// A video, whose poster the picture is: a play mark lies over it.
    pub video: bool,
}

/// A Discord poll on a found message (`PollCard`, read only).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Poll {
    pub question: String,
    /// "Select one answer", while it takes votes.
    pub prompt: String,
    pub multiselect: bool,
    pub answers: Vec<Answer>,
    /// Each answer's votes show: this account voted or the poll closed.
    pub results: bool,
    /// "5 votes".
    pub total: String,
    /// "3h left" or "Poll closed".
    pub left: String,
}

/// An answer of a [`Poll`] (`PollAnswerRow`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Answer {
    pub text: String,
    pub emoji: String,
    /// Voted for by this account.
    pub picked: bool,
    /// "3 votes".
    pub votes: String,
    /// Its share of all of them.
    pub percent: u8,
    /// No answer has more votes.
    pub leading: bool,
}

/// Searching messages: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The field's text changed.
    Edit(String),
    /// Enter: search for the field's text (`search`).
    Submit,
    /// The cross ends the search (`clear`).
    Clear,
    /// Escape in a field that has the keys (`HeaderSearch.escape(cleared)`,
    /// answered with `leave-search`): the keys go to the composer. `cleared`
    /// is set when the field had text, which Escape ends.
    Leave { cleared: bool },
    /// The field got or lost the keys.
    Focus(bool),
    /// Show a result: its room and its item (`jump`).
    Jump { room: String, item: String },
    /// A link in a result's text (`open-link`).
    Link(Link),
    /// "Show more" (`more-results`).
    More,
    /// The panel's cross (`close`).
    Close,
}
