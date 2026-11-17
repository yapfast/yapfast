//! Samples of the DM list. Task N03's.

use yapfast_lang::{tr, trn};

use crate::fixtures::avatar;
use crate::kind::Presence;
use crate::sidebar::dms::{DmRow, Dms};

fn dm(id: &str, name: &str, tint: u32, presence: Presence, detail: &str) -> DmRow {
    DmRow {
        id: id.into(),
        name: name.into(),
        avatar: avatar(&name[..1], tint),
        presence,
        detail: detail.into(),
        ..DmRow::default()
    }
}

/// Bob, a group and six people, Person 5 open and Bob unread.
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
    rows[2].selected = true;
    rows[0].unread = true;
    Dms {
        requests_badge: "1".into(),
        rows,
        ..Dms::default()
    }
}
