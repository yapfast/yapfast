//! The grid's shots: the window with the call's stage open, named after the
//! golden screenshots where one matches. `dmcall/connected_discord_dm.png`
//! is the same call with Discord's DM open, which shows no stage (V01's
//! dock and N03's list draw it); its stage is Bob's DM's,
//! `dmcall/connected_bob_dm.png`, here as `call_bob_dm` beside V01's shot
//! of that name.

use anomi_vm::fixtures::voice::stage as fixtures;
use anomi_vm::fixtures::{GOLDEN, SMALL, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |area, name, size, theme, vm| Shot {
        area,
        name,
        size,
        theme,
        vm,
    };
    let bonfire = |area, name, vm| shot(area, name, GOLDEN, "bonfire", vm);
    vec![
        bonfire("voice", "stage", || fixtures::window(fixtures::lounge())),
        bonfire("voice", "speaking_ring", fixtures::speaking_window),
        bonfire("voice", "watching_stream", || {
            fixtures::window(fixtures::watching())
        }),
        bonfire("voice", "with_camera", fixtures::camera_window),
        bonfire("dmcall", "call_bob_dm", || {
            fixtures::dm_window(fixtures::dm_call())
        }),
        shot("small", "voice_stage", SMALL, "bonfire", || {
            fixtures::window(fixtures::lounge())
        }),
        shot(
            "light",
            "voice_stage",
            GOLDEN,
            "catppuccin-latte",
            fixtures::quiet_window,
        ),
        bonfire("voice", "stage_full_screen", || {
            fixtures::full_window(fixtures::full())
        }),
        bonfire("voice", "stage_others_hidden", || {
            fixtures::window(fixtures::others_hidden())
        }),
        bonfire("voice", "stage_sharing", || {
            fixtures::window(fixtures::sharing())
        }),
        bonfire("voice", "stage_ten", || fixtures::window(fixtures::ten())),
        bonfire("voice", "stage_alone", fixtures::alone_window),
        bonfire("dmcall", "call_watching", || {
            fixtures::dm_window(fixtures::watching())
        }),
    ]
}
