//! The pickers' intents: emoji, GIFs, stickers. Task I12
//! (docs/iced/tasks/integration.md) handles them; until then F01's stub logs
//! each.

use iced::Task;

use anomi_vm::chat::Intent;

use crate::app::App;
use crate::message::Message;

/// `Pickers`, `EmojiPicker`, `GifPicker` and `StickerPicker`.
pub fn handle(_app: &mut App, intent: Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
