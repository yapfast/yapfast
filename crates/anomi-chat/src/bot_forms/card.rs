//! The form's card and the select menu's panel: raised, edged, with their
//! shadows (`ui/interactions.slint:181`, `:51`).

use iced::widget::container;
use iced::{Border, Color, Shadow, Vector};

use anomi_theme::metrics;
use anomi_theme::prelude::*;

/// The card's shadow (`drop-shadow-blur: 28px; drop-shadow-color: #00000077`,
/// `interactions.slint:181`) and the panel's (`16px`/`#00000055`, `:51`).
const CARD_BLUR: f32 = 28.0;
const CARD_ALPHA: u8 = 0x77;
const PANEL_BLUR: f32 = 16.0;
const PANEL_ALPHA: u8 = 0x55;

/// The form's card: 12 px corners, `drop-shadow-blur: 28px`, `#00000077`.
pub fn dialog(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, CARD_ALPHA as f32 / 255.0),
            offset: Vector::ZERO,
            blur_radius: CARD_BLUR,
        },
        ..container::Style::default()
    }
}

/// The select's panel: 8 px corners, `drop-shadow-blur: 16px`, `#00000055`.
pub fn panel(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_CARD)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, PANEL_ALPHA as f32 / 255.0),
            offset: Vector::ZERO,
            blur_radius: PANEL_BLUR,
        },
        ..container::Style::default()
    }
}
