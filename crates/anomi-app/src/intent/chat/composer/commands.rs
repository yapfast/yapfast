//! The slash commands' intents (`src/ui/app_commands.rs`, `bind`): a row
//! of the `/` list taken, an option's pill pressed, a file option's file,
//! and the keys while a command is written or the list shows: Up and Down
//! through the rows, kept in view, Tab and Enter to take one, Escape to
//! close the list and then the command, Backspace in the empty first
//! option to put its name back.

use iced::Task;
use yapfast_core::application::Action;

use anomi_chat::composer::commands::reveal;
use anomi_vm::chat::commands::Intent;
use anomi_vm::chat::composer::{Key, Suggest};
use anomi_widgets::focus;

use super::{files, show, words};
use crate::app::App;
use crate::message::Message;
use crate::project::chat::composer::Field;

/// A press on the list or on the written command's pills.
pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Pick(index) => pick(app, index),
        Intent::Option(index) => option(app, index),
        Intent::File(index) => files::command_file(app, index),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// A key while a command is written or the `/` list shows.
pub(super) fn key(app: &mut App, key: Key) -> Task<Message> {
    match key {
        Key::Tab(step) => {
            let Some(written) = &app.vm.chat.composer.commands.written else {
                return Task::none();
            };
            let len = written.options.len().max(1);
            let next = (written.focused + len).saturating_add_signed(step as isize) % len;
            option(app, next)
        }
        Key::File(index) => files::command_file(app, index),
        Key::Close { back } => {
            let task = app.dispatch(Action::CloseCommand(back));
            if back {
                Task::batch([task, words::put_caret(Field::Channel, usize::MAX)])
            } else {
                task
            }
        }
    }
}

/// A key the list's rows take while it has some.
pub(super) fn suggest(app: &mut App, key: Suggest) -> Task<Message> {
    let len = app.vm.chat.composer.commands.choices.len();
    match key {
        Suggest::Move(step) => {
            let words = &mut app.shown.composer.words;
            let index = i64::try_from(words.command).unwrap_or(0) + i64::from(step);
            let chosen = usize::try_from(index.rem_euclid(len.max(1) as i64)).unwrap_or(0);
            words.command = chosen;
            show(app);
            reveal(chosen)
        }
        Suggest::Choose => {
            let chosen = app.vm.chat.composer.commands.chosen;
            pick(app, chosen)
        }
        Suggest::Dismiss => app.dispatch(Action::CloseCommand(false)),
    }
}

/// A row taken: a command, which the composer then writes, or a value for
/// the focused option. The list starts again from its top, and the keys
/// stay with the composer, whose caret goes to the end of the text.
fn pick(app: &mut App, index: usize) -> Task<Message> {
    app.shown.composer.words.command = 0;
    let task = app.dispatch(Action::PickCommand(index));
    Task::batch([
        task,
        reveal(0),
        words::put_caret(Field::Channel, usize::MAX),
    ])
}

/// The composer writes the option at `index` now, its caret at the end.
fn option(app: &mut App, index: usize) -> Task<Message> {
    app.shown.composer.words.command = 0;
    let task = app.dispatch(Action::FocusCommandOption(index));
    Task::batch([task, words::put_caret(Field::Channel, usize::MAX)])
}

/// The command an app's answer names, written in the composer for its
/// picker (`use-command`, `ui/yapfast.slint:3907`).
pub(crate) fn use_command(app: &mut App, slash: String) -> Task<Message> {
    let task = app.dispatch(Action::SlashQuery(slash));
    Task::batch([task, focus::into(focus::COMPOSER)])
}
