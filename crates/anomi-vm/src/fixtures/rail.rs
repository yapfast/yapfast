//! Samples of the rail. Task N02's.

use crate::rail::{Rail, Server, Tile};

use super::avatar;
use super::pictures;

/// The rail with the DM home selected and three servers, the first unread.
pub fn rail() -> Rail {
    let server = |id: &str, name: &str, initials: &str, unread: bool| Server {
        id: id.into(),
        name: name.into(),
        avatar: crate::kind::Avatar {
            picture: pictures::server(name),
            ..avatar(initials, 0x5865f2)
        },
        tile: Tile {
            unread,
            ..Tile::default()
        },
        call: false,
    };
    Rail {
        home: Tile {
            selected: true,
            badge: "1".into(),
            ..Tile::default()
        },
        servers: vec![
            server("discord:g1", "Server", "S", true),
            server("discord:g2", "F2", "F2", false),
            server("discord:g3", "F3", "F3", false),
        ],
    }
}
