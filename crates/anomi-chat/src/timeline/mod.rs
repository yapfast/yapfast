//! The timeline. Task C01 (docs/iced/tasks/chat.md) fills it; F01's stub
//! draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::{self, timeline::Timeline};

pub fn view(_timeline: &Timeline, _motion: Motion) -> impl View<chat::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C01 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
