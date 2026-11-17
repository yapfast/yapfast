//! Menus' intents: the context menu over the window and the message menu.
//! Task I10 (docs/iced/tasks/integration.md) handles them; until then F01's
//! stub logs each.

use iced::Task;

use anomi_vm::{chat, menu};

use crate::app::App;
use crate::message::Message;

/// The context menu's intents.
pub fn handle(_app: &mut App, intent: menu::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}

/// The message menu's: `chat::Intent::Menu`.
pub fn message(_app: &mut App, intent: chat::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
