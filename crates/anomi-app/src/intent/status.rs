//! The custom status dialog's intents and the classic status menu's
//! (`src/ui/callbacks.rs:1613`-`:1634`): the draft the dialog edits is the
//! app's (`vm.people.status`), saving and clearing the core's; the dialog
//! opens and closes as the core's `status_editor` comes and goes
//! (`project::account::status`).

use iced::Task;
use yapfast_core::application::{Action, ClearAfter as CoreClearAfter};

use anomi_vm::chat::emoji_picker;
use anomi_vm::menu::Spot;
use anomi_vm::people::status::{self, ClearAfter, Emoji};
use anomi_vm::shell::Layer;

use crate::app::App;
use crate::message::Message;
use crate::pictures;
use crate::project::account::status::{custom_picture, unicode};

/// Status's intents.
pub fn handle(app: &mut App, intent: status::Intent) -> Task<Message> {
    use status::Intent;
    let draft = &mut app.vm.people.status;
    match intent {
        Intent::Text(text) => {
            draft.text = text;
            Task::none()
        }
        Intent::RemoveEmoji => {
            draft.emoji = None;
            Task::none()
        }
        Intent::ClearAfter(clear_after) => {
            draft.clear_after = clear_after;
            Task::none()
        }
        Intent::PickEmoji(spot) => pick_emoji(app, spot),
        Intent::Save => save(app),
        Intent::Clear => app.dispatch(Action::ClearCustomStatus),
        Intent::Close => close(app),
        Intent::Choose(choice) => super::account::choose(app, choice),
        Intent::CustomStatus => open(app),
        Intent::EditProfile => app.dispatch(Action::EditProfile),
        Intent::GameDurationDue => {
            super::account::game_due(app);
            Task::none()
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// Opens the dialog, filled from the status there is, with the keys in its
/// field (`init`, `ui/dialogs.slint:99`). The core opens it only with
/// Discord linked.
pub fn open(app: &mut App) -> Task<Message> {
    let task = app.dispatch(Action::OpenStatusEditor);
    if !app.vm.shell.is_open(Layer::Status) {
        return task;
    }
    Task::batch([
        task,
        anomi_widgets::field::focus(anomi_people::status::text_id()),
    ])
}

/// The X, Cancel, Escape and a press beside: the core lets the dialog go,
/// and the projection closes it.
pub fn close(app: &mut App) -> Task<Message> {
    app.dispatch(Action::CloseStatusEditor)
}

/// Saves the draft, never while a save is on its way.
fn save(app: &mut App) -> Task<Message> {
    let draft = &app.vm.people.status;
    if draft.saving {
        return Task::none();
    }
    let action = Action::SetCustomStatus {
        emoji: draft.emoji.as_ref().map(|emoji| emoji.key.clone()),
        text: draft.text.clone(),
        clear_after: match draft.clear_after {
            ClearAfter::Never => CoreClearAfter::Never,
            ClearAfter::Today => CoreClearAfter::Today,
            later => later
                .minutes()
                .map_or(CoreClearAfter::Never, CoreClearAfter::Minutes),
        },
    };
    app.dispatch(action)
}

/// The emoji button: the emoji picker beside it, whose pick comes back
/// through [`picked`] (`pick-emoji-beside`).
fn pick_emoji(app: &mut App, spot: Spot) -> Task<Message> {
    crate::intent::chat::pickers::for_status(app, spot)
}

/// An emoji picked for the status: the draft takes it.
pub fn picked(app: &mut App, picked: &emoji_picker::Emoji) {
    let emoji = match picked {
        emoji_picker::Emoji::Unicode(emoji) => unicode(emoji.as_str()),
        emoji_picker::Emoji::Custom(custom) => {
            let address = custom_picture(&custom.key);
            let picture = pictures::find(&app.core.state, address.as_deref());
            Emoji::custom(&custom.key, &picked.name(), picture)
        }
    };
    app.vm.people.status.emoji = Some(emoji);
}
