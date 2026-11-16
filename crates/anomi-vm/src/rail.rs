//! The server rail: the DM home, then the servers, then adding one.
//! The Slint side: `ui/rail.slint` (`ServerRow`, `RailItem`) and `rail_rows`
//! in `src/ui/refresh.rs`.

use crate::kind::Avatar;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rail {
    /// The DM home's tile: selected while DMs show.
    pub home: Tile,
    pub servers: Vec<Server>,
}

/// What every rail item shows beside its picture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    pub selected: bool,
    pub unread: bool,
    /// Mentions waiting, such as "3" or "99+"; empty for none.
    pub badge: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Server {
    /// The core's server id.
    pub id: String,
    /// Its name, for the tooltip.
    pub name: String,
    /// Up to three initials, shown until its icon arrives.
    pub avatar: Avatar,
    pub tile: Tile,
    /// This account is in a call in one of its voice channels.
    pub call: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    /// Show the DMs.
    Home,
    /// Show a server, by id.
    Server(String),
}
