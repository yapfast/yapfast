//! The form's shots: the golden screenshots have none, so each is named
//! after its state, over #general with the member list replaced.

use anomi_vm::fixtures::chat::new_thread as sample;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    let shot = |name, vm: fn() -> anomi_vm::Vm| Shot {
        area: "thread",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("new_blank", || sample::window(sample::blank())),
        shot("new_written", || sample::window(sample::written())),
        shot(
            "new_from_message",
            || sample::window(sample::from_message()),
        ),
        shot("new_private", || sample::window(sample::private())),
        shot(
            "new_only_private",
            || sample::window(sample::only_private()),
        ),
        shot("new_starting", || sample::window(sample::starting())),
        shot("new_refused", || sample::window(sample::refused())),
        shot("new_leaving", || sample::window(sample::leaving())),
    ]
}
