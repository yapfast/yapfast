//! The profile editor. Task P14 (docs/iced/tasks/people.md) fills it; F01's
//! stub draws nothing until then.

pub mod colour;
mod customize;
pub mod name_style;
pub mod preview;
mod slots;

use iced::widget::column;

use anomi_theme::prelude::*;
use anomi_vm::people::{self, editor::Editor};

/// The profile editor, with the parts other tasks draw. The calls to them are
/// F01's wiring: keep them as the view grows.
pub fn view(editor: &Editor, motion: Motion) -> impl View<people::Intent> + '_ {
    column![preview::view(&editor.preview, motion).map(people::Intent::from)]
}

/// Its shots for `anomi-shots`: none until P14 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
