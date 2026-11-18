//! Forwarding a message: its search, the places chosen, the note and Send
//! (`src/ui/callbacks.rs:1277`-`:1279`, `:1842`). The core keeps what is
//! searched and chosen; the note stays here until Send hands it over.
//! `project::chat::forward` opens and closes the dialog as the core does.

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::chat;
use anomi_vm::chat::forward::Intent;

use crate::app::App;
use crate::message::Message;

/// `chat::Intent::Forward`.
pub fn handle(app: &mut App, intent: chat::Intent) -> Task<Message> {
    let chat::Intent::Forward(intent) = intent else {
        tracing::debug!(?intent, "not the forward dialog's");
        return Task::none();
    };
    match intent {
        Intent::Query(query) => app.dispatch(Action::SearchForward(query)),
        Intent::Note(note) => {
            app.vm.chat.forward.note = note;
            Task::none()
        }
        Intent::Choose(id) => app.dispatch(Action::ChooseForward(id)),
        Intent::Send => {
            let chosen = (app.core.state.forwarding.as_ref())
                .is_some_and(|forwarding| !forwarding.chosen.is_empty());
            if !chosen {
                return Task::none();
            }
            let note = std::mem::take(&mut app.vm.chat.forward.note);
            app.dispatch(Action::Forward(note))
        }
        Intent::Close => close(app),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The close button, Escape or a press outside: the core lets it go, and
/// the projection closes it.
pub fn close(app: &mut App) -> Task<Message> {
    app.dispatch(Action::CloseForward)
}
