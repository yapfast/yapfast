//! The dialogs about places: the quick switcher, a new DM, invite, join,
//! event, gift, authorize, leave. Task I06 (docs/iced/tasks/integration.md)
//! handles them; until then F01's stub logs each.

use iced::Task;

use anomi_vm::Intent;

use crate::app::App;
use crate::message::Message;

/// Their intents, as the window's `Intent`: `Switcher`, `NewDm` or `Places`.
pub fn handle(_app: &mut App, intent: Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
