//! Editing a message in place: its field's text, Enter and Escape and the
//! hint's two words (`inline_edit`, `src/ui/callbacks.rs:3966`). The edit
//! is the core's (`Action::Edit`, `SetEditText`, `SaveEdit`, `CancelEdit`);
//! `project::chat::edit` shows it in its row and gives its field the keys
//! as it starts. Saved or cancelled, the keys go back to the composer,
//! unless saving an emptied message asks whether to delete it.

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::chat;
use anomi_vm::chat::inline_edit::Intent;
use anomi_vm::chat::suggestions::Intent as Offered;
use anomi_widgets::focus;

use crate::app::App;
use crate::intent::chat::composer;
use crate::message::Message;
use crate::project::chat::composer::Field;

/// `chat::Intent::InlineEdit`.
pub fn handle(app: &mut App, intent: chat::Intent) -> Task<Message> {
    let chat::Intent::InlineEdit(intent) = intent else {
        tracing::debug!(?intent, "not the inline edit's");
        return Task::none();
    };
    match intent {
        Intent::Edited { key, text, caret } => {
            if !edits(app, key) {
                return Task::none();
            }
            // The caret's move may have reported the text already.
            let task = if app.core.state.edit().is_some_and(|edit| edit.text == text) {
                Task::none()
            } else {
                app.act(Action::SetEditText(text))
            };
            Task::batch([task, composer::typed(app, Field::Edit, caret)])
        }
        // The suggestions over the field follow its caret, and an emoji
        // from the picker goes in there.
        Intent::Caret { key, caret } if edits(app, key) => {
            composer::follow(app, Field::Edit, caret)
        }
        Intent::Suggest(key) => composer::suggest(app, Field::Edit, key),
        Intent::Offered(Offered::Choose(index)) => composer::choose(app, Field::Edit, index),
        Intent::Save => {
            let task = app.dispatch(Action::SaveEdit);
            Task::batch([task, back(app)])
        }
        Intent::Cancel => {
            let task = app.dispatch(Action::CancelEdit);
            Task::batch([task, back(app)])
        }
        // The emoji picker beside it (I12).
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Whether the edit shown in the row `key` is the core's.
fn edits(app: &App, key: u64) -> bool {
    app.vm.chat.timeline.editing == Some(key) && app.core.state.edit().is_some()
}

/// The keys go back to the composer as the edit ends (a saved one goes
/// once Discord took it); a question about deleting the emptied message
/// keeps them in its row.
fn back(app: &App) -> Task<Message> {
    if app.core.state.confirm_delete.is_some() {
        return Task::none();
    }
    focus::to(focus::COMPOSER)
}
