//! The line over a slider: its label, grey and bold at 13 px, and the value
//! as it is said (`ui/controls.slint:1563`, `:1854`, `:1855`).

use std::borrow::Cow;

use iced::Length;
use iced::widget::text;
use iced::widget::text::{Ellipsis, Wrapping};

use anomi_theme::{Theme, catalog, typography};

/// The header's text size, `font-size: 13px`.
pub const SIZE: f32 = typography::size::LABEL;

/// The gap between the header and the track (`spacing: 6px`).
pub const GAP: f32 = 6.0;

/// The label: it takes the width the value leaves and ends in an ellipsis
/// where it does not fit (`overflow: elide`).
pub fn label<'a>(label: &'a str) -> iced::widget::Text<'a, Theme> {
    text(label)
        .size(SIZE)
        .font(typography::SEMIBOLD)
        .width(Length::Fill)
        .wrapping(Wrapping::None)
        .ellipsis(Ellipsis::End)
        .style(catalog::text::secondary)
}

/// The value, in the text colour.
pub fn value<'a>(shown: impl Into<Cow<'a, str>>) -> iced::widget::Text<'a, Theme> {
    text(shown.into())
        .size(SIZE)
        .font(typography::SEMIBOLD)
        .style(catalog::text::primary)
}

/// Whether the gate sends: "Sending" in the live colour, "Not sending" muted.
pub fn gate<'a>(shown: &'a str, sending: bool) -> iced::widget::Text<'a, Theme> {
    text(shown)
        .size(SIZE)
        .font(typography::SEMIBOLD)
        .style(move |theme: &Theme| text::Style {
            color: Some(if sending {
                theme.palette().live
            } else {
                theme.palette().muted
            }),
            selection: None,
        })
}
