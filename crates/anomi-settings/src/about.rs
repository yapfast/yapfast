//! The About settings page, with updates. Task S10
//! (docs/iced/tasks/settings.md) fills it; F01's stub draws nothing until
//! then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::about::{About, Intent};

pub fn view(_about: &About, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until S10 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
