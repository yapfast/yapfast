//! Keeps iced's window-wide selection out of the rows: the window selects
//! across every text between two points with a drag no widget took, and
//! copies what every text selected on a Ctrl+C nothing took. In the rows
//! the selection is this module's, so operations from outside reach their
//! texts with selecting and copying turned off; their words stay readable,
//! so a test still finds a row by its text.

use std::any::Any;

use iced::advanced::text::{Fragment, Target};
use iced::advanced::widget::Id;
use iced::advanced::widget::operation::{
    Focusable, Operation, Outcome, Scrollable, Text, TextInput,
};
use iced::{Point, Rectangle, Size, Vector};

/// An operation from outside, as it reaches the rows.
pub struct Guard<'a> {
    inner: &'a mut dyn Operation,
}

impl<'a> Guard<'a> {
    pub fn new(inner: &'a mut dyn Operation) -> Self {
        Self { inner }
    }
}

impl Operation for Guard<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        self.inner
            .traverse(&mut |inner| operate(&mut Guard { inner }));
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle, viewport: &Rectangle) {
        self.inner.container(id, bounds, viewport);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Size,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        self.inner
            .scrollable(id, bounds, content, translation, state);
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        self.inner.focusable(id, bounds, state);
    }

    fn text_input(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn TextInput) {
        self.inner.text_input(id, bounds, state);
    }

    fn text(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Text) {
        self.inner.text(id, bounds, &mut Shielded(state));
    }

    fn custom(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Any) {
        self.inner.custom(id, bounds, state);
    }

    fn finish(&self) -> Outcome<()> {
        self.inner.finish()
    }
}

/// A row's text as the window's selection sees it: readable, not selectable.
struct Shielded<'a>(&'a mut dyn Text);

impl Text for Shielded<'_> {
    fn text(&self) -> Fragment<'_> {
        self.0.text()
    }

    fn select(&mut self, _start: Point, _end: Point, _target: Target) {}

    fn select_all(&mut self) {}

    fn deselect(&mut self) {}

    fn copy(&mut self) -> Option<String> {
        None
    }
}
