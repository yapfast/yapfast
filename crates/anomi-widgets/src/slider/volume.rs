//! `VolumeSlider` (`ui/controls.slint:1526`): how loud something sounds,
//! from 0% to `maximum`, under its label and the value as it is said. Above
//! 100%, as Discord allows for people, a mark shows 100% and a drag snaps to
//! it nearby. Dragging reports each step through `on_change` and the last
//! through `on_commit`; the arrow keys move `step` at a time.

use std::borrow::Cow;

use iced::Length;
use iced::widget::{column, row};

use anomi_theme::prelude::*;

use super::dial::Dial;
use super::header;
use super::track::{Axis, Span};

/// What a volume slider shows; the text is the caller's, translated.
#[derive(Clone, Debug)]
pub struct Volume<'a> {
    pub label: &'a str,
    pub value: i32,
    pub maximum: i32,
    /// The value as the slider says it: "{value}%" unless set.
    pub shown: Option<Cow<'a, str>>,
    pub step: i32,
    /// Marks 100% and snaps to it; on by default above 100.
    pub full_mark: bool,
}

impl<'a> Volume<'a> {
    /// A volume from 0% to 200%, 5 at a step.
    pub fn new(label: &'a str, value: i32) -> Self {
        Self::up_to(label, value, 200)
    }

    pub fn up_to(label: &'a str, value: i32, maximum: i32) -> Self {
        Self {
            label,
            value,
            maximum,
            shown: None,
            step: 5,
            full_mark: maximum > 100,
        }
    }

    pub fn shown(mut self, shown: impl Into<Cow<'a, str>>) -> Self {
        self.shown = Some(shown.into());
        self
    }

    pub fn step(mut self, step: i32) -> Self {
        self.step = step;
        self
    }

    pub fn full_mark(mut self, on: bool) -> Self {
        self.full_mark = on;
        self
    }
}

/// The slider under its header. `on_change` follows a drag, `on_commit`
/// keeps the value (after a drag or a key).
pub fn volume<'a, Message: Clone + 'a>(
    volume: Volume<'a>,
    on_change: impl Fn(i32) -> Message + 'a,
    on_commit: impl Fn(i32) -> Message + 'a,
) -> Element<'a, Message> {
    let shown = volume
        .shown
        .unwrap_or_else(|| Cow::Owned(format!("{}%", volume.value)));
    let dial = Dial::new(
        Axis::Across,
        Span {
            min: 0,
            max: volume.maximum,
        },
        volume.value,
        on_change,
    )
    .step(volume.step)
    .mark(volume.full_mark.then_some(100))
    .on_commit(on_commit);
    column![
        row![header::label(volume.label), header::value(shown)],
        dial
    ]
    .spacing(header::GAP)
    .width(Length::Fill)
    .boxed()
}
