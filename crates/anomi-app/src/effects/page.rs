//! Opening pages in the browser people chose: `Effect::OpenPage`, which the
//! core asks for (Discord's captcha, a release's page) and the window does
//! for a link that leaves it (`platform::links`) or the title bar's help
//! (`OpenPage`, `src/ui.rs`). `anomi_desktop::browser` starts the browser
//! and waits for it on a thread of its own.

use iced::Task;
use yapfast_core::application::Effect;

use crate::app::App;
use crate::message::Message;

pub(super) fn run(app: &mut App, effect: Effect) -> Task<Message> {
    match effect {
        Effect::OpenPage(page) => open(app, page),
        effect => return super::waiting(&effect),
    }
    Task::none()
}

#[cfg(not(test))]
pub(crate) fn open(_app: &mut App, page: String) {
    anomi_desktop::browser::open(&page);
}

/// A test opens no browser: it keeps the page asked for.
#[cfg(test)]
pub(crate) fn open(app: &mut App, page: String) {
    app.asked.pages.push(page);
}
