//! Voice's intents: the dock, the stage, controls, devices, sharing, the
//! soundboard. Task I18 (docs/iced/tasks/integration.md) handles them; until
//! then F01's stub logs each.

use iced::Task;

use anomi_vm::voice;

use crate::app::App;
use crate::message::Message;

/// Voice's intents.
pub fn handle(_app: &mut App, intent: voice::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
