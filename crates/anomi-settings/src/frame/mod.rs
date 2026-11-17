//! The settings frame. Task S01 (docs/iced/tasks/settings.md) fills it; F01's
//! stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::{self, frame::Frame};

pub fn view(_frame: &Frame, _motion: Motion) -> impl View<settings::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until S01 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
