//! The new DM and group DM dialog. Task N11 (docs/iced/tasks/shell.md) fills
//! it; F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::new_dm::{Intent, NewDm};

pub fn view(_new_dm: &NewDm, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until N11 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
