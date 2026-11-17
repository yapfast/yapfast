//! The profile card. Task P07 (docs/iced/tasks/people.md) fills it; F01's
//! stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::people::{self, profile::Profile};

pub fn view(_profile: &Profile, _motion: Motion) -> impl View<people::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until P07 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
