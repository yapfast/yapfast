//! A scheduled event's dialog. Task N12 (docs/iced/tasks/shell.md) fills it;
//! F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::places::event::{Event, Intent};

pub fn view(_event: &Event, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until N12 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
