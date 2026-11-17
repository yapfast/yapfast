//! `Float`, which lifts a widget off the page while it is dragged or held,
//! with the shadow Slint gives a raised card: no offset, a soft blur.
//!
//! iced draws a `Float`'s style at rest too, so the default has no shadow.
//! A caller sets [`lifted`] or [`dialog`] only while its content is lifted
//! (scaled or translated), and drops it when the content is set down.

use iced::widget::float::{Catalog, Style, StyleFn};
use iced::{Color, Shadow, Vector, border};

use crate::{Theme, metrics};

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(|_| Style::default())
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }
}

/// A raised card's shadow, `ui/interactions.slint:51`: 16 px, `#00000055`.
const LIFTED_BLUR: f32 = 16.0;
const LIFTED_ALPHA: u8 = 0x55;
/// A dialog's shadow, as 29 places in Slint have it (`ui/friends.slint:436`):
/// 32 px, `#00000066`, 12 px down.
const DIALOG_BLUR: f32 = 32.0;
const DIALOG_ALPHA: u8 = 0x66;
const DIALOG_OFFSET: f32 = 12.0;

fn shadow(blur_radius: f32, alpha: u8, offset: Vector, radius: f32) -> Style {
    Style {
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, alpha as f32 / 255.0),
            offset,
            blur_radius,
        },
        shadow_border_radius: border::radius(radius),
    }
}

/// A row picked up: a popover's shadow.
pub fn lifted(_theme: &Theme) -> Style {
    shadow(
        LIFTED_BLUR,
        LIFTED_ALPHA,
        Vector::ZERO,
        metrics::RADIUS_CARD,
    )
}

/// A dialog's shadow.
pub fn dialog(_theme: &Theme) -> Style {
    shadow(
        DIALOG_BLUR,
        DIALOG_ALPHA,
        Vector::new(0.0, DIALOG_OFFSET),
        metrics::RADIUS_DIALOG,
    )
}
