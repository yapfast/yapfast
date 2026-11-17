//! The card both dialogs sit on: raised, edged, 12 px corners and the
//! shadow under it (`background: Theme.raised; border-radius: 12px;
//! drop-shadow-blur: 32px; drop-shadow-offset-y: 12px; #00000066`,
//! `ui/voice.slint:1222`, `:1288`).

use iced::widget::container;
use iced::{Border, Color, Shadow, Vector};

use anomi_theme::{Theme, metrics};

pub fn card(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(palette.raised.into()),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0x66 as f32 / 255.0),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..container::Style::default()
    }
}
