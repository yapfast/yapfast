//! The account card's intents. Muting and deafening come with I03, the
//! settings with S01 and I19 (docs/iced/tasks/); until then they're logged.

use iced::Task;

use anomi_vm::account::Intent;

use crate::app::App;
use crate::message::Message;

pub fn handle(_app: &mut App, intent: Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
