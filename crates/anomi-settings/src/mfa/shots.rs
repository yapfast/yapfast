//! The prompt's shots for `anomi-shots`: no golden shot exists, so they are
//! compared with the Slint app's prompt under `scripts/ui-session`.

use anomi_vm::fixtures::settings::mfa as sample;

use crate::shots::{GOLDEN, Shot};

pub fn shots() -> Vec<Shot> {
    fn shot(name: &'static str, vm: fn() -> anomi_vm::Vm) -> Shot {
        Shot {
            area: "mfa",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        }
    }
    vec![
        shot("listing", || sample::asked(sample::listing())),
        shot("app", || sample::asked(sample::app())),
        shot("app_ready", || sample::asked(sample::app_ready())),
        shot("busy", || sample::asked(sample::busy())),
        shot("refused", || sample::asked(sample::refused())),
        shot("sms", || sample::asked(sample::sms())),
        shot("password", || sample::asked(sample::password())),
        shot("backup", || sample::asked(sample::backup())),
        shot("warning", || sample::asked(sample::warning())),
        shot("warning_over", || sample::asked(sample::warning_over())),
        shot("passkey", || sample::asked(sample::passkey())),
    ]
}
