//! The composer's intents, with its suggestions' and slash commands'. Task
//! I11 (docs/iced/tasks/integration.md) handles them; until then F01's stub
//! logs each.

use iced::Task;

use anomi_vm::chat::Intent;

use crate::app::App;
use crate::message::Message;

/// `Composer`, `Suggestions` and `Commands`.
pub fn handle(_app: &mut App, intent: Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
