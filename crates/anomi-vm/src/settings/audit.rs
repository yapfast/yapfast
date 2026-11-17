//! A server's audit log: what it shows and what it asks for. Task X07
//! (docs/iced/tasks/settings.md). The Slint side: `AuditLogSection` over
//! `AuditLogPrompt` (`ui/moderation.slint:330`, `:77`).

use crate::kind::Avatar;
use crate::text::Text;

/// Which way an entry changed things; it colours its lines' numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// Made something: the live colour.
    Create,
    /// Changed it: the accent.
    #[default]
    Update,
    /// Removed it: danger.
    Delete,
}

/// A line of an entry, or an item under one, such as a permission.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// "01" and on; empty for an item.
    pub number: String,
    pub text: Text,
}

/// A row of the log: who did what, and when.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub actor: Avatar,
    /// The sentence, its bold parts in bold.
    pub title: Text,
    pub time: String,
    pub kind: Kind,
    /// What opens under the row; none, and the row doesn't open.
    pub lines: Vec<Line>,
}

impl Row {
    pub fn expandable(&self) -> bool {
        !self.lines.is_empty()
    }
}

/// A server's audit log: what its screen draws. The window holds it as
/// `vm.settings.audit`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Audit {
    /// Newest first.
    pub rows: Vec<Row>,
    /// The row whose lines show, by its place.
    pub open: Option<usize>,
    /// The filters' choices, everyone and every action first, and the
    /// chosen ones.
    pub users: Vec<String>,
    pub user: usize,
    pub actions: Vec<String>,
    pub action: usize,
    pub fetching: bool,
    /// Discord may have older entries.
    pub older: bool,
    /// Discord didn't send the newest.
    pub failed: bool,
}

impl Audit {
    /// Whether the end of the list should ask for older entries now.
    pub fn wants_older(&self) -> bool {
        self.older && !self.fetching
    }
}

/// A server's audit log: what a click or a key on it asks for. Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Filter by the user at this place of the filter, 0 for everyone.
    User(usize),
    /// Filter by the action at this place, 0 for every action.
    Action(usize),
    /// Load older entries: the button at the list's end.
    Older,
    /// The list scrolled or its rows changed; `near_end`, its end is less
    /// than a view's height below, so older entries load as Discord's do
    /// when [`Audit::wants_older`]. Sent only while that holds.
    Scrolled { near_end: bool },
    /// Open or close the row at this place.
    Toggle(usize),
}
