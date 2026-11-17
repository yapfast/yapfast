//! Samples of the server's header. Task N05's.

use crate::fixtures::pictures;
use crate::kind::Expected;
use crate::sidebar::server_header::ServerHeader;

/// The first server's header in the golden window: its name and the invite
/// button, nothing under them.
pub fn server_header() -> ServerHeader {
    ServerHeader {
        name: "Server".into(),
        invite: true,
        ..ServerHeader::default()
    }
}

/// A Community server with its banner, a boost goal under way and events.
pub fn community() -> ServerHeader {
    ServerHeader {
        name: "Server".into(),
        community: true,
        banner: Expected::Ready(pictures::picture("/banners/server")),
        invite: true,
        boosts: Some(12),
        events: Some(2),
        ..ServerHeader::default()
    }
}

/// A verified server, its menu open, with no events yet.
pub fn verified() -> ServerHeader {
    ServerHeader {
        name: "Server".into(),
        verified: true,
        open: true,
        invite: true,
        boosts: Some(40),
        events: Some(0),
        ..ServerHeader::default()
    }
}
