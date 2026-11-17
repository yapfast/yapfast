//! A Community server's Browse Channels: what it shows and what it asks for.
//! The Slint side: `BrowseRow` and `BrowseScreen` (`ui/browse.slint`), its
//! rows from `browse_rows` in `src/ui/refresh.rs`. Task N14's.

/// What a row is: a category's heading, or a channel of one kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// A category's heading, with its Follow category switch.
    Category,
    #[default]
    Text,
    Announcement,
    /// Its button says Join.
    Voice,
    /// Its button says Open.
    Forum,
}

/// A channel, or a category's heading.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// Its room id.
    pub id: String,
    pub name: String,
    /// What the channel is for, as plain text; empty draws no line.
    pub topic: String,
    pub kind: Kind,
    /// A channel's box is ticked, or a category's switch on.
    pub followed: bool,
    /// Followed through its category, so its box changes only with it.
    pub by_category: bool,
}

impl Row {
    pub fn is_category(&self) -> bool {
        self.kind == Kind::Category
    }
}

/// A Community server's Browse Channels: what its screen draws. The window
/// holds it as `vm.browse`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Browse {
    /// The channels whose names have the search in them, each under its
    /// category's heading, filtered by the app.
    pub rows: Vec<Row>,
    /// What the search field holds.
    pub filter: String,
}

/// A Community server's Browse Channels: what a click or a key on it asks
/// for. Keep `#[non_exhaustive]`: the app matches it with a wildcard until
/// its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The search field was edited: its whole text now.
    Filter(String),
    /// A box or switch was flipped: the room id, and whether to follow it.
    Follow { room: String, follow: bool },
    /// A channel's button was clicked, to open it: its room id.
    View(String),
}
