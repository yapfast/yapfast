//! The shots of a bot message's components: the "Discord" app's notice in
//! its DM (`dm/discord_components_notice`), and the status bot's controls
//! outside a container, with the failed-interaction line.

use anomi_vm::Vm;
use anomi_vm::chat::timeline::Timeline;
use anomi_vm::fixtures::chat::components as fixtures;
use anomi_vm::fixtures::pictures::pictured;
use anomi_vm::fixtures::{avatar, shell};
use anomi_vm::header::{Friend, Header, Title};
use anomi_vm::kind::Presence;
use anomi_vm::sidebar::Place;
use anomi_vm::sidebar::dms::DmRow;

use crate::shots::{GOLDEN, Shot};

/// The DM home with `timeline` open, its row selected; the app's DM listed
/// under Bob's when it isn't there, and named in the header.
fn dm(timeline: Timeline) -> Vm {
    let mut vm = shell::dm_home();
    let (room, name) = timeline
        .room
        .as_ref()
        .map(|room| (room.id.clone(), room.name.clone()))
        .unwrap_or_default();
    let picture = pictured(avatar(&name[..1.min(name.len())], 0x5865f2), &name);
    if let Place::Dms(dms) = &mut vm.sidebar.place {
        if !dms.rows.iter().any(|row| row.id == room) {
            let row = DmRow {
                id: room.clone(),
                name: name.clone(),
                avatar: picture.clone(),
                presence: Presence::Offline,
                ..DmRow::default()
            };
            dms.rows.insert(1, row);
        }
        for row in &mut dms.rows {
            row.selected = row.id == room;
        }
    }
    if let Header::Open(place) = &mut vm.header
        && let Title::Direct(direct) = &mut place.title
    {
        direct.name = name;
        direct.avatar = picture;
        // An app isn't befriended.
        place.tools.friend = Friend::None;
    }
    vm.chat.timeline = timeline;
    vm
}

pub fn shots() -> Vec<Shot> {
    vec![
        Shot {
            area: "dm",
            name: "discord_components_notice",
            size: GOLDEN,
            theme: "bonfire",
            vm: || dm(fixtures::notice_timeline()),
        },
        Shot {
            area: "components",
            name: "controls",
            size: GOLDEN,
            theme: "bonfire",
            vm: || dm(fixtures::deploy_timeline()),
        },
    ]
}
