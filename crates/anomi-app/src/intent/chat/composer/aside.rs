//! The thread beside's composer (`aside-*` callbacks,
//! `src/ui/callbacks.rs:1123`-`:1137`, `:2538`-`:2564`): the channel's
//! composer over the thread's draft. A keystroke hands the core the
//! thread's draft (`SetAsideDraft`, which reaches the composer alone) and
//! follows the words at the caret, whose suggestions show over this field
//! while it was typed in last. Its files are the thread's.

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::chat::composer::Intent;

use super::{files, follow, suggest, typed};
use crate::app::App;
use crate::message::Message;
use crate::project::chat::composer::Field;

pub(crate) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    let room = app.vm.chat.thread_panel.composer.room;
    match intent {
        // A report about a thread no longer beside is dropped.
        Intent::Edited { room: at, .. } | Intent::Caret { room: at, .. } if at != room => {
            Task::none()
        }
        Intent::Edited { text, caret, .. } => {
            let task = if app.core.state.aside_draft() == text {
                Task::none()
            } else {
                app.act(Action::SetAsideDraft(text))
            };
            Task::batch([task, typed(app, Field::Aside, caret)])
        }
        Intent::Caret { caret, .. } => follow(app, Field::Aside, caret),
        Intent::Send => app.dispatch(Action::SendAside),
        Intent::Focus(focused) => {
            app.vm.chat.thread_panel.composer.focused = focused;
            // Typing anywhere comes here now (`aside-composer-last`).
            if focused {
                app.aside_last = true;
            }
            Task::none()
        }
        Intent::Paste => files::paste(app, true),
        Intent::Attach => files::attach(app, true),
        Intent::Unstage(index) => app.dispatch(Action::UnstageAside(index)),
        Intent::ToggleSpoiler(index) => app.dispatch(Action::ToggleAsideSpoiler(index)),
        Intent::CancelReply => app.dispatch(Action::CancelAsideCompose),
        Intent::ToggleReplyPing => app.dispatch(Action::ToggleReplyPing { aside: true }),
        Intent::Suggest(key) => suggest(app, Field::Aside, key),
        // A thread has no menu, GIFs, stickers, commands or requests.
        intent => {
            tracing::debug!(?intent, "not the thread beside's composer's");
            Task::none()
        }
    }
}
