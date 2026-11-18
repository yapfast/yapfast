//! Copying pictures: `Effect::CopyPicture`, a message's picture as it was
//! downloaded, which Copy Image asks the core for (`CopyPicture`,
//! `src/ui.rs`). `anomi_desktop::copy` decodes it and holds it on the
//! clipboard from a thread of its own, until something else is copied.
//! Text is copied through iced's clipboard where it's asked for.

use iced::Task;
use yapfast_core::application::Effect;

use crate::app::App;
use crate::message::Message;

pub(super) fn run(app: &mut App, effect: Effect) -> Task<Message> {
    match effect {
        Effect::CopyPicture(file) => copy(app, file),
        effect => return super::waiting(&effect),
    }
    Task::none()
}

#[cfg(not(test))]
fn copy(_app: &mut App, file: Vec<u8>) {
    anomi_desktop::copy::picture(file);
}

/// A test reaches no clipboard: it counts the pictures copied.
#[cfg(test)]
fn copy(app: &mut App, _file: Vec<u8>) {
    app.asked.copied += 1;
}
