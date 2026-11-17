//! The shots of a bot's forms and select menus. No golden shot exists:
//! compare them with the Slint app's under `scripts/ui-session`.

use anomi_vm::fixtures::chat::bot_forms as sample;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm: fn() -> anomi_vm::Vm| Shot {
        area: "bot",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("form", sample::asked),
        shot("form_unsupported", sample::asked_unsupported),
        shot("form_saving", sample::asked_saving),
        shot("form_refused", sample::asked_refused),
        shot("select_several", || sample::picking(sample::several())),
        shot("select_single", || sample::picking(sample::single())),
        shot("select_people", || sample::picking(sample::people())),
        shot("select_nothing", || sample::picking(sample::nothing())),
    ]
}
