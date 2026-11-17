//! The window frame's intents: panel edges, layers. Task I01
//! (docs/iced/tasks/integration.md) handles them; until then F01's stub logs
//! each.

use iced::Task;

use anomi_vm::shell;

use crate::app::App;
use crate::message::Message;

/// The frame's intents.
pub fn handle(_app: &mut App, intent: shell::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
