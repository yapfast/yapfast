//! Deleting a server or handing it over. Task X12
//! (docs/iced/tasks/settings.md) fills it; F01's stub draws nothing until
//! then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::ownership::{Intent, Ownership};

pub fn view(_ownership: &Ownership, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until X12 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
