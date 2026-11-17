//! The conversation header's intents: its buttons. Task I04
//! (docs/iced/tasks/integration.md) handles them; until then F01's stub logs
//! each.

use iced::Task;

use anomi_vm::header;

use crate::app::App;
use crate::message::Message;

/// The header's intents.
pub fn handle(_app: &mut App, intent: header::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
