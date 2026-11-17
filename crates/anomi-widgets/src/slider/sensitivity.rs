//! `SensitivitySlider` (`ui/controls.slint:1818`): the Discord voice gate's
//! sensitivity, in dBFS from -80 to 0, over the microphone's level as the
//! gate hears it: green up to -18 dBFS, orange to -6, red above. A handle
//! marks the sensitivity. Dragging reports each step through `on_change` and
//! the last through `on_commit`; the arrow keys move 1 dB.

use iced::Length;
use iced::widget::{column, row};

use yapfast_lang::tr;

use anomi_theme::prelude::*;

use super::dial::{Backdrop, Dial};
use super::header;
use super::track::{Axis, Span};

/// The sensitivity's range.
pub const SPAN: Span = Span { min: -80, max: 0 };

/// The gap between the label, the gate's words and the value
/// (`ui/controls.slint:1856`, `spacing: 10px`).
const HEADER_GAP: f32 = 10.0;

/// What the slider shows. The label is the caller's, translated; the gate's
/// words and the value's unit are the widget's own, as in Slint.
#[derive(Clone, Copy, Debug)]
pub struct Sensitivity<'a> {
    pub label: &'a str,
    pub value: i32,
    /// The microphone's level in dBFS; below -80 shows nothing.
    pub level: f32,
    /// The gate is open: the microphone would be sent.
    pub sending: bool,
}

/// The slider under its header.
pub fn sensitivity<'a, Message: Clone + 'a>(
    sensitivity: Sensitivity<'a>,
    on_change: impl Fn(i32) -> Message + 'a,
    on_commit: impl Fn(i32) -> Message + 'a,
) -> Element<'a, Message> {
    let words = if sensitivity.sending {
        tr!("Sending")
    } else {
        tr!("Not sending")
    };
    let gate = header::gate(words, sensitivity.sending);
    let shown = tr!("{} dB", sensitivity.value);
    let dial = Dial::new(Axis::Across, SPAN, sensitivity.value, on_change)
        .backdrop(Backdrop::Level(sensitivity.level))
        .on_commit(on_commit);
    column![
        row![header::label(sensitivity.label), gate, header::value(shown)].spacing(HEADER_GAP),
        dial
    ]
    .spacing(header::GAP)
    .width(Length::Fill)
    .boxed()
}
