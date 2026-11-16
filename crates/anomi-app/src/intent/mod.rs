//! What the screens ask, handled: each area's intents in its own module,
//! which either changes the view-model's own state (a filter's text) or
//! hands the core an `Action`, as `src/ui/callbacks.rs` does for the Slint
//! window.

mod account;
mod rail;
mod sidebar;

use iced::Task;

use anomi_vm::Intent;

use crate::app::App;
use crate::message::Message;

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Rail(intent) => rail::handle(app, intent),
        Intent::Sidebar(intent) => sidebar::handle(app, intent),
        Intent::Account(intent) => account::handle(app, intent),
    }
}
