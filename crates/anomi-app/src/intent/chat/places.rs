//! Threads, forums, search and pins. Task I13
//! (docs/iced/tasks/integration.md) handles them; until then F01's stub logs
//! each.

use iced::Task;

use anomi_vm::chat::Intent;

use crate::app::App;
use crate::message::Message;

/// `Search`, `Pins`, `Threads`, `ThreadPanel`, `NewThread`, `Forum` and `NewPost`.
pub fn handle(_app: &mut App, intent: Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}

/// The bar over a forum post open as the conversation: React to post,
/// Follow, Copy link (the window's `shell::Intent::ReactToPost`,
/// `FollowPost`, `CopyPostLink`).
pub fn post_bar(_app: &mut App, intent: anomi_vm::shell::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
