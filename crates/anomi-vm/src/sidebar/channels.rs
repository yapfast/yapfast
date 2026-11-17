//! A server's channel list: each category's heading above its channels.
//! The Slint side: `ChannelRow` and `CategoryHeading` in `ui/rail.slint`, and
//! the channel half of `show_sidebar` in `src/ui/refresh.rs`. Task N04
//! (docs/iced/tasks/shell.md) takes it further.

/// The channels of the server shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Channels {
    pub rows: Vec<ChannelRow>,
}

/// A channel, or a category's heading above its channels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelRow {
    pub id: String,
    pub name: String,
    /// The category this row starts, shown above it; empty when it starts none.
    pub heading: String,
    pub kind: ChannelKind,
    pub selected: bool,
    pub unread: bool,
    pub badge: String,
    pub muted: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChannelKind {
    #[default]
    Text,
    Voice,
    Forum,
    Thread,
}

/// What the channel list asks for. Keep `#[non_exhaustive]`: the app matches
/// it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Open a channel, by room id.
    Open(String),
}
