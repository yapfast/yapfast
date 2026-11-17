//! One 36 px row of a list: a mark, then what it is called, as a button that
//! lights under the pointer, and stays lit while it is the chosen one.

use iced::widget::{button, container, row};
use iced::{Alignment, Border, Fill, Padding};

use anomi_theme::metrics;
use anomi_theme::prelude::*;
use anomi_vm::chat::suggestions::Intent;

pub const HEIGHT: f32 = 36.0;
/// `padding-left: 8px; padding-right: 10px; spacing: 10px`.
const LEFT: f32 = 8.0;
const RIGHT: f32 = 10.0;
pub const GAP: f32 = 10.0;

/// `parts` in a row that sends [`Intent::Choose`] with `index`; `chosen` is
/// the highlighted one (`index == mention-index`).
pub fn choice<'a>(
    index: usize,
    chosen: bool,
    parts: impl IntoIterator<Item = Element<'a, Intent>>,
) -> Element<'a, Intent> {
    let content = row(parts)
        .spacing(GAP)
        .align_y(Alignment::Center)
        .height(Fill);
    button(content)
        .padding(Padding {
            top: 0.0,
            right: RIGHT,
            bottom: 0.0,
            left: LEFT,
        })
        .width(Fill)
        .height(HEIGHT)
        .on_press(Intent::Choose(index))
        .style(move |theme: &Theme, status| {
            let palette = theme.palette();
            let fill = if chosen {
                Some(palette.active)
            } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                Some(palette.hover)
            } else {
                None
            };
            button::Style {
                background: fill.map(Into::into),
                text_color: palette.text,
                border: Border::default().rounded(metrics::RADIUS_ROW),
                ..button::Style::default()
            }
        })
        .boxed()
}

/// A fixed-size box holding `content` in its middle, a mark's room.
pub fn mark<'a>(content: Element<'a, Intent>, side: f32) -> Element<'a, Intent> {
    container(content).center(side).boxed()
}
