//! The dock's shots: the window with it on the account card, named after
//! the golden screenshots where one matches.

use anomi_vm::fixtures::voice::dock as fixtures;
use anomi_vm::fixtures::{GOLDEN, SMALL, Shot};
use anomi_vm::voice::Voice;
use anomi_vm::{Vm, fixtures::sidebar::channels::joined_window};

pub fn shots() -> Vec<Shot> {
    let shot = |area, name, size, vm| Shot {
        area,
        name,
        size,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("voice", "joined_sidebar_dock", GOLDEN, || Vm {
            voice: Voice {
                dock: fixtures::joined(),
                ..Voice::default()
            },
            ..joined_window()
        }),
        shot("dmcall", "dock_bob_dm", GOLDEN, || {
            fixtures::dm_window(fixtures::dm_call())
        }),
        shot("voice", "dock_joining", GOLDEN, || {
            fixtures::window(fixtures::joining())
        }),
        shot("voice", "dock_camera_starting", GOLDEN, || {
            fixtures::window(fixtures::starting_camera())
        }),
        shot("voice", "dock_camera_unavailable", GOLDEN, || {
            fixtures::window(fixtures::camera_unavailable())
        }),
        shot("voice", "dock_sharing", GOLDEN, || {
            fixtures::window(fixtures::sharing())
        }),
        shot("voice", "dock_sharing_long", GOLDEN, || {
            fixtures::window(fixtures::sharing_long())
        }),
        shot("voice", "dock_sharing_narrow", GOLDEN, || {
            fixtures::narrow_window(fixtures::sharing_long())
        }),
        shot("voice", "dock_choosing", GOLDEN, || {
            fixtures::window(fixtures::choosing())
        }),
        shot("voice", "dock_warning", GOLDEN, || {
            fixtures::window(fixtures::warned())
        }),
        shot("voice", "dock_error_in_call", GOLDEN, || {
            fixtures::window(fixtures::failed_in_call())
        }),
        shot("voice", "dock_failed", GOLDEN, || {
            fixtures::window(fixtures::failed())
        }),
        shot("small", "voice_dock", SMALL, || {
            fixtures::window(fixtures::sharing())
        }),
    ]
}
