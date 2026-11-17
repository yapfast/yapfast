//! People's intents: friends, requests, Active Now, the member list,
//! profiles, cards, about a person, adding people. Task I16
//! (docs/iced/tasks/integration.md) handles them; until then F01's stub logs
//! each.

use iced::Task;

use anomi_vm::people;

use crate::app::App;
use crate::message::Message;

/// People's intents, but status's and the editor's.
pub fn handle(_app: &mut App, intent: people::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
