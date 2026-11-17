//! A server's settings frame and its profile page. Task X01
//! (docs/iced/tasks/settings.md) fills it; F01's stub draws nothing until
//! then.

pub mod audit;
pub mod bans;
pub mod boosts;
pub mod emoji;
pub mod engagement;
pub mod invites;
pub mod members;
pub mod ownership;
mod profile;
pub mod roles;
pub mod safety;
pub mod stickers;

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::{self, server::Server};

pub fn view(_server: &Server, _motion: Motion) -> impl View<settings::Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until X01 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
