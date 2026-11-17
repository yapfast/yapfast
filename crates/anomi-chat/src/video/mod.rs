//! A video in a message, and full screen. Task C09 (docs/iced/tasks/chat.md)
//! fills it; F01's stub draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::video::{FullScreen, Intent, Video};

pub fn view(_video: &Video, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// A video played full screen. Drawn by the shell over the window.
pub fn full_screen(_video: &FullScreen, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C09 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
