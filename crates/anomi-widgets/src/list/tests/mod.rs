//! The list driven headless through iced's simulator: rows of mixed heights
//! whose tops are found by their labels, with a count of the rows alive.

mod anchoring;
mod copies;
mod drags;
mod gestures;
mod virtual_rows;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use iced::advanced::widget::Id;
use iced::widget::{container, text};
use iced::{Event, Fill, Point, Settings, Size, mouse, window};
use iced_test::Simulator;

use anomi_theme::Renderer;
use anomi_theme::prelude::*;

use super::{Drag, List, Viewport, list};

/// The list's place: 600 px tall.
pub const VIEW: Size = Size::new(400.0, 600.0);
pub const ID: &str = "list";

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Seen(Viewport),
    Dragged(Drag),
}

/// Rows of known heights, keyed by number, each labelled "Row N" at its top.
pub struct Rows {
    pub keys: Vec<u64>,
    pub heights: Vec<f32>,
    /// One clone a row alive.
    alive: Rc<()>,
    /// The most alive at once since it was last read.
    most: Cell<usize>,
}

impl Rows {
    /// `count` rows, keyed from `first`, between 24 and 88 px tall.
    pub fn mixed(first: u64, count: usize) -> Self {
        let keys: Vec<u64> = (first..first + count as u64).collect();
        let heights = keys.iter().map(|key| height(*key)).collect();
        Self::with(keys, heights)
    }

    /// Rows all `each` tall.
    pub fn even(count: usize, each: f32) -> Self {
        Self::with((0..count as u64).collect(), vec![each; count])
    }

    /// These rows, each its height.
    pub fn with(keys: Vec<u64>, heights: Vec<f32>) -> Self {
        Self {
            keys,
            heights,
            alive: Rc::new(()),
            most: Cell::new(0),
        }
    }

    /// The rows alive now: built for the view or kept for the next.
    pub fn alive(&self) -> usize {
        Rc::strong_count(&self.alive) - 1
    }

    /// The most rows alive at once since the last call, counting those a
    /// layout built and dropped within it.
    pub fn most_alive(&self) -> usize {
        self.most.replace(self.alive())
    }

    pub fn list(&self, motion: Motion) -> List<'_, Message> {
        list(
            self.keys.len(),
            |index| self.keys[index],
            move |index| self.row(index),
            motion,
        )
        .id(Id::new(ID))
    }

    fn row(&self, index: usize) -> Element<'_, Message> {
        let guard = self.alive.clone();
        self.most.set(self.most.get().max(self.alive()));
        container(text(label(self.keys[index])).size(12))
            .height(self.heights[index])
            .width(Fill)
            .style(move |_: &Theme| {
                let _ = &guard;
                container::Style::default()
            })
            .boxed()
    }
}

/// A row's height by its key: 24 to 88 px.
pub fn height(key: u64) -> f32 {
    24.0 + (key * 7 % 5) as f32 * 16.0
}

pub fn label(key: u64) -> String {
    format!("Row {key}")
}

pub fn simulate(list: List<'_, Message>) -> Simulator<'_, Message, Theme, Renderer> {
    Simulator::with_size(Settings::default(), VIEW, list.boxed())
}

pub fn front() -> Motion {
    Motion {
        front: true,
        ..Motion::default()
    }
}

pub fn still() -> Motion {
    Motion::default()
}

/// The top of row `key`'s label, if the row is built.
pub fn top(ui: &mut Simulator<'_, Message, Theme, Renderer>, key: u64) -> Option<f32> {
    ui.find(label(key)).ok().map(|found| found.bounds().y)
}

/// A frame at `at`.
pub fn frame(ui: &mut Simulator<'_, Message, Theme, Renderer>, at: Instant) {
    let _ = ui.simulate([Event::Window(window::Event::RedrawRequested(at))]);
}

/// Frames until whatever moves has landed.
pub fn settle(ui: &mut Simulator<'_, Message, Theme, Renderer>) {
    let start = Instant::now();
    for step in 0..4 {
        frame(ui, start + Duration::from_secs(1 + step));
    }
}

/// Pixels scrolled, towards the end when positive, as a touchpad does.
pub fn scroll(ui: &mut Simulator<'_, Message, Theme, Renderer>, pixels: f32) {
    ui.point_at(Point::new(200.0, 300.0));
    let _ = ui.scroll(mouse::ScrollDelta::Pixels { x: 0.0, y: -pixels });
}

pub fn moved_to(position: Point) -> Event {
    Event::Mouse(mouse::Event::CursorMoved { position })
}

pub fn press(button: mouse::Button) -> Event {
    Event::Mouse(mouse::Event::ButtonPressed(button))
}

pub fn release(button: mouse::Button) -> Event {
    Event::Mouse(mouse::Event::ButtonReleased(button))
}
