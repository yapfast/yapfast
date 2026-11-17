//! The GIF picker. Task C18 (docs/iced/tasks/chat.md) fills it; F01's stub
//! draws nothing until then.

use iced::widget::space;

use anomi_theme::prelude::*;
use anomi_vm::chat::gif_picker::{GifPicker, Intent};

pub fn view(_gif_picker: &GifPicker, _motion: Motion) -> impl View<Intent> + '_ {
    space()
}

/// Its shots for `anomi-shots`: none until C18 adds them, each the whole
/// window from a fixture (see `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    Vec::new()
}
