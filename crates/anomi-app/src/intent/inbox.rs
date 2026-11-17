//! The inbox's intents. Task I04 (docs/iced/tasks/integration.md) handles
//! them; until then F01's stub logs each.

use iced::Task;

use anomi_vm::inbox;

use crate::app::App;
use crate::message::Message;

/// The inbox's intents.
pub fn handle(_app: &mut App, intent: inbox::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
