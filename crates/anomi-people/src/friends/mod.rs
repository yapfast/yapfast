//! The Friends screen. Task P01 (docs/iced/tasks/people.md) fills it; F01's
//! stub draws nothing until then.

pub mod add;
mod friend_row;
pub mod requests;
mod tabs;

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::people::friends::{Friends, Intent};

pub fn view(_friends: &Friends, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until P01 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
