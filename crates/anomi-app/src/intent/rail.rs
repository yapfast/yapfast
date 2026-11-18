//! The rail's intents (`src/ui/callbacks.rs:2716`-`:2735`, `:934`, `:1316`):
//! the DM home and a server, a folder opened or closed, an unread DM, joining
//! a server, and the rows in view, whose pictures are asked for.

use iced::Task;
use yapfast_core::application::{Action, Navigation};

use anomi_vm::rail::Intent;

use crate::app::App;
use crate::message::Message;

pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Home => app.dispatch(Action::Navigate(Navigation::DirectMessages)),
        Intent::Server(id) => app.dispatch(Action::Navigate(Navigation::Server(id))),
        Intent::Folder(id) => {
            app.layout.edit(|saved| {
                if !saved.open_folders.remove(&id) {
                    saved.open_folders.insert(id);
                }
            });
            app.refresh();
            Task::none()
        }
        Intent::Dm(room) => app.dispatch(Action::Open(room)),
        Intent::Join => app.dispatch(Action::OpenJoinServer),
        Intent::Seen(ids) => {
            crate::project::rail_seen(&app.core.state, &mut app.shown, ids, &mut app.pictures);
            Task::none()
        }
        // A server's, a folder's or an unread DM's menu.
        Intent::Menu(id, spot) => super::menus::open(app, id, false, spot),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}
