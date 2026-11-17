//! The window's own frame, with its title bar, caption buttons and resize
//! edges. Task F18 (docs/iced/tasks/foundation.md) fills it; F01's stub draws
//! nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chrome::{Chrome, Intent};

pub fn view(_chrome: &Chrome, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until F18 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
