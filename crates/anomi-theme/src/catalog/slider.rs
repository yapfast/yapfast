//! Sliders, as `VolumeSlider` (`ui/controls.slint:1526`) draws them: a 4 px rail,
//! the accent up to the value and a control's face after it, and a 14 px
//! handle in the text colour. The handle's focus ring is the widget's.

use iced::widget::slider::{Catalog, Handle, HandleShape, Rail, Status, Style, StyleFn};
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
}

pub fn default(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    // The handle grows a ring in the accent while it is held, as Slint's does
    // while it has the focus a press gives it.
    let (width, ring) = match status {
        Status::Dragged => (metrics::SLIDER_RING, palette.accent),
        Status::Active | Status::Hovered => (0.0, Color::TRANSPARENT),
    };
    Style {
        rail: Rail {
            backgrounds: (
                Background::Color(palette.accent),
                Background::Color(palette.button_default),
            ),
            width: metrics::SLIDER_RAIL,
            border: Border::default().rounded(metrics::SLIDER_RAIL / 2.0),
        },
        handle: Handle {
            shape: HandleShape::Circle {
                radius: metrics::SLIDER_HANDLE / 2.0,
            },
            background: Background::Color(palette.text),
            border_width: width,
            border_color: ring,
        },
    }
}
