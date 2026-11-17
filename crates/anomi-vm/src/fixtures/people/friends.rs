//! Samples of the Friends screen for tests and shots, after the fake
//! Discord's data: task P01's. Bob (do not disturb, playing Factorio, in the
//! Lounge) and Carol (idle, her status, in the Lounge) are around; Person 0,
//! 2 and 4 are not.

use crate::fixtures::avatar;
use crate::fixtures::kind::lounge;
use crate::fixtures::pictures::pictured;
use crate::kind::{Expected, Presence, Status};
use crate::people::friends::{FriendRow, Friends, Voice};
use yapfast_lang::tr;

fn friend(id: &str, name: &str, tint: u32, presence: Presence) -> FriendRow {
    FriendRow {
        id: id.into(),
        name: name.into(),
        avatar: pictured(avatar(&name[..1], tint), name),
        presence,
        ..FriendRow::default()
    }
}

fn in_lounge(status: Status, mut row: FriendRow) -> FriendRow {
    row.status = status;
    row.voice = Some(Voice {
        room: "discord:v1".into(),
        spot: lounge(),
    });
    row
}

/// A friend's status as the list shows it: one line of words, `activity`'s,
/// with no icon for what they do.
fn said(text: &str, emoji: Expected) -> Status {
    Status {
        text: text.into(),
        emoji,
        ..Status::default()
    }
}

fn around() -> Vec<FriendRow> {
    vec![
        in_lounge(
            said(&tr!("Playing {}", "Factorio"), Expected::None),
            friend(
                "1400000000000000002",
                "Bob",
                0xc2554d,
                Presence::DoNotDisturb,
            ),
        ),
        in_lounge(
            said("🌙 back later after dinner, probably", Expected::None),
            friend("1400000000000000003", "Carol", 0x3a9357, Presence::Idle),
        ),
    ]
}

fn others() -> Vec<FriendRow> {
    [0, 2, 4]
        .into_iter()
        .map(|n| {
            friend(
                &format!("160000000000000000{n}"),
                &format!("Person {n}"),
                0x3a9357,
                Presence::Offline,
            )
        })
        .collect()
}

/// The Online tab (`friends/online_tab_active_now`): those around.
pub fn online() -> Friends {
    Friends {
        rows: around(),
        total: 5,
        ..Friends::default()
    }
}

/// The All tab (`friends/all_tab`): everyone, those around first.
pub fn all() -> Friends {
    let mut rows = around();
    rows.extend(others());
    Friends {
        rows,
        all: true,
        total: 5,
        ..Friends::default()
    }
}

/// Online with nobody around.
pub fn nobody_around() -> Friends {
    Friends {
        total: 5,
        ..Friends::default()
    }
}

/// A search that matches no friend.
pub fn no_match() -> Friends {
    Friends {
        all: true,
        total: 5,
        filter: "zed".into(),
        ..Friends::default()
    }
}

/// A row of every kind of line under a name: a voice channel alone, a
/// status alone, both, and none.
pub fn lines() -> Friends {
    let mut rows = all().rows;
    rows[0].status = Status::default();
    rows[1].voice = None;
    Friends {
        rows,
        all: true,
        total: 5,
        ..Friends::default()
    }
}

/// No friends yet: the welcome.
pub fn none() -> Friends {
    Friends::default()
}
