//! The core's effects, run: `execute` in `src/ui.rs` of the Slint app.
//! Each becomes nothing, a call made here, or a `Task` iced runs.
//!
//! The skeleton runs the session's: starting the backend and sending it
//! commands. The rest come with their tasks (docs/iced/tasks/integration.md):
//! notifications, sounds, opening pages and copying pictures (I15), audio
//! devices and the camera's preview (I18), linking Discord (I05). Until
//! then they're logged.

mod clipboard;
mod devices;
mod link;
mod notify;
mod page;
mod sound;

use iced::Task;
use yapfast_core::application::Effect;

use crate::app::App;
use crate::backend;
use crate::message::Message;

pub fn run(app: &mut App, effects: Vec<Effect>) -> Task<Message> {
    let mut tasks = Vec::new();
    for effect in effects {
        match effect {
            Effect::Start(start) => tasks.push(backend::start(app, start)),
            Effect::Backend(command) => backend::send(app, command),
            other => tracing::debug!(effect = name(&other), "not run by the iced interface yet"),
        }
    }
    Task::batch(tasks)
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
