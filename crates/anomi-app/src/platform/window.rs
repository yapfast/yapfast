//! The window: F18's title bar intents (drag, resize, minimize, maximize,
//! close) as `iced::window` tasks. Task I14 (docs/iced/tasks/integration.md)
//! fills it; until then F01's stub logs each.

use iced::Task;

use anomi_vm::chrome;

use crate::app::App;
use crate::message::Message;

/// The title bar's intents.
pub fn handle(_app: &mut App, intent: chrome::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
