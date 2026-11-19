//! Asking again for the previews of others' streams in sight, as often as
//! their clients upload them (`yapfast_core::application::stream_previews`).
//! The call's projection notes which are in sight (`project::voice::place`):
//! their tiles in the grid, uncovered, the window in front. After each
//! update the core hears of a change (`Action::VisibleStreams`), and asks at
//! once for a preview that fell due while it didn't show; one wait is set
//! for the moment the next one falls due, and nothing ticks meanwhile.
//! Behind another window, minimised, in the tray or with the call off
//! screen none is in sight, so no wait runs; coming back asks once for
//! those that fell due.

use std::time::Duration;

use iced::Task;
use iced::task::Handle;
use yapfast_core::application::Action;
use yapfast_core::clock::now_ms;

use super::Due;
use crate::app::{App, later};
use crate::message::Message;

/// How far past the moment a preview falls due its wait ends, in
/// milliseconds, so the core's clock has passed it.
const PAST: u64 = 50;

/// The previews in sight, what the core was told of them, and the wait.
#[derive(Default)]
pub struct Previews {
    /// The streams whose previews are in sight, by user id, as the call was
    /// last placed.
    pub(crate) in_sight: Vec<String>,
    told: Vec<String>,
    /// The wait for the next preview due, and when, in Unix milliseconds,
    /// it falls due: dropped, its timer goes.
    wait: Option<(u64, Handle)>,
}

impl Previews {
    /// When the wait that runs ends, for the tests.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> Option<u64> {
        self.wait.as_ref().map(|(due, _)| *due)
    }
}

/// After each update: tells the core which previews are in sight when that
/// changed, and keeps one wait set for the next that falls due.
pub fn after(app: &mut App) -> Task<Message> {
    let previews = &mut app.shown.voice.previews;
    let told = if previews.in_sight == previews.told {
        Task::none()
    } else {
        previews.told.clone_from(&previews.in_sight);
        let users = previews.told.clone();
        app.dispatch(Action::VisibleStreams(users))
    };
    let due = app.core.stream_previews_due();
    let wait = &mut app.shown.voice.previews.wait;
    let Some(due) = due else {
        *wait = None;
        return told;
    };
    if wait.as_ref().is_some_and(|(set, _)| *set == due) {
        return told;
    }
    let delay = Duration::from_millis(due.saturating_sub(now_ms()) + PAST);
    let (task, timer) = later(delay, Message::Voice(Due::Previews(due))).abortable();
    *wait = Some((due, timer.abort_on_drop()));
    Task::batch([told, task])
}

/// The wait set for `due` ended: the core asks for the previews in sight
/// that fell due. One set again since, or dropped, came too late.
pub fn ended(app: &mut App, due: u64) -> Task<Message> {
    let wait = &mut app.shown.voice.previews.wait;
    if wait.as_ref().is_none_or(|(set, _)| *set != due) {
        return Task::none();
    }
    *wait = None;
    app.dispatch(Action::StreamPreviewsDue)
}
