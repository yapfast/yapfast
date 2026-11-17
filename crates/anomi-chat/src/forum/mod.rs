//! A forum. Task C26 (docs/iced/tasks/chat.md) fills it; F01's stub draws
//! nothing until then.

mod card;
mod list;
pub mod new_post;

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::{self, forum::Forum};

pub fn view(_forum: &Forum, _motion: Motion) -> impl View<chat::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C26 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
