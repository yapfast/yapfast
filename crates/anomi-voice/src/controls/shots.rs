//! The controls' shots: the window with the pill under its stage, named
//! after the golden screenshots where one matches. The pill appears once
//! V04's stage places `controls::view`.

use anomi_vm::fixtures::voice::controls as fixtures;
use anomi_vm::fixtures::{GOLDEN, SMALL, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |area, name, size, vm| Shot {
        area,
        name,
        size,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("voice", "controls_joined", GOLDEN, || {
            fixtures::window(fixtures::joined())
        }),
        shot("dmcall", "controls_dm", GOLDEN, || {
            fixtures::dm_window(fixtures::dm_call())
        }),
        shot("voice", "controls_silenced", GOLDEN, || {
            fixtures::window(fixtures::silenced())
        }),
        shot("voice", "controls_camera_starting", GOLDEN, || {
            fixtures::window(fixtures::starting_camera())
        }),
        shot("voice", "controls_camera_unavailable", GOLDEN, || {
            fixtures::window(fixtures::camera_unavailable())
        }),
        shot("voice", "controls_live", GOLDEN, || {
            fixtures::window(fixtures::live())
        }),
        shot("voice", "controls_joining", GOLDEN, || {
            fixtures::window(fixtures::joining())
        }),
        shot("voice", "controls_leaving", GOLDEN, || {
            fixtures::window(fixtures::leaving())
        }),
        shot("small", "controls", SMALL, || {
            fixtures::window(fixtures::joined())
        }),
    ]
}
