//! A message's poll, and the dialog that makes one. Task C07
//! (docs/iced/tasks/chat.md) fills it; F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::poll::{Dialog, Intent, Poll};

pub fn view(_poll: &Poll, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// The dialog that makes a poll. Drawn by the shell over the window.
pub fn dialog(_poll: &Dialog, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C07 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
