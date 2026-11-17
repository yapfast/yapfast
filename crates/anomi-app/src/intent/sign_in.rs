//! Signing in, linking Discord and two-factor prompts. Task I05
//! (docs/iced/tasks/integration.md) handles them; until then F01's stub logs
//! each.

use iced::Task;

use anomi_vm::settings::mfa;
use anomi_vm::sign_in;

use crate::app::App;
use crate::message::Message;

/// Sign-in's intents.
pub fn handle(_app: &mut App, intent: sign_in::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}

/// The two-factor prompt's intents.
pub fn mfa(_app: &mut App, intent: mfa::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
