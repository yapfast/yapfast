//! Samples of the sidebar: the DM list, or the first server's channels.
//! F01 keeps this file; the parts are their tasks'.

pub mod channels;
pub mod dms;
pub mod server_header;

use crate::sidebar::{Place, Server, Sidebar};

/// The DM list, Person 5 open.
pub fn home() -> Sidebar {
    Sidebar {
        place: Place::Dms(dms::dms()),
        filter: String::new(),
        version: 1,
    }
}

/// The first server's channels, #general open.
pub fn server() -> Sidebar {
    Sidebar {
        place: Place::Server(Server {
            header: server_header::server_header(),
            channels: channels::channels(),
        }),
        filter: String::new(),
        version: 1,
    }
}
