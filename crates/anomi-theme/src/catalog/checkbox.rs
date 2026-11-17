//! Checkboxes, as `Mark` (`ui/interactions.slint:116`) draws them: a box with a
//! 2 px ring, filled with the accent and ticked once on. Build it with
//! `.size(metrics::MARK_SIZE)`; iced's default is 16 px. The row's hover fill
//! is the row's own (`button::plain`), as in Slint. `Mark` has no disabled
//! look (`MarkRow`'s `enabled` only drops the row's hover), so a disabled box
//! draws as an enabled one.

use iced::widget::checkbox::{Catalog, Status, Style, StyleFn};
use iced::{Background, Border, Color};

use crate::{Theme, metrics};

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style {
        class(self, status)
    }

    fn selection(&self) -> Color {
        self.palette().accent.scale_alpha(0.35)
    }
}

/// The ring, the fill and the tick of the box at a status.
pub fn default(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let (Status::Active { is_checked }
    | Status::Hovered { is_checked }
    | Status::Disabled { is_checked }) = status;
    let ring = if is_checked {
        palette.accent
    } else {
        palette.secondary
    };
    Style {
        background: Background::Color(if is_checked {
            palette.accent
        } else {
            Color::TRANSPARENT
        }),
        icon_color: palette.on_accent,
        border: Border {
            color: ring,
            width: metrics::MARK_RING,
            radius: metrics::RADIUS_MARK.into(),
        },
        text_color: Some(palette.text),
    }
}
