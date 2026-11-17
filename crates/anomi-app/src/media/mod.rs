//! Media: video frames through `picture::set_frame_buffers`, the image
//! viewer, voice messages. Task I09 (docs/iced/tasks/integration.md) fills
//! this folder; until then F01's stub logs their intents.

use iced::Task;

use anomi_vm::chat::Intent;

use crate::app::App;
use crate::message::Message;

/// `Video`, `Viewer` and `VoiceMessage` of `chat::Intent`.
pub fn handle(_app: &mut App, intent: Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
