//! A server's Engagement settings. Task X02 (docs/iced/tasks/settings.md)
//! fills it; F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::settings::engagement::{Engagement, Intent};

pub fn view(_engagement: &Engagement, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until X02 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
