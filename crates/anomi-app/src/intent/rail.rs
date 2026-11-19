//! The rail's intents (`src/ui/callbacks.rs:2716`-`:2735`, `:934`, `:1316`):
//! the DM home and a server, a folder opened or closed, an unread DM, joining
//! a server, and the rows in view, whose pictures are asked for; and a
//! server or folder dragged to another place, the drag kept in the rail's
//! view-model while it goes on, the order it leaves given to the core.

use iced::Task;
use yapfast_core::application::{Action, Navigation};
use yapfast_core::chat::ServerGroup;

use anomi_vm::rail::{Drag, Intent, arrange};

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
        Intent::Lift(id) => {
            app.vm.rail.drag = Some(Drag {
                id,
                ..Drag::default()
            });
            Task::none()
        }
        Intent::Aim(place) => {
            if let Some(drag) = &mut app.vm.rail.drag {
                drag.aim(place);
            }
            Task::none()
        }
        Intent::Drop => let_go(app),
        Intent::PutBack => {
            app.vm.rail.drag = None;
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Letting go: the order the drag leaves goes to the core, which shows it
/// at once and keeps it in the account's settings.
fn let_go(app: &mut App) -> Task<Message> {
    let Some(drag) = app.vm.rail.drag.take() else {
        return Task::none();
    };
    match arrange::dropped(&app.vm.rail, &drag) {
        Some(groups) => app.dispatch(Action::ArrangeServers(
            groups
                .into_iter()
                .map(|group| ServerGroup {
                    folder: group.folder,
                    servers: group.servers,
                })
                .collect(),
        )),
        None => Task::none(),
    }
}
