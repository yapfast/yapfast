//! The message menu. Task C12 (docs/iced/tasks/chat.md) fills it; F01's stub
//! draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::menu::{Intent, Menu};

pub fn view(_menu: &Menu, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C12 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
