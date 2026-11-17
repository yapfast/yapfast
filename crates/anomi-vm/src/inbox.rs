//! The inbox popout of mentions and unreads: what it shows and what it asks
//! for. Task N08 (docs/iced/tasks/shell.md). The Slint side: `InboxPopout`
//! in `ui/yapfast/inbox.slint`, its rows from `show_inbox` in
//! `src/ui/refresh.rs`.

use crate::kind::Avatar;

/// Discord's first two tabs of the popout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    /// The channels and conversations with unread messages (a muted one
    /// only for a mention).
    #[default]
    Unreads,
    /// Those that mention this account.
    Mentions,
}

/// A channel or conversation with something waiting.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// The room's id, which opening it sends back.
    pub id: String,
    /// A conversation's name, or a channel's with its `#`.
    pub name: String,
    /// The server a channel belongs to; empty for a conversation.
    pub place: String,
    /// A conversation's picture; a channel keeps its initial on a tint.
    pub avatar: Avatar,
    /// How many times it mentions this account, as a badge writes it
    /// (`kind::count_badge`); empty when none.
    pub badge: String,
}

/// The inbox popout of mentions and unreads: what its screen draws. The
/// window holds it as `vm.inbox`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inbox {
    pub tab: Tab,
    /// The open tab's rows, the app's to fill when the tab changes.
    pub rows: Vec<Row>,
}

/// The inbox popout of mentions and unreads: what a click or a key on it asks
/// for. Closing is the shell's (Escape, a press outside). Keep
/// `#[non_exhaustive]`: the app matches it with a wildcard until its I task
/// handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Show another tab.
    Tab(Tab),
    /// Open this room (its id) and close the popout.
    Open(String),
}
