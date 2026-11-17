//! The full profile. Task P09 (docs/iced/tasks/people.md) fills it; F01's
//! stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::people::{self, full_profile::FullProfile};

pub fn view(_full_profile: &FullProfile, _motion: Motion) -> impl View<people::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until P09 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
