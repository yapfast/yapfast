//! The quick switcher. Task N10 (docs/iced/tasks/shell.md) fills it; F01's
//! stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::switcher::{Intent, Switcher};

pub fn view(_switcher: &Switcher, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until N10 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
