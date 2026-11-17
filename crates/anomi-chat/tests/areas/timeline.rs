//! The timeline: task C01's tests, driving `anomi_chat::timeline::view` over
//! its fixtures in the simulator (`super::support`): what shows without
//! messages and at the history's edges, the bars, the keys, and what the
//! list tells.

mod bars;
mod edges;
mod geometry;
mod keys;
mod reports;

use std::time::Instant;

use iced::{Event, Size, window};
use iced_test::Simulator;

use anomi_chat::timeline;
use anomi_theme::{Motion, Renderer, Theme};
use anomi_vm::chat::{
    self,
    timeline::{Intent, Shown, Timeline},
};

use super::support::{front, settings};

/// The timeline's place in the golden window: the conversation less its
/// header, composer and member list.
pub const SIZE: Size = Size::new(760.0, 700.0);

pub fn simulate(
    timeline: &Timeline,
    motion: Motion,
) -> Simulator<'_, chat::Intent, Theme, Renderer> {
    use iced::Widget as _;
    Simulator::with_size(settings(), SIZE, timeline::view(timeline, motion).boxed())
}

/// The window in front.
pub fn shown(timeline: &Timeline) -> Simulator<'_, chat::Intent, Theme, Renderer> {
    simulate(timeline, front())
}

/// `ui` over `timeline` now, in front, keeping the state of what stays.
pub fn rebuild<'a>(
    ui: Simulator<'a, chat::Intent, Theme, Renderer>,
    timeline: &'a Timeline,
) -> Simulator<'a, chat::Intent, Theme, Renderer> {
    ui.rebuild(timeline::view(timeline, front()))
}

/// A frame at `at`, on which the list tells what shows.
pub fn frame(ui: &mut Simulator<'_, chat::Intent, Theme, Renderer>, at: Instant) {
    let _ = ui.simulate([Event::Window(window::Event::RedrawRequested(at))]);
}

/// The timeline's intents among those gathered.
fn intents(asked: impl IntoIterator<Item = chat::Intent>) -> Vec<Intent> {
    asked
        .into_iter()
        .filter_map(|intent| match intent {
            chat::Intent::Timeline(intent) => Some(intent),
            _ => None,
        })
        .collect()
}

/// What was asked, but for the list's reports of what shows.
pub fn asked(ui: &mut Simulator<'_, chat::Intent, Theme, Renderer>) -> Vec<Intent> {
    intents(ui.drain())
        .into_iter()
        .filter(|intent| !matches!(intent, Intent::Shown(_)))
        .collect()
}

/// The list's reports among what was asked.
pub fn reports(ui: &mut Simulator<'_, chat::Intent, Theme, Renderer>) -> Vec<Shown> {
    intents(ui.drain())
        .into_iter()
        .filter_map(|intent| match intent {
            Intent::Shown(shown) => Some(shown),
            _ => None,
        })
        .collect()
}

/// Whether `text` shows.
pub fn shows(ui: &mut Simulator<'_, chat::Intent, Theme, Renderer>, text: &str) -> bool {
    ui.find(text).is_ok()
}
