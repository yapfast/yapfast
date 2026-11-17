//! The timeline's and the message's intents, with its reactions',
//! attachments' and embeds'. Task I07 (docs/iced/tasks/integration.md)
//! handles them; until then F01's stub logs each.

use iced::Task;

use anomi_vm::chat::Intent;

use crate::app::App;
use crate::message::Message;

/// `Timeline`, `Message`, `Reactions`, `Attachments` and `Embeds`.
pub fn handle(_app: &mut App, intent: Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
