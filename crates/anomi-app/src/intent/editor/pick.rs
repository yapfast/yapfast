//! Choosing an avatar or a banner: the file dialog (`pick-profile-picture`,
//! `src/ui/callbacks.rs:1553`), off the window's thread, and the core reads
//! the file it chose. The Slint app crops nothing, and neither does this:
//! Discord takes the picture as it is. The test session's `pick avatar
//! PATH` and `pick banner PATH` (`crate::control`) stand for the dialog.

use std::path::PathBuf;

use iced::Task;
use yapfast_core::application::Action;
use yapfast_core::chat::ProfilePicture;
use yapfast_lang::tr;

use crate::app::App;
use crate::message::{Message, Once};

/// The kinds of file the dialog offers.
const PICTURES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];

/// Opens the file dialog for the avatar or the banner, where this account
/// may change it and nothing is read or saved meanwhile; nothing comes of
/// one closed without a choice.
pub(super) fn choose(app: &mut App, kind: ProfilePicture) -> Task<Message> {
    let ready = app
        .core
        .state
        .profile_editor
        .as_ref()
        .is_some_and(|editor| {
            editor.may_picture(kind)
                && editor.profile.is_some()
                && !editor.saving
                && editor.reading.is_none()
        });
    if !ready {
        return Task::none();
    }
    let title = match kind {
        ProfilePicture::Avatar => tr!("Change Avatar"),
        ProfilePicture::Banner => tr!("Change Banner"),
    };
    let pick = move || {
        rfd::FileDialog::new()
            .set_title(title)
            .add_filter(tr!("Pictures"), &PICTURES)
            .pick_file()
    };
    Task::perform(
        async move { tokio::task::spawn_blocking(pick).await.ok().flatten() },
        |path| path,
    )
    .and_then(move |path| Task::done(chosen(kind, path)))
}

/// The file chosen for `kind`, for the core to read.
pub(crate) fn chosen(kind: ProfilePicture, path: PathBuf) -> Message {
    Message::Act(Once::new(Action::PickProfilePicture { kind, path }))
}
