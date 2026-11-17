//! The soundboard's shots: the Lounge's window with it open above its
//! button, named after the golden screenshot.

use anomi_vm::fixtures::voice::soundboard as fixtures;
use anomi_vm::fixtures::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm| Shot {
        area: "voice",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("soundboard", || fixtures::window(fixtures::full())),
        shot("soundboard_loading", || {
            fixtures::window(fixtures::loading())
        }),
        shot("soundboard_deafened", || {
            fixtures::window(fixtures::deafened())
        }),
        shot("soundboard_waiting", || {
            fixtures::window(fixtures::waiting())
        }),
        shot(
            "soundboard_scrolling",
            || fixtures::window(fixtures::long()),
        ),
    ]
}
