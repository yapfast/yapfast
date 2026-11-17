//! Samples of the DM list. Task N03's.

use yapfast_lang::{tr, trn};

use crate::fixtures::avatar;
use crate::fixtures::kind::{bob_tag, carol_activity, lounge};
use crate::fixtures::pictures::{self, pictured};
use crate::kind::{Doing, Expected, NameStyle, Presence, Status};
use crate::sidebar::dms::{DmRow, Dms, Empty, Voice};

fn dm(id: &str, name: &str, tint: u32, presence: Presence, detail: &str) -> DmRow {
    DmRow {
        id: id.into(),
        name: name.into(),
        avatar: pictured(avatar(&name[..1], tint), name),
        presence,
        detail: detail.into(),
        ..DmRow::default()
    }
}

/// Bob (his server tag, playing Factorio, in the Lounge), a group and six
/// people, Person 5 open.
pub fn dms() -> Dms {
    let mut rows = vec![
        dm(
            "discord:d1",
            "Bob",
            0xc2554d,
            Presence::DoNotDisturb,
            &tr!("Playing {}", "Factorio"),
        ),
        dm(
            "discord:d2",
            "Weekend plans",
            0xb86e22,
            Presence::None,
            &trn!("{n} member", "{n} members", 3),
        ),
    ];
    for person in (0..6).rev() {
        let name = format!("Person {person}");
        rows.push(dm(
            &format!("discord:p{person}"),
            &name,
            0x3a9357,
            Presence::Offline,
            "",
        ));
    }
    let mut tag = bob_tag();
    tag.badge = Expected::Ready(pictures::picture(
        "/clan-badges/1400000000000000100/bobtag.png",
    ));
    rows[0].tag = Some(tag);
    rows[0].voice = Some(Voice {
        room: "discord:v1".into(),
        spot: lounge(),
    });
    rows[2].selected = true;
    Dms {
        requests_badge: "1".into(),
        rows,
        ..Dms::default()
    }
}

/// Every state a row has, for the shots and tests that look at them: a
/// pinned DM, one with a call going and mentions, a muted one, Discord's
/// own, a person in a voice channel with nothing else said, one to wave at,
/// and Carol, with her words, her emoji and her styled name.
pub fn states() -> Dms {
    let mut rows = vec![
        dm(
            "discord:d3",
            "Pinned friend",
            0x4a7bd0,
            Presence::Online,
            "",
        ),
        dm(
            "discord:d4",
            "Call in progress",
            0x8a5bd0,
            Presence::Idle,
            "",
        ),
        dm(
            "discord:d5",
            "Muted channel",
            0x707070,
            Presence::Mobile,
            "",
        ),
        dm("discord:d6", "Discord", 0x5865f2, Presence::None, ""),
        dm(
            "discord:d7",
            "Alone in voice",
            0x2d9f8a,
            Presence::Online,
            "",
        ),
        dm("discord:d8", "Newcomer", 0xd0a24a, Presence::Offline, ""),
        dm("discord:d9", "Carol", 0xd05b9b, Presence::Idle, ""),
    ];
    rows[0].pinned = true;
    rows[1].call = true;
    rows[1].unread = true;
    rows[1].badge = "3".into();
    rows[2].muted = true;
    rows[3].official = true;
    rows[3].discord = true;
    rows[4].voice = Some(Voice {
        room: "discord:v1".into(),
        spot: lounge(),
    });
    rows[5].wave = "discord:u8".into();
    rows[6].status = Status {
        doing: Doing::Nothing,
        emoji: Expected::Ready(pictures::badge("carol-status", 16)),
        ..carol_activity()
    };
    rows[6].name_style = NameStyle::new(3, 2, &[0xff7eb3, 0x7a5cff]);
    Dms {
        requests_badge: "12".into(),
        rows,
        ..Dms::default()
    }
}

/// `count` people, for a list that only draws what is near the view.
pub fn many(count: usize) -> Dms {
    let rows = (0..count)
        .map(|at| {
            dm(
                &format!("discord:m{at}"),
                &format!("Contact {at}"),
                0x3a9357,
                Presence::Offline,
                "",
            )
        })
        .collect();
    Dms {
        rows,
        ..Dms::default()
    }
}

/// No DMs at all, before the first sync or the search matching none.
pub fn none(why: Empty) -> Dms {
    Dms {
        empty: why,
        ..Dms::default()
    }
}

/// A status with words but no icon or emoji.
pub fn plain_status(text: &str) -> Status {
    Status {
        text: text.into(),
        ..Status::default()
    }
}
