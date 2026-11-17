//! Pane grids: the region a pane would drop into takes the soft accent with
//! a 2 px accent border, and a split under the pointer or being dragged is
//! drawn in the accent.

use iced::widget::pane_grid::{Catalog, Highlight, Line, Style, StyleFn};
use iced::{Background, Border};

use crate::Theme;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> <Self as Catalog>::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &<Self as Catalog>::Class<'_>) -> Style {
        class(self)
    }
}

/// Width of the split's accent line and of the region's border.
const EDGE: f32 = 2.0;

pub fn default(theme: &Theme) -> Style {
    let palette = theme.palette();
    let split = Line {
        color: palette.accent,
        width: EDGE,
    };
    Style {
        hovered_region: Highlight {
            background: Background::Color(palette.accent_soft),
            border: Border {
                color: palette.accent,
                width: EDGE,
                radius: crate::metrics::RADIUS_ROW.into(),
            },
        },
        picked_split: split,
        hovered_split: split,
    }
}
