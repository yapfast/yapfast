//! The person's settings' intents. Task I19 (docs/iced/tasks/integration.md)
//! handles them; until then F01's stub logs each.

use iced::Task;

use anomi_vm::settings;

use crate::app::App;
use crate::message::Message;

/// The person's settings' intents.
pub fn handle(_app: &mut App, intent: settings::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
