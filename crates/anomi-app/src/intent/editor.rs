//! The profile editor's intents, its preview's and the name style window's.
//! Task I17 (docs/iced/tasks/integration.md) handles them; until then F01's
//! stub logs each.

use iced::Task;

use anomi_vm::people;

use crate::app::App;
use crate::message::Message;

/// `Editor`, `EditorPreview` and `NameStyle` of `people::Intent`.
pub fn handle(_app: &mut App, intent: people::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
