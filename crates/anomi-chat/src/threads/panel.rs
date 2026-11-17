//! A thread beside its channel. Task C24 (docs/iced/tasks/chat.md) fills it;
//! F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::{self, thread_panel::ThreadPanel};

pub fn view(_thread_panel: &ThreadPanel, _motion: Motion) -> impl View<chat::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C24 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
