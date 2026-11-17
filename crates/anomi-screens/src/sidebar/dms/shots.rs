//! The DM list's shots for `anomi-shots`: the left column of the golden
//! `dm/*` screenshots, each the whole window from a fixture
//! (see `crate::shots`).

use anomi_vm::Vm;
use anomi_vm::fixtures::{GOLDEN, Shot, pictures, shell};
use anomi_vm::kind::Expected;
use anomi_vm::sidebar::dms::{Dms, Empty};
use anomi_vm::sidebar::{Place, Sidebar};
use anomi_widgets::nameplate::sample;

/// The DM home with the list `dms` made of the sample.
fn with(change: impl FnOnce(&mut Dms)) -> Vm {
    let mut vm = shell::dm_home();
    if let Place::Dms(dms) = &mut vm.sidebar.place {
        change(dms);
    }
    vm.sidebar = Sidebar {
        version: vm.sidebar.version + 1,
        ..vm.sidebar
    };
    vm
}

/// A decoration like the golden screenshots': a pink ring on nothing, which
/// the avatar sits inside (a fifth larger than it).
fn ring() -> iced::widget::image::Handle {
    const SIDE: u32 = 96;
    let (middle, outer, inner) = (SIDE as f32 / 2.0, 47.0_f32, 41.0_f32);
    let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let distance =
                ((x as f32 + 0.5 - middle).powi(2) + (y as f32 + 0.5 - middle).powi(2)).sqrt();
            let alpha = ((outer - distance).clamp(0.0, 1.0)
                * (distance - inner).clamp(0.0, 1.0)
                * 255.0) as u8;
            pixels.extend([255, 105, 180, alpha]);
        }
    }
    iced::widget::image::Handle::from_rgba(SIDE, SIDE, pixels)
}

/// Bob as the golden screenshots show him: his tag's badge arrived, his
/// avatar decorated, his plate behind the row.
fn dress(dms: &mut Dms) {
    let bob = &mut dms.rows[0];
    if let Some(tag) = &mut bob.tag {
        tag.badge = Expected::Ready(pictures::badge("bob-tag", 16));
    }
    bob.decoration = Some(ring());
    bob.plate.picture = Some(sample::strip("bob"));
    bob.plate.palette = sample::palette("sky");
}

fn bob_open(dms: &mut Dms) {
    dress(dms);
    dms.rows[2].selected = false;
    dms.rows[0].selected = true;
}

fn group_open(dms: &mut Dms) {
    dress(dms);
    dms.rows[2].selected = false;
    dms.rows[1].selected = true;
}

pub fn shots() -> Vec<Shot> {
    fn shot(name: &'static str, vm: fn() -> Vm) -> Shot {
        Shot {
            area: "dm",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("person_chat", || with(dress)),
        shot("conversation_profile", || with(bob_open)),
        shot("group", || with(group_open)),
        shot("list_states", || {
            with(|dms| *dms = anomi_vm::fixtures::sidebar::dms::states())
        }),
        shot("list_empty", || {
            with(|dms| *dms = anomi_vm::fixtures::sidebar::dms::none(Empty::NoMatch))
        }),
    ]
}
