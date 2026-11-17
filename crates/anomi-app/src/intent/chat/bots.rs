//! Bot components, polls, emoji and sticker cards, and bot forms. Task I08
//! (docs/iced/tasks/integration.md) handles them; until then F01's stub logs
//! each.

use iced::Task;

use anomi_vm::chat::Intent;

use crate::app::App;
use crate::message::Message;

/// `Components`, `Poll`, `Cards` and `BotForms`.
pub fn handle(_app: &mut App, intent: Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
