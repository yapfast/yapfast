//! The core's effects, run: `execute` in `src/ui.rs` of the Slint app.
//! Each becomes nothing, a call made here, or a `Task` iced runs.
//!
//! [`run`] hands each effect to its module, its I task's
//! (docs/iced/tasks/integration.md), which takes the effect whole and
//! returns what iced runs for it: notifications, sounds, opening pages and
//! copying pictures (I15), audio devices and the camera's preview (I18),
//! linking Discord (I05). Until its task, a module logs what it gets.
//!
//! In a test nothing reaches the desktop: no browser opens, no clipboard or
//! notification service is asked, and the app keeps what was asked in
//! [`Asked`] instead.

mod clipboard;
mod devices;
pub mod link;
pub(crate) mod notify;
mod page;
pub(crate) mod sound;

use iced::Task;
use yapfast_core::application::Effect;

use crate::app::App;
use crate::message::Message;
use crate::{backend, layout};

pub fn run(app: &mut App, effects: Vec<Effect>) -> Task<Message> {
    let mut tasks = Vec::new();
    for effect in effects {
        let task = match effect {
            Effect::Start(start) => {
                // Each server opens on the channel it showed last.
                layout::carry::channels(&app.layout, &mut app.core);
                backend::start(app, start)
            }
            Effect::Backend(command) if kept_back(app, &command) => continue,
            Effect::Backend(command) => {
                backend::send(app, command);
                continue;
            }
            Effect::Notify(_) => notify::run(app, effect),
            Effect::Sound(_) => sound::run(app, effect),
            Effect::ListDevices | Effect::PreviewCamera(_) => devices::run(app, effect),
            Effect::LinkDiscord | Effect::LinkDiscordToken(_) | Effect::StopDiscordLink => {
                link::run(app, effect)
            }
            Effect::OpenPage(_) => page::run(app, effect),
            Effect::CopyPicture(_) => clipboard::run(app, effect),
        };
        tasks.push(task);
    }
    Task::batch(tasks)
}

/// A command the person's settings keep from the backend: typing, where
/// they turned off saying so (`share_typing`, `src/ui.rs:3114`).
pub(crate) fn kept_back(app: &App, command: &yapfast_core::chat::Command) -> bool {
    matches!(command, yapfast_core::chat::Command::Typing { .. })
        && !app.layout.share_typing.unwrap_or(true)
}

/// What the effects asked of the desktop in a test.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct Asked {
    /// The pages opened in the browser.
    pub pages: Vec<String>,
    /// The notifications shown: their conversation and the text they say,
    /// empty where they say only that a message came.
    pub notified: Vec<(String, String)>,
    /// How many pictures were copied.
    pub copied: usize,
}

/// An effect not run yet, until its module's task.
fn waiting(effect: &Effect) -> Task<Message> {
    tracing::debug!(effect = name(effect), "not run by the iced interface yet");
    Task::none()
}

fn name(effect: &Effect) -> &'static str {
    match effect {
        Effect::Start(_) => "Start",
        Effect::Backend(_) => "Backend",
        Effect::Notify(_) => "Notify",
        Effect::Sound(_) => "Sound",
        Effect::ListDevices => "ListDevices",
        Effect::LinkDiscord => "LinkDiscord",
        Effect::LinkDiscordToken(_) => "LinkDiscordToken",
        Effect::StopDiscordLink => "StopDiscordLink",
        Effect::OpenPage(_) => "OpenPage",
        Effect::CopyPicture(_) => "CopyPicture",
        Effect::PreviewCamera(_) => "PreviewCamera",
    }
}
