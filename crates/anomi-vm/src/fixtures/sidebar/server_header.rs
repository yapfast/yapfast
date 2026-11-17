//! Samples of the server's header. Task N05's.

use crate::sidebar::server_header::ServerHeader;

pub fn server_header() -> ServerHeader {
    ServerHeader {
        name: "Server".into(),
    }
}
