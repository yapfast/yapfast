//! The menu's shots: the Lounge's window with it open over the controls'
//! share button, named after the golden screenshot where one matches.

use anomi_vm::Vm;
use anomi_vm::fixtures::voice::share as fixtures;
use anomi_vm::fixtures::{GOLDEN, Shot};
use anomi_vm::voice::share::Share;

use super::built;

/// The window with `share` open, its rows made.
fn window(share: Share) -> Vm {
    let mut vm = fixtures::window(share);
    vm.voice.share = built(std::mem::take(&mut vm.voice.share));
    vm
}

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm| Shot {
        area: "voice",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("share_menu", || window(fixtures::dialog())),
        shot("share_menu_screens", || window(fixtures::listed())),
        shot("share_menu_one_screen", || window(fixtures::one_screen())),
        shot("share_menu_apps", || window(fixtures::apps())),
        shot("share_menu_switching", || window(fixtures::switching())),
    ]
}
