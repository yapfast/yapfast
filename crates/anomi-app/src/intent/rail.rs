//! The rail's intents: `on_select_server` and the DM home in
//! `src/ui/callbacks.rs`.

use iced::Task;
use yapfast_core::application::{Action, Navigation};

use anomi_vm::rail::Intent;

use crate::app::App;
use crate::message::Message;

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let place = match intent {
        Intent::Home => Navigation::DirectMessages,
        Intent::Server(id) => Navigation::Server(id),
    };
    app.dispatch(Action::Navigate(place))
}
