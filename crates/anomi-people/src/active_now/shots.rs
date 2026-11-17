//! The column's shots: the golden `friends/online_tab_active_now`, with its
//! card's picture, and the column's other states, each the whole window on
//! the friends screen (see `crate::shots`).

use anomi_vm::fixtures::people::active_now as sample;
use anomi_vm::fixtures::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    fn shot(name: &'static str, vm: fn() -> anomi_vm::Vm) -> Shot {
        Shot {
            area: "active_now",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("online_tab_active_now", || sample::window(sample::around())),
        shot("busy", || sample::window(sample::busy())),
        shot("quiet", || sample::window(sample::quiet())),
        Shot {
            theme: "catppuccin-latte",
            ..shot("light", || sample::window(sample::busy()))
        },
    ]
}
