//! The dialogs' shots for `anomi-shots`: no golden shot exists, so they are
//! compared with the Slint app's dialogs under `scripts/ui-session`.

use anomi_vm::fixtures::settings::ownership as sample;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    fn shot(name: &'static str, vm: fn() -> anomi_vm::Vm) -> Shot {
        Shot {
            area: "ownership",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("delete", || sample::asked(sample::delete())),
        shot("delete_wrong", || sample::asked(sample::delete_wrong())),
        shot("deleting", || sample::asked(sample::deleting())),
        shot("delete_refused", || sample::asked(sample::delete_refused())),
        shot("transfer", || sample::asked(sample::transfer())),
        shot("transfer_agreed", || {
            sample::asked(sample::transfer_agreed())
        }),
        shot("transfer_verified", || {
            sample::asked(sample::transfer_verified())
        }),
        shot("transfer_refused", || {
            sample::asked(sample::transfer_refused())
        }),
        shot("code", || sample::asked(sample::code())),
        shot("code_resent", || sample::asked(sample::code_resent())),
        shot("code_refused", || sample::asked(sample::code_refused())),
    ]
}
