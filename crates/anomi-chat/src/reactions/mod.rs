//! A message's reactions, and the Reactions window. Task C03
//! (docs/iced/tasks/chat.md) fills it; F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::reactions::{Dialog, Intent, Reactions};

pub fn view(_reactions: &Reactions, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// The Reactions window: who reacted with what. Drawn by the shell over the
/// window.
pub fn dialog(_reactions: &Dialog, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C03 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
