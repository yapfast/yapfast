//! Progress and meters. iced's bar is 30 px thick unless built with a girth:
//! `.girth(metrics::PROGRESS_HEIGHT)` for `primary`, `METER_HEIGHT` for the
//! level styles. `primary` is the update's install bar
//! (`ui/settings.slint:2023`): the accent in the sidebar's trough.
//! `level`, `loud` and `clipping` are `LevelBar`'s (`ui/controls.slint:1798`): an
//! 8 px trough in a control's face. LevelBar's fill ends square, clipped by
//! the trough, which iced's bar cannot do because the fill shares the
//! trough's radius, so F12 draws LevelBar's segments itself and does not stack
//! these bars.

use iced::widget::progress_bar::{Catalog, Style, StyleFn};
use iced::{Background, Border, Color};

use crate::{Theme, metrics};

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(primary)
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }
}

/// The install bar's corner radius, `ui/settings.slint:2025`.
const PROGRESS_RADIUS: f32 = 3.0;

fn bar(theme: &Theme, bar: Color) -> Style {
    Style {
        background: Background::Color(theme.palette().button_default),
        bar: Background::Color(bar),
        border: Border::default().rounded(metrics::METER_HEIGHT / 2.0),
    }
}

/// Progress: the accent in the sidebar's trough, radius 3.
pub fn primary(theme: &Theme) -> Style {
    let palette = theme.palette();
    Style {
        background: Background::Color(palette.sidebar),
        bar: Background::Color(palette.accent),
        border: Border::default().rounded(PROGRESS_RADIUS),
    }
}

/// A level that is fine: the live colour.
pub fn level(theme: &Theme) -> Style {
    bar(theme, theme.palette().live)
}

/// A level that is loud.
pub fn loud(theme: &Theme) -> Style {
    bar(theme, theme.palette().loud)
}

/// A level that clips.
pub fn clipping(theme: &Theme) -> Style {
    bar(theme, theme.palette().danger)
}
