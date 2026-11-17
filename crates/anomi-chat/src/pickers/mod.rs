//! The expression picker above the composer, with its tabs. Task C18
//! (docs/iced/tasks/chat.md) fills it; F01's stub draws nothing until then.

pub mod emoji;
pub mod gif;
pub mod sticker;

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::{self, pickers::Pickers};

pub fn view(_pickers: &Pickers, _motion: Motion) -> impl View<chat::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C18 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
