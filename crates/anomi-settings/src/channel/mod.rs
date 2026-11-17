//! A channel's settings frame and its overview. Task X13
//! (docs/iced/tasks/settings.md) fills it; F01's stub draws nothing until
//! then.

pub mod create;
mod overview;
pub mod permissions;

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::{self, channel::Channel};

pub fn view(_channel: &Channel, _motion: Motion) -> impl View<settings::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until X13 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
