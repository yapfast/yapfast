//! A new thread. Task C25 (docs/iced/tasks/chat.md) fills it; F01's stub
//! draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::{self, new_thread::NewThread};

pub fn view(_new_thread: &NewThread, _motion: Motion) -> impl View<chat::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C25 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
