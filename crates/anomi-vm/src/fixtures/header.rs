//! Samples of the conversation's header. Task N07's.

use crate::header::Header;

use super::avatar;
use super::pictures::pictured;

/// Person 5's DM.
pub fn dm() -> Header {
    Header::Direct {
        name: "Person 5".into(),
        avatar: pictured(avatar("P", 0x3a9357), "Person 5"),
    }
}

/// #general of the first server.
pub fn channel() -> Header {
    Header::Channel {
        name: "general".into(),
        topic: "Co-op nights, plans and links.".into(),
    }
}
