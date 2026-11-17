//! The member list. Task P05 (docs/iced/tasks/people.md) fills it; F01's stub
//! draws nothing until then.

mod group;
mod member_row;

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::people::{self, members::Members};

pub fn view(_members: &Members, _motion: Motion) -> impl View<people::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until P05 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
