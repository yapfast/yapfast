//! Samples of the inbox popout of mentions and unreads for tests and shots,
//! after the fake Discord's data: task N08's.

use crate::Vm;
use crate::chrome::Chrome;
use crate::fixtures::pictures::pictured;
use crate::fixtures::{avatar, chrome, shell};
use crate::inbox::{Inbox, Row, Tab};
use crate::kind::count_badge;
use crate::shell::Layer;

fn channel(id: &str, name: &str, place: &str, tint: u32, mentions: u64) -> Row {
    Row {
        id: id.into(),
        name: format!("#{name}"),
        place: place.into(),
        avatar: avatar(&name[..1].to_uppercase(), tint),
        badge: count_badge(mentions),
    }
}

fn dm(id: &str, name: &str, tint: u32, mentions: u64) -> Row {
    Row {
        id: id.into(),
        name: name.into(),
        place: String::new(),
        avatar: pictured(avatar(&name[..1], tint), name),
        badge: count_badge(mentions),
    }
}

/// The unreads tab: conversations and channels, some with mentions.
pub fn unreads() -> Inbox {
    Inbox {
        tab: Tab::Unreads,
        rows: vec![
            dm("dm-alice", "Alice", 0x5865f2, 2),
            dm("dm-bob", "Bob", 0xeb459e, 0),
            channel("general", "general", "Server", 0x3ba55d, 0),
            channel("random", "random", "Server", 0xfaa61a, 120),
            dm("dm-carol", "Carol", 0xed4245, 0),
        ],
    }
}

/// The mentions tab: only what mentions this account.
pub fn mentions() -> Inbox {
    let mut inbox = unreads();
    inbox.tab = Tab::Mentions;
    inbox.rows.retain(|row| !row.badge.is_empty());
    inbox
}

/// A tab with nothing in it.
pub fn empty(tab: Tab) -> Inbox {
    Inbox {
        tab,
        rows: Vec::new(),
    }
}

/// The inbox open over the DM home with `inbox`.
pub fn open(inbox: Inbox) -> Vm {
    shell::open(
        Vm {
            inbox,
            chrome: Chrome {
                inbox_open: true,
                ..chrome::framed()
            },
            ..shell::dm_home()
        },
        Layer::Inbox,
    )
}
