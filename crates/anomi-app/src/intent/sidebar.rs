//! The sidebar's intents: opening a room, Friends and Requests, and the
//! search field, which filters the rows without the core; and Browse
//! Channels'. Task I02 (docs/iced/tasks/integration.md) takes it further.

use iced::Task;
use yapfast_core::application::{Action, DmScreen};

use anomi_vm::browse;
use anomi_vm::sidebar::{Intent, channels, dms};

use crate::app::App;
use crate::message::Message;

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Dms(dms::Intent::Open(room)) | Intent::Channels(channels::Intent::Open(room)) => {
            app.dispatch(Action::Open(room))
        }
        Intent::Dms(dms::Intent::Friends) => app.dispatch(Action::OpenDmScreen(DmScreen::Friends)),
        Intent::Dms(dms::Intent::Requests) => {
            app.dispatch(Action::OpenDmScreen(DmScreen::Requests))
        }
        Intent::Filter(text) => {
            app.vm.sidebar.filter = text;
            app.refresh();
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Browse Channels' intents (N14).
pub fn browse(_app: &mut App, intent: browse::Intent) -> Task<Message> {
    tracing::debug!(?intent, "not handled by the iced interface yet");
    Task::none()
}
