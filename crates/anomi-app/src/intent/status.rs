//! Status's intents: the status menu and the custom status dialog. Task I03
//! (docs/iced/tasks/integration.md) handles them; until then F01's stub logs
//! each.

use iced::Task;

use anomi_vm::people::status;

use crate::app::App;
use crate::message::Message;

/// Status's intents.
pub fn handle(_app: &mut App, intent: status::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
