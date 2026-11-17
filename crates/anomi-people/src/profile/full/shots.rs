//! The full profile's shots for `anomi-shots`, each the whole window with
//! the modal open: Bob's Activity tab and his mutual friends and servers, as
//! the golden screenshots show them, then the states a tab can be in, which
//! have none. Bob's decoration and effect, which the widgets draw, are added
//! here as on the popout's shots.

use iced::widget::image;

use anomi_vm::Vm;
use anomi_vm::fixtures::people::full_profile::{self as samples, open};
use anomi_vm::people::full_profile::FullProfile;
use anomi_widgets::effect;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm: fn() -> Vm| Shot {
        area: "profile",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("full", || dressed(samples::bob())),
        shot("full_mutual_friends", || dressed(samples::on_friends())),
        shot("full_mutual_servers", || dressed(samples::on_servers())),
        shot("full_recent", || dressed(samples::recent())),
        shot("full_board", || dressed(samples::board())),
        shot("full_wishlist", || dressed(samples::wishlist())),
        shot("full_loading", || dressed(samples::loading())),
        shot("full_empty", || dressed(samples::empty())),
        shot("full_themed", || dressed(samples::themed())),
    ]
}

/// Bob as the golden screenshots show him: his avatar's pink ring and his
/// profile effect.
fn dressed(mut full: FullProfile) -> Vm {
    full.card.decoration = Some(ring());
    full.card.effect = Some(effect::sample::petals());
    open(full)
}

/// A decoration like the golden screenshots': a pink ring on nothing, which
/// the avatar sits inside (a fifth larger than it), as the card's shots
/// draw it.
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
