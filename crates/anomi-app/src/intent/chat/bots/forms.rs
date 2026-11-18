//! A bot's form and its select menus (`src/ui/callbacks.rs:839`-`:917`):
//! each change goes to the core, which the projection shows. A menu's
//! choice sent from a message shows in its box at once, until the bot
//! answers (`project::chat::kept::Chosen`).

use iced::Task;
use yapfast_core::application::Action;
use yapfast_lang::tr;

use anomi_vm::chat::bot_forms::Intent;
use anomi_vm::chat::embeds::Intent as EmbedsIntent;

use crate::app::App;
use crate::message::{Message, Once};
use crate::project::chat::kept::Chosen;

pub(super) fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Edit(index, value) => app.dispatch(Action::SetModalField { index, value }),
        Intent::OpenSelect(index, spot) => {
            app.shown.bot_forms.at = Some(spot);
            app.dispatch(Action::OpenModalSelect(index))
        }
        Intent::ChooseFiles(index) => choose_files(index),
        Intent::RemoveFile(index, file) => app.dispatch(Action::RemoveModalFile { index, file }),
        // A link in the text to read, as an embed's.
        Intent::Link(link) => super::super::timeline::handle(
            app,
            anomi_vm::chat::Intent::Embeds(EmbedsIntent::Link(link)),
        ),
        Intent::Submit => app.dispatch(Action::SubmitModal),
        Intent::Close => app.dispatch(Action::CloseModal),
        Intent::Toggle(index) => toggle(app, index),
        Intent::Search(query) => app.dispatch(Action::SearchSelect(query)),
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// The file dialog for a form's file upload, as the composer's
/// (`bot-modal-choose-files`); nothing comes of one closed without a choice.
fn choose_files(index: usize) -> Task<Message> {
    let pick = || {
        rfd::FileDialog::new()
            .set_title(tr!("Choose files"))
            .pick_files()
    };
    Task::perform(
        async move { tokio::task::spawn_blocking(pick).await.ok().flatten() },
        |files| files,
    )
    .and_then(move |files| {
        Task::done(Message::Act(Once::new(Action::AddModalFiles {
            index,
            files,
        })))
    })
}

/// An option of the open menu: a menu of one option sends it at once,
/// which closes it, and its box shows it until the bot answers; the window
/// then shows the message's controls held back.
fn toggle(app: &mut App, index: usize) -> Task<Message> {
    let sent = from_message(app, |menu| {
        let choice = menu.choices.get(index).filter(|_| menu.max == 1)?;
        Some(choice.label.clone())
    });
    let task = app.dispatch(Action::ToggleSelect(index));
    if app.core.state.select_menu.is_none() {
        chosen(app, sent);
        app.refresh();
    }
    task
}

/// Closes the open menu through the core: one of several options sends
/// what changed, which its box shows until the bot answers.
pub(crate) fn close_select(app: &mut App) -> Task<Message> {
    let sent = from_message(app, |menu| {
        let picked = menu.choices.iter().filter(|choice| choice.selected);
        let labels: Vec<&str> = picked.map(|choice| choice.label.as_str()).collect();
        (menu.max > 1).then(|| labels.join(", "))
    });
    let task = app.dispatch(Action::CloseSelect);
    chosen(app, sent);
    task
}

/// What the open menu of a message would show as chosen, with its item and
/// custom id; none for a form's.
fn from_message(
    app: &App,
    text: impl FnOnce(&yapfast_core::application::SelectMenu) -> Option<String>,
) -> Option<(String, String, String)> {
    let menu = app.core.state.select_menu.as_ref()?;
    if menu.modal_field.is_some() {
        return None;
    }
    Some((menu.item.clone(), menu.custom_id.clone(), text(menu)?))
}

/// The menu's choice went to its bot: its box shows it while it waits.
fn chosen(app: &mut App, sent: Option<(String, String, String)>) {
    let Some((item, custom_id, text)) = sent else {
        return;
    };
    let state = &app.core.state;
    if state.select_menu.is_some() || !state.interactions.busy(&item) {
        return;
    }
    let message = state.messages.iter().find(|message| message.item == item);
    let Some(message) = message.cloned() else {
        return;
    };
    let kept = &mut app.shown.timeline.kept;
    kept.chosen
        .retain(|chosen| chosen.item != item || chosen.custom_id != custom_id);
    kept.chosen.push(Chosen::new(&message, custom_id, text));
    let key = anomi_vm::chat::timeline::group::key(&message.id);
    app.shown.timeline.again.push(key);
}
