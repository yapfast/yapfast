//! Scrollables: the thin scrollbar of the interface. Its thumb is always
//! shown, dim at rest and lit while the pointer is over the bar's strip or
//! drags it, as `ScrollBar` in `ui/scrollview.slint:65`, `:49` does. Each axis
//! lights on its own, as each `ScrollBar` has its own touch area (`:71`).
//!
//! A style can't set widths, so callers build
//! `scrollable(..).direction(Direction::Vertical(catalog::scrollable::bar()))`
//! to get the 2 px thumb set in from the edge and the ends. iced can't widen
//! it to 6 px on hover as Slint does (`:48`).

use iced::widget::container;
use iced::widget::scrollable::{AutoScroll, Rail, Scrollbar, Scroller, Status, Style};
use iced::{Background, Border, Color, Shadow, Vector};

use super::catalog_with_status;
use crate::{Theme, rgba};

/// The thumb at rest and lit, dark themes then light
/// (`ui/scrollview.slint:65`, `:49`; `anomi-widgets/src/scrollbar.rs:34-37`).
const THUMB_DARK: Color = rgba(0xffff_ff14);
const THUMB_LIGHT: Color = rgba(0x0000_0073);
const LIT_DARK: Color = rgba(0xffff_ff40);
const LIT_LIGHT: Color = rgba(0x0000_0099);

/// The thumb's width (`ui/scrollview.slint:29`).
pub const THUMB_WIDTH: f32 = 2.0;
/// The thumb's distance from the edge and from both ends (`ui/scrollview.slint:28`, `:63`).
pub const THUMB_INSET: f32 = 4.0;

/// The scrollbar with Slint's size: a 2 px thumb, its right edge 4 px in, and
/// 4 px clear at both ends; the strip that lights it is 10 px.
pub fn bar() -> Scrollbar {
    Scrollbar::new()
        .width(THUMB_WIDTH)
        .scroller_width(THUMB_WIDTH)
        .margin(THUMB_INSET)
        .padding(THUMB_INSET)
}

catalog_with_status!(scrollable, thin);

pub fn thin(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let (vertical, horizontal) = match status {
        Status::Active { .. } => (false, false),
        Status::Hovered {
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_hovered,
            ..
        } => (
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_hovered,
        ),
        Status::Dragged {
            is_vertical_scrollbar_dragged,
            is_horizontal_scrollbar_dragged,
            ..
        } => (
            is_vertical_scrollbar_dragged,
            is_horizontal_scrollbar_dragged,
        ),
    };
    let rail = |lit: bool| {
        let thumb = match (palette.dark, lit) {
            (true, false) => THUMB_DARK,
            (true, true) => LIT_DARK,
            (false, false) => THUMB_LIGHT,
            (false, true) => LIT_LIGHT,
        };
        Rail {
            background: None,
            border: Border::default(),
            scroller: Scroller {
                background: Background::Color(thumb),
                border: Border::default().rounded(f32::MAX),
            },
        }
    };
    Style {
        container: container::Style::default(),
        vertical_rail: rail(vertical),
        horizontal_rail: rail(horizontal),
        gap: None,
        auto_scroll: AutoScroll {
            background: Background::Color(palette.raised.scale_alpha(0.9)),
            border: Border::default()
                .rounded(u32::MAX)
                .width(1)
                .color(palette.edge),
            shadow: Shadow {
                color: Color::BLACK.scale_alpha(0.5),
                offset: Vector::ZERO,
                blur_radius: 2.0,
            },
            icon: palette.text,
        },
    }
}
