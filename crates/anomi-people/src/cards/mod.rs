//! The server, tag and role cards. Task P12 (docs/iced/tasks/people.md) fills
//! it; F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::people::cards::{Cards, Intent};

pub fn view(_cards: &Cards, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until P12 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
