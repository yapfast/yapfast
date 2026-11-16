//! The column beside the rail: the DM list, or a server's channels.
//! The Slint side: `RoomRow` in `ui/rail.slint`, the sidebar in
//! `ui/yapfast.slint`, and `show_sidebar` in `src/ui/refresh.rs`.

use crate::kind::{Avatar, Presence};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sidebar {
    pub place: Place,
    /// What the search field at the top holds; the rows are already
    /// filtered by it.
    pub filter: String,
    /// Changes whenever the rows do, so a view can tell.
    pub version: u64,
}

/// What the sidebar lists.
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    Dms(Dms),
    Server(Server),
}

impl Default for Place {
    fn default() -> Self {
        Self::Dms(Dms::default())
    }
}

/// The DM home: Friends and Requests, then the DMs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dms {
    pub friends_selected: bool,
    pub requests_selected: bool,
    /// Requests waiting, as a badge; empty for none.
    pub requests_badge: String,
    pub rows: Vec<DmRow>,
}

/// A DM in the list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DmRow {
    /// The core's room id.
    pub id: String,
    pub name: String,
    pub avatar: Avatar,
    /// A one-to-one DM's person's presence; none for a group.
    pub presence: Presence,
    /// The line under the name: their status or what they're doing, or a
    /// group's size.
    pub detail: String,
    pub selected: bool,
    pub unread: bool,
    /// Mentions waiting; empty for none.
    pub badge: String,
    pub muted: bool,
}

/// A server's channel list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Server {
    pub name: String,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    /// Open a DM or a channel, by room id.
    Open(String),
    /// The search field's text changed.
    Filter(String),
    Friends,
    Requests,
}
