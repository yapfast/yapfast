//! Forwarding messages. Task I10 (docs/iced/tasks/integration.md) handles
//! them; until then F01's stub logs each.

use iced::Task;

use anomi_vm::chat;

use crate::app::App;
use crate::message::Message;

/// `chat::Intent::Forward`.
pub fn handle(_app: &mut App, intent: chat::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
