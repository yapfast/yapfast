//! Samples of the conversation column for tests and shots, after the fake
//! Discord's data: task N01's. The areas in it (the timeline, the member
//! list, the profile) fill their own fields.

use yapfast_lang::tr;

use crate::conversation::{Aside, Beside, Body, Call, Conversation, ForumPost};

/// A one-to-one DM: its messages, with the person's profile beside them.
pub fn dm() -> Conversation {
    Conversation {
        body: Body::Messages,
        beside: Beside {
            profile: true,
            ..Beside::default()
        },
        ..Conversation::default()
    }
}

/// A server's channel: its messages, with the member list beside them.
pub fn channel() -> Conversation {
    Conversation {
        body: Body::Messages,
        beside: Beside {
            members: true,
            ..Beside::default()
        },
        ..Conversation::default()
    }
}

/// Friends, from the DM home.
pub fn friends() -> Conversation {
    Conversation {
        body: Body::Friends,
        ..Conversation::default()
    }
}

/// A channel with a thread open beside it, in place of its member list.
pub fn thread_beside() -> Conversation {
    Conversation {
        beside: Beside {
            thread: Some(Aside::Thread),
            ..channel().beside
        },
        ..channel()
    }
}

/// A channel with search results beside it.
pub fn searching() -> Conversation {
    Conversation {
        beside: Beside {
            search: true,
            ..channel().beside
        },
        ..channel()
    }
}

/// A forum post open as the conversation, followed: its bar above it.
pub fn forum_post() -> Conversation {
    Conversation {
        forum_post: Some(ForumPost {
            reacts: true,
            following: Some(true),
        }),
        ..channel()
    }
}

/// A DM while a call goes on in it, not joined: the way to join above the
/// messages.
pub fn dm_call_elsewhere() -> Conversation {
    Conversation {
        call: Call::Elsewhere,
        ..dm()
    }
}

/// A DM in its call: the stage above the messages.
pub fn dm_call_here() -> Conversation {
    Conversation {
        call: Call::Here { focused: false },
        ..dm()
    }
}

/// The voice channel Lounge, joined alone: its stage fills the column,
/// under the line that says so.
pub fn voice() -> Conversation {
    Conversation {
        body: Body::Voice,
        call: Call::Here { focused: false },
        description: tr!("You're here alone. Stay as long as you like; others can join later.")
            .into(),
        ..Conversation::default()
    }
}

/// The local preview's sample data.
pub fn preview() -> Conversation {
    Conversation {
        preview: true,
        ..dm()
    }
}
