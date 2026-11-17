//! Samples of the shared vocabulary (tags, statuses, a voice spot) for tests
//! and shots, after the fake Discord's data: task F14's. They hold no
//! pictures; shots fill the badge and the emoji. The DM list and member list
//! fixtures take the row form of a status ([`carol_activity`], one line, as
//! Slint's rows flatten it); profiles take [`carol_status`].

use crate::kind::{Doing, Expected, Spot, Status, Tag};

/// Bob's server tag, with its server's id and its badge on its way.
pub fn bob_tag() -> Tag {
    Tag {
        text: "BOB".into(),
        badge: Expected::Waiting,
        server: "discord:g1".into(),
    }
}

/// Bob playing Factorio.
pub fn bob_status() -> Status {
    Status {
        text: "Factorio".into(),
        doing: Doing::Playing,
        ..Status::default()
    }
}

/// Carol listening to music, with a custom status and its emoji expected, as
/// a profile shows it, with its line break.
pub fn carol_status() -> Status {
    Status {
        text: "back later\nafter dinner, probably".into(),
        emoji: Expected::Waiting,
        doing: Doing::Listening,
    }
}

/// Carol's status as a row shows it: the same, on one line (`one_line`).
pub fn carol_activity() -> Status {
    Status {
        text: "back later after dinner, probably".into(),
        ..carol_status()
    }
}

/// A voice channel on the first server.
pub fn lounge() -> Spot {
    Spot::new("Lounge", "Server")
}
