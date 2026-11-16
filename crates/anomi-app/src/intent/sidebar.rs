//! The sidebar's intents: opening a room, Friends and Requests, and the
//! search field, which filters the rows without the core.

use iced::Task;
use yapfast_core::application::{Action, DmScreen};

use anomi_vm::sidebar::Intent;

use crate::app::App;
use crate::message::Message;

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Open(room) => app.dispatch(Action::Open(room)),
        Intent::Friends => app.dispatch(Action::OpenDmScreen(DmScreen::Friends)),
        Intent::Requests => app.dispatch(Action::OpenDmScreen(DmScreen::Requests)),
        Intent::Filter(text) => {
            app.vm.sidebar.filter = text;
            app.refresh();
            Task::none()
        }
    }
}
