//! Buttons. The default is a plain one: no fill until hovered, as most of
//! the interface's buttons are icons and rows.

use iced::widget::button::{Status, Style};
use iced::{Background, Border, Color};

use super::catalog_with_status;
use crate::Theme;

catalog_with_status!(button, plain);

/// No fill at rest, the hover fill under the pointer.
pub fn plain(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let background = match status {
        Status::Hovered | Status::Pressed => Some(Background::Color(palette.hover)),
        Status::Active | Status::Disabled => None,
    };
    Style {
        background,
        text_color: palette.text,
        border: Border::default().rounded(crate::metrics::RADIUS_ROW),
        ..Style::default()
    }
}

/// Nothing at all: for a button whose content draws its own states.
pub fn bare(theme: &Theme, _status: Status) -> Style {
    Style {
        background: None,
        text_color: theme.palette().text,
        ..Style::default()
    }
}

/// The main action of a dialog or a form: the accent's fill.
pub fn primary(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let fill = match status {
        Status::Hovered | Status::Pressed => palette.accent_hover,
        Status::Active => palette.accent,
        Status::Disabled => palette.accent.scale_alpha(0.5),
    };
    filled(fill, palette.on_accent)
}

/// A neutral action beside the main one.
pub fn secondary(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let fill = match status {
        Status::Hovered | Status::Pressed => palette.button_default_hover,
        Status::Active => palette.button_default,
        Status::Disabled => palette.button_default.scale_alpha(0.5),
    };
    filled(fill, palette.text)
}

/// A destructive action.
pub fn danger(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let fill = match status {
        Status::Hovered | Status::Pressed => palette.danger_hover,
        Status::Active => palette.danger,
        Status::Disabled => palette.danger.scale_alpha(0.5),
    };
    filled(fill, palette.on_danger)
}

fn filled(fill: Color, text: Color) -> Style {
    Style {
        background: Some(Background::Color(fill)),
        text_color: text,
        border: Border::default().rounded(crate::metrics::RADIUS_FIELD),
        ..Style::default()
    }
}
