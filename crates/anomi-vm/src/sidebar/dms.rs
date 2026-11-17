//! The DM home's list: Friends and Requests, then the DMs. The Slint side:
//! `RoomRow` in `ui/rail.slint` (the DM rows), `DmHeading` and the DM half of
//! `show_sidebar` in `src/ui/refresh.rs`. Task N03 (docs/iced/tasks/shell.md)
//! takes it further.

use crate::kind::{Avatar, Presence};

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

/// What the DM list asks for. Keep `#[non_exhaustive]`: the app matches it
/// with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// Open a DM, by room id.
    Open(String),
    Friends,
    Requests,
}
