//! The card a custom emoji or a sticker opens. Task C08
//! (docs/iced/tasks/chat.md) fills it; F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::cards::{Cards, Intent};

pub fn view(_cards: &Cards, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C08 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
