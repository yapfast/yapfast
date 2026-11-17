//! Samples of the participant grid for tests and shots, after the fake
//! Discord's Lounge (`testing/fake_discord.ts:2134`) and Bob's DM call:
//! task V04's. Each golden screenshot of the stage has its stage here
//! ([`lounge`], [`speaking`], [`watching`], [`with_camera`], [`quiet`],
//! [`dm_call`]), the states the goldens don't show have theirs ([`full`],
//! [`others_hidden`], [`sharing`], [`alone`], [`crowd`]), and the windows
//! they show in ([`window`], [`dm_window`], [`full_window`]), with the
//! server's list as each golden has it ([`speaking_window`],
//! [`camera_window`], [`quiet_window`], [`sharing_window`],
//! [`alone_window`]). The tiles are V03's
//! (`fixtures::voice::tiles`).

use yapfast_lang::trn;

use crate::Vm;
use crate::conversation::{Call, Conversation};
use crate::fixtures::pictures::pictured;
use crate::fixtures::sidebar::channels::joined_window;
use crate::fixtures::{avatar, chat, conversation, header, shell};
use crate::header::{Friend, Header};
use crate::shell::Layer;
use crate::sidebar::Place;
use crate::sidebar::channels::VoicePerson;
use crate::voice::dock::Dock;
use crate::voice::stage::Stage;
use crate::voice::tiles::{Key, Mark, Outline, Tile};

use super::tiles::{self, person};
use super::{controls, dock};

const ALICE: &str = "1400000000000000001";
const BOB: &str = "1400000000000000002";

/// The stage of `tiles`, none large.
fn grid(tiles: Vec<Tile>) -> Stage {
    Stage::new(tiles, None, false, false)
}

/// The Lounge's stage once Alice joined: four tiles two by two, Bob's
/// stream offered in his (`voice/stage.png`).
pub fn lounge() -> Stage {
    grid(tiles::lounge())
}

/// Bob speaks (`voice/speaking_ring.png`).
pub fn speaking() -> Stage {
    grid(tiles::speaking())
}

/// The Lounge once Bob left: two over one (`light/voice_stage.png`).
pub fn quiet() -> Stage {
    grid(tiles::quiet())
}

/// Alice watches Bob's stream, large in his tile, the others in a row
/// beneath it (`voice/watching_stream.png`).
pub fn watching() -> Stage {
    Stage::new(tiles::watching(), Some(1), false, false)
}

/// Bob films too: his stream large in a tile of its own, his camera in the
/// row beneath (`voice/with_camera.png`).
pub fn with_camera() -> Stage {
    Stage::new(tiles::with_camera(), Some(0), false, false)
}

/// Bob's stream over the whole window, alone and square.
pub fn full() -> Stage {
    Stage::new(tiles::watching(), Some(1), true, false)
}

/// Bob's stream large with the others hidden from its button.
pub fn others_hidden() -> Stage {
    Stage::new(tiles::watching(), Some(1), false, true)
}

/// Alice shares her screen, live: it leads the five tiles, which take
/// columns of at least 240 px.
pub fn sharing() -> Stage {
    let mut tiles = vec![tiles::sharing()];
    tiles.extend(tiles::lounge());
    grid(tiles)
}

/// Alice alone in the Lounge, under the line that says so.
pub fn alone() -> Stage {
    grid(vec![tiles::alice()])
}

/// Alice and Bob in his DM's call, side by side
/// (`dmcall/connected_bob_dm.png`).
pub fn dm_call() -> Stage {
    grid(vec![
        tiles::alice(),
        person(BOB, "Bob", tiles::bob_avatar()),
    ])
}

/// A call of `count`: the Lounge's four, then Erin and people of the fake's
/// (`Person N`), one in three muted and one speaking. Ten fill a stage that
/// scrolls; the call Q03 measures against Slint's memory.
pub fn crowd(count: usize) -> Vec<Tile> {
    let mut tiles = tiles::lounge();
    tiles.extend((0..count.saturating_sub(4)).map(|index| {
        let name = match index {
            0 => "Erin".to_owned(),
            _ => format!("Person {}", index - 1),
        };
        let face = pictured(avatar(&name[..1], 0x3ba55c + index as u32 * 0x1f2f), &name);
        let mut tile = person(&format!("{ALICE}{index}"), &name, face);
        if index % 3 == 1 {
            tile.chip.mark = Some(Mark::Muted);
        }
        tile
    }));
    tiles.truncate(count);
    if let Some(tile) = tiles.get_mut(5) {
        tile.outline = Outline::Speaking;
    }
    tiles
}

/// Ten in the call, none large.
pub fn ten() -> Stage {
    grid(crowd(10))
}

/// The Lounge's window, its stage open: the call's speaker on the server,
/// its one event over the list, the Lounge chosen with Alice in it, the
/// dock on the account card, the header counting who is here, and `stage`
/// under the call's empty line.
pub fn window(stage: Stage) -> Vm {
    let mut vm = joined_window();
    if let Place::Server(server) = &mut vm.sidebar.place {
        server.header.events = Some(1);
        for row in &mut server.channels.rows {
            row.selected = row.name == "Lounge";
        }
    }
    vm.header = counted(people_in(&stage));
    vm.conversation = Conversation {
        description: String::new(),
        call: Call::Here {
            focused: stage.focused().is_some(),
        },
        ..conversation::voice()
    };
    vm.voice.dock = Dock {
        time: "0:10".into(),
        ..dock::joined()
    };
    vm.voice.controls = controls::joined();
    vm.voice.stage = stage;
    vm
}

/// The Lounge's window with Bob gone from the server's list too
/// (`light/voice_stage.png`).
pub fn quiet_window() -> Vm {
    let mut vm = window(quiet());
    in_lounge(&mut vm, |people| {
        people.retain(|person| person.name != "Bob")
    });
    vm
}

/// Bob speaking, ringed in green and in bold in the server's list too
/// (`voice/speaking_ring.png`).
pub fn speaking_window() -> Vm {
    let mut vm = window(speaking());
    bob_in_lounge(&mut vm, |bob| bob.speaking = true);
    vm
}

/// Bob filming, his camera before his LIVE tag in the server's list too
/// (`voice/with_camera.png`).
pub fn camera_window() -> Vm {
    let mut vm = window(with_camera());
    bob_in_lounge(&mut vm, |bob| bob.video = true);
    vm
}

/// Alice sharing her screen: LIVE beside her in the server's list, and the
/// dock and the call controls sharing too.
pub fn sharing_window() -> Vm {
    let mut vm = window(sharing());
    in_lounge(&mut vm, |people| {
        people
            .iter_mut()
            .filter(|person| person.name == "Alice")
            .for_each(|alice| alice.live = true);
    });
    // Bob watches, as the screen's tile counts.
    vm.voice.dock = Dock {
        time: "0:10".into(),
        share_watchers: 1,
        ..dock::sharing()
    };
    vm.voice.controls.sharing = true;
    vm
}

/// `change` to the people in the Lounge's row of the server's list.
fn in_lounge(vm: &mut Vm, mut change: impl FnMut(&mut Vec<VoicePerson>)) {
    if let Place::Server(server) = &mut vm.sidebar.place {
        for row in &mut server.channels.rows {
            if row.name == "Lounge" {
                change(&mut row.people);
            }
        }
    }
}

/// `change` to Bob in the Lounge's row of the server's list.
fn bob_in_lounge(vm: &mut Vm, change: impl Fn(&mut VoicePerson)) {
    in_lounge(vm, |people| {
        people
            .iter_mut()
            .filter(|person| person.name == "Bob")
            .for_each(&change);
    });
}

/// Alice alone in the Lounge: the line over the stage says so.
pub fn alone_window() -> Vm {
    let mut vm = window(alone());
    vm.conversation.description = conversation::voice().description;
    in_lounge(&mut vm, |people| {
        people.retain(|person| person.name == "Alice")
    });
    vm
}

/// Bob's DM with its call above his messages, `stage` in it: his header,
/// and his row chosen in the list with the call's mark
/// (`dmcall/connected_bob_dm.png`), as V07's `join::call_window` has them,
/// and his profile docked beside.
pub fn dm_window(stage: Stage) -> Vm {
    let mut vm = shell::dm_with(conversation::dm_call_here);
    vm.header = header::dm_activity();
    if let Header::Open(place) = &mut vm.header {
        place.tools.friend = Friend::None;
        place.tools.pins = Some(false);
        place.tools.profile = Some(true);
        // In the call the header offers no call (Slint's `can-call`).
        place.tools.call = None;
    }
    let timeline = chat::timeline::bob();
    let room = timeline
        .room
        .as_ref()
        .map_or("", |room| room.id.as_str())
        .to_owned();
    if let Place::Dms(dms) = &mut vm.sidebar.place {
        for row in &mut dms.rows {
            row.selected = row.id == room;
            if row.selected {
                row.call = true;
                row.voice = None;
            }
        }
    }
    vm.chat.timeline = timeline;
    vm.people.profile.panel = Some(crate::fixtures::people::profile::bob_docked());
    vm.conversation.call = Call::Here {
        focused: stage.focused().is_some(),
    };
    vm.shell.voice_dock = true;
    vm.voice.dock = dock::dm_call();
    vm.voice.controls = controls::dm_call();
    vm.voice.stage = stage;
    vm
}

/// The Lounge's window with `stage` over all of it, full screen.
pub fn full_window(stage: Stage) -> Vm {
    shell::open(window(stage), Layer::FullStage)
}

/// How many people a stage shows: each person's own tile.
fn people_in(stage: &Stage) -> usize {
    stage
        .tiles
        .iter()
        .filter(|tile| matches!(tile.key, Key::Person(_)))
        .count()
}

/// The Lounge's header, counting `people`.
fn counted(people: usize) -> Header {
    let mut header = header::voice();
    if let Header::Open(place) = &mut header {
        place.tools.count = trn!("{n} member", "{n} members", people as u64);
    }
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_sample_says_which_tile_is_large() {
        assert_eq!(lounge().focused(), None);
        assert_eq!(watching().focused(), Some(1));
        assert_eq!(with_camera().focused(), Some(0));
        assert!(full().full() && !others_hidden().full());
        assert!(others_hidden().others_hidden());
        assert_eq!(crowd(10).len(), 10);
        assert_eq!(crowd(3).len(), 3);
        assert_eq!(
            people_in(&with_camera()),
            4,
            "a stream's own tile isn't a person"
        );
        assert_eq!(people_in(&sharing()), 4, "nor this account's screen");
    }

    #[test]
    fn the_lists_show_the_call_as_the_goldens_do() {
        let vm = dm_window(dm_call());
        let Place::Dms(dms) = &vm.sidebar.place else {
            panic!("the DMs listed");
        };
        let chosen: Vec<_> = dms.rows.iter().filter(|row| row.selected).collect();
        assert_eq!(chosen.len(), 1);
        assert_eq!((chosen[0].name.as_str(), chosen[0].call), ("Bob", true));

        let bob = |mut vm: Vm| {
            let mut found = None;
            in_lounge(&mut vm, |people| {
                found = people
                    .iter()
                    .find(|person| person.name == "Bob")
                    .map(|bob| (bob.speaking, bob.video));
            });
            found
        };
        assert_eq!(bob(speaking_window()), Some((true, false)));
        assert_eq!(bob(camera_window()), Some((false, true)));
        assert_eq!(bob(quiet_window()), None);

        let mut names = Vec::new();
        in_lounge(&mut alone_window(), |people| {
            names = people.iter().map(|person| person.name.clone()).collect();
        });
        assert_eq!(names, ["Alice"], "alone in the list too");

        let mut vm = sharing_window();
        let mut alice_live = false;
        in_lounge(&mut vm, |people| {
            alice_live = people
                .iter()
                .any(|person| person.name == "Alice" && person.live);
        });
        assert!(alice_live && vm.voice.dock.sharing && vm.voice.controls.sharing);

        let Header::Open(place) = &dm_window(dm_call()).header else {
            panic!("Bob's header");
        };
        assert!(place.tools.call.is_none(), "no call button while in it");
    }
}
