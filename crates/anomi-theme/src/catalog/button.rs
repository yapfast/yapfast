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

/// The main action of a dialog or a form: the accent's fill. This and the
/// four after it are `ActionButton`'s looks (`ui/controls.slint:8`).
pub fn primary(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let fill = match status {
        Status::Hovered | Status::Pressed => palette.accent_hover,
        Status::Active | Status::Disabled => palette.accent,
    };
    action(fill, palette.on_accent, status)
}

/// A neutral action beside the main one.
pub fn secondary(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let fill = match status {
        Status::Hovered | Status::Pressed => palette.button_default_hover,
        Status::Active | Status::Disabled => palette.button_default,
    };
    action(fill, palette.text, status)
}

/// A destructive action.
pub fn danger(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let fill = match status {
        Status::Hovered | Status::Pressed => palette.danger_hover,
        Status::Active | Status::Disabled => palette.danger,
    };
    action(fill, palette.on_danger, status)
}

/// No fill until the pointer lifts it (`ui/controls.slint:32`); its ink is
/// `secondary` until then (`:47-48`).
pub fn ghost(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let fill = match status {
        Status::Hovered | Status::Pressed => palette.lift(0.08),
        Status::Active | Status::Disabled => Color::TRANSPARENT,
    };
    let ink = match status {
        Status::Hovered | Status::Pressed => palette.text,
        Status::Active | Status::Disabled => palette.secondary,
    };
    action(fill, ink, status)
}

/// A positive action: the live colour (`ui/controls.slint:30`), a tenth
/// brighter under the pointer.
pub fn success(theme: &Theme, status: Status) -> Style {
    let palette = theme.palette();
    let fill = match status {
        Status::Hovered | Status::Pressed => {
            let up = |channel: f32| (channel * 1.1).min(1.0);
            Color {
                r: up(palette.live.r),
                g: up(palette.live.g),
                b: up(palette.live.b),
                ..palette.live
            }
        }
        Status::Active | Status::Disabled => palette.live,
    };
    action(fill, palette.on_accent, status)
}

/// The shape every action shares; a disabled one fades whole
/// (`opacity: 0.5`, `ui/controls.slint:35`).
fn action(fill: Color, text: Color, status: Status) -> Style {
    let fade = if status == Status::Disabled { 0.5 } else { 1.0 };
    Style {
        background: Some(Background::Color(fill.scale_alpha(fade))),
        text_color: text.scale_alpha(fade),
        border: Border::default().rounded(crate::metrics::RADIUS_ROW),
        ..Style::default()
    }
}
