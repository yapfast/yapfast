//! Samples of adding a friend for tests and shots, after the fake Discord's
//! data: task P03's.

use crate::people::add_friend::{AddFriend, Note};

/// The dialog as it opens: an empty field (the golden screenshot's).
pub fn empty() -> AddFriend {
    AddFriend::default()
}

/// A name typed, ready to send.
pub fn typed() -> AddFriend {
    AddFriend {
        name: "bob".into(),
        ..AddFriend::default()
    }
}

/// The request is on its way.
pub fn sending() -> AddFriend {
    AddFriend {
        note: Note::Sending,
        ..typed()
    }
}

/// Discord took it; the name is cleared for the next.
pub fn sent() -> AddFriend {
    AddFriend {
        name: String::new(),
        note: Note::Sent("bob".into()),
    }
}

/// Discord refused: no one by that name.
pub fn failed() -> AddFriend {
    AddFriend {
        note: Note::Failed("Unknown user".into()),
        ..typed()
    }
}
