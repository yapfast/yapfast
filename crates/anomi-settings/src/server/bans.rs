//! A server's Bans. Task X06 (docs/iced/tasks/settings.md) fills it; F01's
//! stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::bans::{Bans, Intent};

pub fn view(_bans: &Bans, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until X06 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
