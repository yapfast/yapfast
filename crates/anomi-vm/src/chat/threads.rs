//! The threads popover: what it shows and what it asks for. Task C23
//! (docs/iced/tasks/chat.md). The Slint side: `ThreadsPopover`
//! (`ui/header.slint:406`) over the rows `show_threads` builds
//! (`src/ui/refresh.rs:1921`).
//!
//! A channel's list of its threads, under the header's Threads button, as
//! Discord's client has it: the active threads the account is in, the other
//! active ones and older ones, a page at a time, or what a search of their
//! names found (which stand in for the sections, without headings). The app
//! words everything the list says (headings, "Started by ", "5m ago"), as
//! `show_threads` does; the view words only its own chrome.

use iced::Color;

use crate::kind::Avatar;

/// Each thread's row is a 72 px card in 80 px (`ThreadEntry`), the first
/// section's heading 32 px tall and later ones 40 px, 16 px above a
/// search's first result, and the header 48 px with its line.
const CARD: f32 = 80.0;
const FIRST_HEADING: f32 = 32.0;
const HEADING: f32 = 40.0;
const FIRST_CARD: f32 = 16.0;
const HEADER: f32 = 49.0;
const LOADING: f32 = 34.0;

/// The threads popover: what its screen draws. The window holds it as
/// `vm.chat.threads`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Threads {
    /// The list: headings and the threads under them, in order.
    pub rows: Vec<Row>,
    /// Said instead of the list: while it loads, or why it failed.
    pub note: String,
    /// There are none to show.
    pub empty: bool,
    /// The threads' names can be searched (Read Message History).
    pub searchable: bool,
    /// What the search field holds.
    pub query: String,
    /// The search field has the keys.
    pub focused: bool,
    /// Older threads are still to load.
    pub more: bool,
    /// A page is on its way.
    pub loading: bool,
    /// Threads can start in the channel.
    pub can_create: bool,
}

/// A line of the list.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// "1 JOINED THREAD", "OLDER THREADS": already upper-case.
    Heading(String),
    Thread(Box<Thread>),
}

/// A thread as a card: its name, then its newest message, or who started an
/// archived one, and when it was last active; on the right a few of those in
/// it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Thread {
    /// The thread's room id.
    pub room: String,
    pub name: String,
    pub private: bool,
    pub archived: bool,
    /// Who the line names, with their avatar and role colour: the newest
    /// message's author, or an archived thread's owner.
    pub person: Option<Person>,
    /// Before the person, such as "Started by ".
    pub lead: String,
    /// After the person: the message's text, or for no one, what to say.
    pub text: String,
    /// When it was last active, as "5m ago" or "Last active 5m ago".
    pub time: String,
    /// How many are in it (none drawn for 0), and a few of their avatars.
    pub members: u32,
    pub member_avatars: Vec<Avatar>,
}

/// The one a thread's line names.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Person {
    pub name: String,
    pub avatar: Avatar,
    /// Their role's colour, if it has one.
    pub color: Option<Color>,
}

impl Threads {
    /// How tall the popover would be with every row showing, header
    /// included (`fit`): the window clamps it between 400 px and 80 % of its
    /// height. The rows and the loading line count whatever a note says, as
    /// in Slint: `show_threads` keeps the sections while it searches.
    pub fn fit(&self) -> f32 {
        let mut height = HEADER;
        for (at, row) in self.rows.iter().enumerate() {
            height += match (row, at) {
                (Row::Heading(_), 0) => FIRST_HEADING,
                (Row::Heading(_), _) => HEADING,
                (Row::Thread(_), 0) => FIRST_CARD + CARD,
                (Row::Thread(_), _) => CARD,
            };
        }
        if self.loading {
            height += LOADING;
        }
        height
    }
}

/// Where a thread was asked for from: the header's Create, or the empty
/// list's Create Thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadFrom {
    List,
    ListEmpty,
}

/// The threads popover: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The search field was edited to this; empty shows the list again.
    Edit(String),
    /// Enter in the search field: search the threads' names for this.
    Search(String),
    /// Open a thread, in place of the channel or, with Shift, beside it.
    Open { room: String, beside: bool },
    /// The list's view moved, `offset` px from its start. Older threads
    /// load when the person scrolled it (`offset` above 0) near its end
    /// (`near_end`) while `more` holds and no page is on its way; a list
    /// too short to scroll loads no more, as in Slint.
    Scrolled { offset: f32, near_end: bool },
    /// The search field got or lost the keys.
    Focus(bool),
    /// Start a thread.
    Create(ThreadFrom),
    /// Close the popover (Escape and a press outside are the shell's).
    Close,
}
