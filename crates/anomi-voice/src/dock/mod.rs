//! The voice dock. Task V01 (docs/iced/tasks/voice.md) fills it; F01's stub
//! draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::voice::dock::{Dock, Intent};

pub fn view(_dock: &Dock, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until V01 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
