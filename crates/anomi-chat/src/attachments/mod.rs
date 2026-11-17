//! A message's gallery, files and stickers under its text: task C04
//! (docs/iced/tasks/chat.md). `MessageGallery`, `FileCard` and
//! `MessageStickers` (`ui/messages.slint:1615`, `:1750`, `:1683`).
//!
//! - [`gallery`]: pictures and videos as one block, each cell clipped to 8
//!   px corners, with its spoiler cover, GIF star, Save and play button.
//! - [`file`]: a file as a card, a voice message as C11's player.
//! - [`sticker`]: stickers on their own, a click opens the sticker's card.
//!
//! Each part sits 4 px under what is above it, as `padding-top: 4px` does;
//! a message with none draws nothing.

mod file;
mod gallery;
mod sticker;

#[cfg(feature = "shots")]
mod shots;

use iced::widget::column;

use anomi_theme::prelude::*;
use anomi_vm::chat::attachments::{Attachments, Intent};

pub fn view(attachments: &Attachments, motion: Motion) -> impl View<Intent> + '_ {
    let mut parts = column![];
    if attachments.is_empty() {
        return parts;
    }
    if !attachments.gallery.cells.is_empty() {
        parts = parts.push(gallery::view(attachments, motion));
    }
    for card in &attachments.files {
        parts = parts.push(file::view(attachments, card, motion));
    }
    if !attachments.stickers.is_empty() {
        parts = parts.push(sticker::view(attachments, motion));
    }
    parts
}

/// Its shots for `anomi-shots`: each the whole window from a fixture (see
/// `crate::shots`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    shots::all()
}
