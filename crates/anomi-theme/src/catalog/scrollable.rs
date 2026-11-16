//! Scrollables: the thin scrollbar of the interface, which shows its thumb
//! only while the pointer is over the list, as `ui/scrollview.slint` does.

use iced::widget::container;
use iced::widget::scrollable::{AutoScroll, Rail, Scroller, Status, Style};
use iced::{Background, Border, Color, Shadow, Vector};

use super::catalog_with_status;
use crate::Theme;

catalog_with_status!(scrollable, thin);

pub fn thin(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let (shown, strong) = match status {
        Status::Active { .. } => (false, false),
        Status::Hovered {
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_hovered,
            ..
        } => (
            true,
            is_vertical_scrollbar_hovered || is_horizontal_scrollbar_hovered,
        ),
        Status::Dragged { .. } => (true, true),
    };
    let thumb = match (shown, strong) {
        (false, _) => Color::TRANSPARENT,
        (true, false) => palette.button_default,
        (true, true) => palette.button_default_hover,
    };
    let rail = Rail {
        background: None,
        border: Border::default(),
        scroller: Scroller {
            background: Background::Color(thumb),
            border: Border::default().rounded(4),
        },
    };
    Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
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
