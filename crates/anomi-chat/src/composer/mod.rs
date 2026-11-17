//! The composer's frame. Task C14 (docs/iced/tasks/chat.md) fills it; F01's
//! stub draws nothing until then.

pub mod commands;
mod request_bar;
mod staged;
pub mod suggestions;

use iced::widget::column;

use anomi_theme::prelude::*;
use anomi_vm::chat::{self, composer::Composer};

/// The composer's frame, with the parts other tasks draw. The calls to them
/// are F01's wiring: keep them as the view grows.
pub fn view(composer: &Composer, motion: Motion) -> impl View<chat::Intent> + '_ {
    column![
        suggestions::view(&composer.suggestions, motion).map(chat::Intent::from),
        commands::view(&composer.commands, motion).map(chat::Intent::from),
    ]
}

/// Its shots for `anomi-shots`: none until C14 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
