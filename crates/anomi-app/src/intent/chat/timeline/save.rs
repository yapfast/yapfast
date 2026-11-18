//! Saving a message's attachment to a file (`save-attachment`,
//! `src/ui/callbacks.rs:2635`): the file dialog, off the window's thread,
//! suggests the attachment's name, and the core downloads it to the path
//! chosen. Closed without a choice, nothing is saved.

use std::path::Path;

use iced::Task;
use yapfast_core::application::Action;
use yapfast_lang::tr;

use crate::app::App;
use crate::message::{Message, Once};

/// Asks where to save the attachment `index` of the message `key`.
pub(crate) fn choose(app: &mut App, key: u64, index: usize) -> Task<Message> {
    let Some(message) = app.shown.holding(key).message(key) else {
        return Task::none();
    };
    let item = message.item.clone();
    choose_item(app, &item, index)
}

/// As [`choose`], for the message `item` of the open room or the thread
/// beside: the picture large's, the video full screen's.
pub(crate) fn choose_item(app: &App, item: &str, index: usize) -> Task<Message> {
    let Some(message) = app.core.state.message(item) else {
        return Task::none();
    };
    let item = item.to_owned();
    let name = (message.attachments.get(index)).map_or("", |attachment| &attachment.name);
    let name = suggested(name);
    let pick = move || {
        rfd::FileDialog::new()
            .set_title(tr!("Save attachment"))
            .set_file_name(name)
            .save_file()
    };
    Task::perform(
        async move { tokio::task::spawn_blocking(pick).await.ok().flatten() },
        |path| path,
    )
    .and_then(move |path| {
        let save = Action::SaveAttachment {
            item: item.clone(),
            index,
            path,
        };
        Task::done(Message::Act(Once::new(save)))
    })
}

/// The name the dialog suggests: the file's name alone, so a sender can't
/// choose the folder, hide the file behind a leading dot, or turn how its
/// extension reads with characters that change the text's direction.
fn suggested(name: &str) -> String {
    let name = (Path::new(name).file_name())
        .map(|name| {
            (name.to_string_lossy().chars())
                .filter(|&c| !c.is_control() && !yapfast_core::links::hidden(c))
                .collect::<String>()
                .trim_start_matches(['.', ' '])
                .to_owned()
        })
        .unwrap_or_default();
    if name.is_empty() {
        "attachment".into()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_saved_file_is_suggested_by_its_name_alone() {
        assert_eq!(super::suggested("../../.bashrc"), "bashrc");
        assert_eq!(super::suggested("photo\u{202e}gpj.exe"), "photogpj.exe");
        assert_eq!(super::suggested(""), "attachment");
        assert_eq!(super::suggested("report.pdf"), "report.pdf");
    }
}
