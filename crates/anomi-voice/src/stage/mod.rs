//! The participant grid. Task V04 (docs/iced/tasks/voice.md) fills it; F01's
//! stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::voice::{self, stage::Stage};

pub fn view(_stage: &Stage, _motion: Motion) -> impl View<voice::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until V04 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
