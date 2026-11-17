//! The soundboard. Task V12 (docs/iced/tasks/voice.md) fills it; F01's stub
//! draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::voice::soundboard::{Intent, Soundboard};

pub fn view(_soundboard: &Soundboard, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until V12 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
