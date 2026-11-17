//! The switch, as `SettingsToggle` (`ui/yapfast/settings-toggle.slint:8`)
//! draws it: a 48 by 24 track whose 18 px knob sits 3 px in. Build it with
//! `.size(metrics::TOGGLE_HEIGHT)`; the track is then twice as wide.
//!
//! Slint slides the knob and fades the track in `Theme.quick`; the stock
//! toggler jumps, so a settings row that wants the slide wraps it in S01's
//! switch. The text is muted when disabled, as `SettingsSwitch` has it
//! (`ui/settings.slint:62`).

use iced::widget::toggler::{Catalog, Status, Style, StyleFn};
use iced::{Background, Color};

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

pub fn default(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let disabled = matches!(status, Status::Disabled { .. });
    let (on, lit, fade) = match status {
        Status::Active { is_toggled } => (is_toggled, false, 1.0),
        Status::Hovered { is_toggled } => (is_toggled, true, 1.0),
        Status::Disabled { is_toggled } => (is_toggled, false, 0.5),
    };
    let track = match (on, lit) {
        (true, true) => palette.accent_hover,
        (true, false) => palette.accent,
        (false, true) => palette.button_default_hover,
        (false, false) => palette.button_default,
    };
    let knob = if on {
        palette.on_accent
    } else {
        palette.secondary
    };
    Style {
        background: Background::Color(track.scale_alpha(fade)),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: Background::Color(knob.scale_alpha(fade)),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: Some(if disabled {
            palette.muted
        } else {
            palette.text
        }),
        border_radius: None,
        padding_ratio: metrics::TOGGLE_KNOB_INSET / metrics::TOGGLE_HEIGHT,
    }
}
