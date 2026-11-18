//! The inbox popout's intents, and the bar's Inbox button that opens and
//! closes it (`toggle-inbox`, `inbox-opened`, `ui/yapfast.slint:5590`-
//! `:5621`; `src/ui/callbacks.rs:1288`). It is `Layer::Inbox`; the bar's
//! button shows pressed while it is open (`Chrome::inbox_open`).

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::inbox::Intent;
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;

/// The inbox's intents.
pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        // Its rows are those of the tab (`changed-tab => inbox-opened()`).
        Intent::Tab(tab) => {
            app.vm.inbox.tab = tab;
            app.refresh();
            Task::none()
        }
        Intent::Open(id) => {
            close(app);
            app.dispatch(Action::SwitchTo(id))
        }
        Intent::Seen(ids) => {
            crate::project::inbox_seen(&app.core.state, &mut app.shown, ids, &mut app.pictures);
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The bar's Inbox button: opens the popout, its rows made as it opens, or
/// closes it.
pub fn toggle(app: &mut App) -> Task<Message> {
    if app.vm.shell.is_open(Layer::Inbox) {
        close(app);
    } else {
        app.vm.shell.open(Layer::Inbox);
        app.vm.chrome.inbox_open = true;
        app.refresh();
    }
    Task::none()
}

/// Closes the popout: from a row, its button, Escape or a press outside.
pub fn close(app: &mut App) {
    app.vm.shell.close(Layer::Inbox);
    app.vm.chrome.inbox_open = false;
}

#[cfg(test)]
mod tests {
    use anomi_vm::inbox::{Intent, Tab};
    use anomi_vm::shell::Layer;
    use yapfast_core::application::Action;

    use crate::app::App;

    #[test]
    fn the_inbox_opens_lists_and_opens_a_room() {
        let folder = std::env::temp_dir().join(format!("anomi-inbox-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let mut app = App::for_tests(&folder);
        drop(app.core.dispatch(Action::Preview));
        // An unread channel, a muted one, and a DM that mentions.
        for room in app.core.state.rooms.iter_mut() {
            let room = std::sync::Arc::make_mut(room);
            match room.id.as_str() {
                "games" => room.unread = 3,
                "weekend" => {
                    room.unread = 5;
                    room.muted = true;
                }
                "sam" => room.mentions = 1,
                _ => {}
            }
        }
        app.core.state.list_changes += 1;
        drop(super::toggle(&mut app));
        assert!(app.vm.shell.is_open(Layer::Inbox));
        assert!(app.vm.chrome.inbox_open);
        let names: Vec<&str> = app
            .vm
            .inbox
            .rows
            .iter()
            .map(|row| row.name.as_str())
            .collect();
        assert_eq!(names, ["#games", "Sam"]);
        assert_eq!(app.vm.inbox.rows[0].place, "Friends");
        assert_eq!(app.vm.inbox.rows[1].place, "");
        drop(super::handle(&mut app, Intent::Tab(Tab::Mentions)));
        let names: Vec<&str> = app
            .vm
            .inbox
            .rows
            .iter()
            .map(|row| row.name.as_str())
            .collect();
        assert_eq!(names, ["Sam"]);
        assert_eq!(app.vm.inbox.rows[0].badge, "1");
        drop(super::handle(&mut app, Intent::Open("games".into())));
        assert!(!app.vm.shell.is_open(Layer::Inbox));
        assert!(!app.vm.chrome.inbox_open);
        assert_eq!(app.core.state.selected.as_deref(), Some("games"));
        drop(app);
        let _ = std::fs::remove_dir_all(folder);
    }
}
