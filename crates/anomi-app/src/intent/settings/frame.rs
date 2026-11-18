//! The frame's intents (the sections, Close, the build under them) and My
//! Account's (`src/ui/callbacks.rs`'s `on_logout`, `on_edit_profile`,
//! `on_retry_sync`; the custom status, as upstream's be8166e8, closes
//! Settings and opens the status dialog).

use std::time::Duration;

use iced::Task;
use yapfast_core::application::Action;

use anomi_vm::settings::{account, frame};

use super::Due;
use crate::app::{App, later};
use crate::message::Message;

/// How long the build says it was copied.
const COPIED_FOR: Duration = Duration::from_secs(2);

pub(super) fn frame(app: &mut App, intent: frame::Intent) -> Task<Message> {
    use frame::Intent as I;
    match intent {
        I::Choose(page) => {
            let before = app.vm.settings.frame.page;
            if page == before {
                return Task::none();
            }
            super::left(app, before);
            app.vm.settings.frame.page = page;
            super::entered(app, page)
        }
        I::Close => super::close(app),
        I::EditProfile => edit_profile(app),
        // The sections filter as the field holds it: the frame alone.
        I::Search(query) => {
            app.vm.settings.frame.query = query;
            Task::none()
        }
        I::Copy => copy(app),
        intent => {
            tracing::debug!(?intent, "the person's settings keep no draft");
            Task::none()
        }
    }
}

pub(super) fn account(app: &mut App, intent: account::Intent) -> Task<Message> {
    use account::Intent as I;
    match intent {
        I::Reconnect => app.dispatch(Action::RetrySync),
        I::EditProfile => edit_profile(app),
        I::SetCustomStatus => {
            let closed = super::close(app);
            Task::batch([closed, crate::intent::status::open(app)])
        }
        I::SignOut => {
            let closed = super::close(app);
            Task::batch([closed, app.dispatch(Action::Logout)])
        }
        intent => {
            tracing::debug!(?intent, "not handled by the iced interface yet");
            Task::none()
        }
    }
}

/// "Edit Profile": Settings closes under the editor, which the projection
/// opens as the core shows it (I17).
fn edit_profile(app: &mut App) -> Task<Message> {
    let closed = super::close(app);
    Task::batch([closed, app.dispatch(Action::EditProfile)])
}

/// The build's line, for a report: copied, and said so for a moment.
pub(super) fn copy(app: &mut App) -> Task<Message> {
    let summary = crate::build_info::current().summary;
    let kept = &mut app.shown.settings.kept;
    kept.copied += 1;
    let copy = kept.copied;
    app.vm.settings.frame.copied = true;
    app.vm.settings.about.copied = true;
    Task::batch([
        iced::clipboard::write(summary).discard(),
        later(COPIED_FOR, Message::Settings(Due::Copied(copy))),
    ])
}

/// The moment a copy was said passed, unless another copy came since.
pub(super) fn copied_gone(app: &mut App, copy: u64) -> Task<Message> {
    if app.shown.settings.kept.copied == copy {
        app.vm.settings.frame.copied = false;
        app.vm.settings.about.copied = false;
    }
    Task::none()
}
