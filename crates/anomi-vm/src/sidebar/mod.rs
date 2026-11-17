//! The column beside the rail: the DM list, or a server's header and
//! channels, under a search field. The Slint side: the sidebar in
//! `ui/yapfast.slint`, the lists in `ui/rail.slint`, and `show_sidebar` in
//! `src/ui/refresh.rs`.
//!
//! F01 keeps this file: [`Sidebar`], [`Place`] and the nested [`Intent`].
//! Its parts are their tasks': [`dms`] (N03), [`channels`] (N04) and
//! [`server_header`] (N05).

pub mod channels;
pub mod dms;
pub mod server_header;

#[derive(Clone, Debug, Default)]
pub struct Sidebar {
    pub place: Place,
    /// What the search field at the top holds; the rows are already
    /// filtered by it.
    pub filter: String,
    /// Changes whenever the rows do, so a view can tell.
    pub version: u64,
}

/// What the sidebar lists.
#[derive(Clone, Debug)]
pub enum Place {
    Dms(dms::Dms),
    Server(Server),
}

impl Default for Place {
    fn default() -> Self {
        Self::Dms(dms::Dms::default())
    }
}

/// A server: its header above its channels.
#[derive(Clone, Debug, Default)]
pub struct Server {
    pub header: server_header::ServerHeader,
    pub channels: channels::Channels,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// The search field's text changed.
    Filter(String),
    Dms(dms::Intent),
    Channels(channels::Intent),
    ServerHeader(server_header::Intent),
}

crate::parts!(Intent in Sidebar:
    Dms(dms::Intent),
    Channels(channels::Intent),
    ServerHeader(server_header::Intent),
);
