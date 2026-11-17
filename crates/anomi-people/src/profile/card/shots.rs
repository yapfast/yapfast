//! The card's shots for `anomi-shots`, each the whole window from a
//! fixture: Bob's and Carol's popouts beside their member rows, Bob's with
//! his menu open, and Bob's card docked beside his DM. The decoration and
//! the effect the widgets draw are added here, as the fixtures can't reach
//! them.

use iced::widget::image;

use anomi_vm::Vm;
use anomi_vm::fixtures::people::profile;
use anomi_vm::people::profile::Card;
use anomi_widgets::effect;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |area, name, vm: fn() -> Vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("profile", "popout_member", || {
            dressed(profile::popout_member())
        }),
        shot("profile", "popout_carol", profile::popout_carol),
        shot("profile", "popout_menu", || dressed(profile::popout_menu())),
        shot(
            "dm",
            "conversation_profile",
            || dressed(profile::dm_panel()),
        ),
    ]
}

/// Bob as the golden screenshots show him: his avatar's pink ring and his
/// profile effect, on every card of his.
fn dressed(mut vm: Vm) -> Vm {
    let ring = ring();
    let profile = &mut vm.people.profile;
    dress(&mut profile.card, &ring);
    if let Some(panel) = &mut profile.panel {
        dress(panel, &ring);
    }
    vm
}

fn dress(card: &mut Card, ring: &image::Handle) {
    card.decoration = Some(ring.clone());
    card.effect = Some(effect::sample::petals());
}

/// A decoration like the golden screenshots': a pink ring on nothing, which
/// the avatar sits inside (a fifth larger than it).
fn ring() -> image::Handle {
    const SIDE: u32 = 96;
    let (middle, outer, inner) = (SIDE as f32 / 2.0, 47.0_f32, 41.0_f32);
    let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let distance = (x as f32 + 0.5 - middle).hypot(y as f32 + 0.5 - middle);
            let alpha = ((outer - distance).clamp(0.0, 1.0)
                * (distance - inner).clamp(0.0, 1.0)
                * 255.0) as u8;
            pixels.extend([255, 105, 180, alpha]);
        }
    }
    image::Handle::from_rgba(SIDE, SIDE, pixels)
}
