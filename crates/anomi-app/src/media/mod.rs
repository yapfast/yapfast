//! Media: the video playing in a message, in its cell and full screen
//! ([`video`]); its frames, and any moving picture's, from where they're
//! made to the widget that draws them ([`frames`]); the image viewer
//! ([`viewer`]); voice messages ([`voice`]). Task I09
//! (docs/iced/tasks/integration.md).
//!
//! The projection ([`show`]) runs after the timeline's, as it fills the
//! rows that play in place, under `Reach::VIDEO` for what the core says of
//! the players and the viewer. A frame costs a redraw of its rectangle and
//! nothing else; nothing runs while the window is behind or in the tray,
//! or what plays is paused, ended or out of view.

mod frames;
mod time;
mod video;
mod viewer;
mod voice;

use std::sync::Arc;

use iced::Task;
use yapfast_core::application::AppState;

use anomi_vm::Vm;
use anomi_vm::chat::Intent;

use crate::app::App;
use crate::message::Message;

pub use frames::{Alarm, Clock, Source, install};
pub use time::{video_time, voice_time};

/// What the window keeps of its media, across its views.
#[derive(Default)]
pub struct Media {
    /// The sources whose frames the window follows.
    alarm: Arc<Alarm>,
    /// What the alarm follows: the video playing in view, and the call's
    /// pictures in sight (I18's `project::voice`).
    playing: Option<Source>,
    call: Vec<Source>,
    /// The camera filmed to try it, on Voice & video (I19).
    test: Option<Source>,
    video: video::Shown,
    viewer: viewer::Shown,
}

/// Follows the frames of what moves on screen while the window lives: start
/// it once, as the app boots.
pub fn follow(shown: &crate::project::Shown) -> Task<Message> {
    frames::follow(shown.media.alarm.clone())
}

/// A frame the view must hold came: a video's first, or its bar moved a
/// step ([`frames`]).
pub fn frames(app: &mut App) -> Task<Message> {
    app.show(crate::project::Reach::VIDEO);
    Task::none()
}

/// `Video`, `Viewer` and `VoiceMessage` of `chat::Intent`.
pub fn handle(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::Video(intent) => video::handle(app, intent),
        Intent::Viewer(intent) => viewer::handle(app, intent),
        Intent::VoiceMessage(intent) => voice::handle(app, intent),
        intent => {
            tracing::debug!(?intent, "not media's");
            Task::none()
        }
    }
}

/// Shows the video, the viewer and the voice messages, after the rows were
/// built, and follows the frames of what can be seen: with the window in
/// front, the video full screen or its row in view.
pub fn show(state: &AppState, vm: &mut Vm, media: &mut Media) {
    let video = video::show(state, vm, &mut media.video);
    voice::show(state, vm);
    viewer::show(state, &mut vm.shell, &mut vm.chat.viewer, &mut media.viewer);
    media.playing = video.filter(|_| vm.chrome.focused);
    follow_all(media);
}

/// The call's pictures in sight now (`project::voice`), followed with the
/// video's.
pub fn follow_call(media: &mut Media, sources: Vec<Source>) {
    media.call = sources;
    follow_all(media);
}

/// The camera's test's frames, while it films.
pub fn follow_test(media: &mut Media, source: Option<Source>) {
    media.test = source;
    follow_all(media);
}

fn follow_all(media: &Media) {
    let sources = (media.playing.iter().chain(&media.call).chain(&media.test))
        .cloned()
        .collect();
    media.alarm.set(sources);
}

/// The timeline told which rows are in view: the video's frames are taken
/// while its row is.
pub fn seen(app: &mut App) {
    // Nothing to follow while nothing plays.
    if app.shown.media.video.plays() {
        show(&app.core.state, &mut app.vm, &mut app.shown.media);
    }
}

/// Escape or a press beside it closed the layer: through the player or the
/// core.
pub fn close(app: &mut App, layer: anomi_vm::shell::Layer) -> Task<Message> {
    match layer {
        anomi_vm::shell::Layer::Video => video::collapse(app),
        _ => viewer::close(app),
    }
}

/// A layer of media played its way out: what it showed goes.
pub fn gone(app: &mut App, layer: anomi_vm::shell::Layer) {
    match layer {
        anomi_vm::shell::Layer::Video => video::gone(app),
        _ => viewer::gone(app),
    }
}

/// The display's scale or the zoom changed: the video's pictures come at
/// the new number of pixels.
pub fn rescaled(app: &mut App) -> Task<Message> {
    // The call's cameras too, at their tiles' new number of pixels (I18).
    if !app.hidden {
        let shown = &mut app.shown;
        crate::project::voice::place(&app.core, &app.vm, &mut shown.voice, &mut shown.media);
    }
    video::rescaled(app)
}

#[cfg(test)]
mod tests;
