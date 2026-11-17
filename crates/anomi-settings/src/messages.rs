//! The Messages settings page. Task S04 (docs/iced/tasks/settings.md) fills
//! it; F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::messages::{Intent, Messages};

pub fn view(_messages: &Messages, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until S04 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
