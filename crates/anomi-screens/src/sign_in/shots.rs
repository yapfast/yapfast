//! The sign-in screen's shots for `anomi-shots`: no golden screenshot (the
//! reference session would drop its login), so each is compared with the
//! Slint app by hand.

use anomi_vm::fixtures::sign_in as sample;

use crate::shots::{GOLDEN, SMALL, Shot};

fn shot(name: &'static str, vm: fn() -> anomi_vm::Vm) -> Shot {
    Shot {
        area: "sign_in",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    }
}

/// Linking Discord in each phase, the token field, the graphics notice, the
/// Matrix sign-in and a short window where the card scrolls.
pub fn shots() -> Vec<Shot> {
    vec![
        shot("link_idle", sample::idle),
        shot("link_starting", sample::starting),
        shot("link_code", sample::code),
        shot("link_scanned", sample::scanned),
        shot("link_failed", sample::failed),
        shot("link_captcha", sample::captcha),
        shot("link_token", sample::token),
        shot("link_checking", sample::checking),
        shot("link_connecting", sample::connecting),
        shot("graphics_notice", sample::graphics_notice),
        shot("matrix", sample::matrix),
        shot("matrix_refused", sample::matrix_refused),
        shot("matrix_busy", sample::matrix_busy),
        Shot {
            size: SMALL,
            ..shot("link_code_small", sample::code)
        },
    ]
}
