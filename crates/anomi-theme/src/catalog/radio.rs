//! Radio buttons, as `Mark` (`ui/interactions.slint:116`) draws them with
//! `round`: a ring, the accent when chosen, with a dot half its size. Build it
//! with `.size(metrics::MARK_SIZE)`; iced's default is 16 px.

use iced::widget::radio::{Catalog, Status, Style, StyleFn};
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
    let (Status::Active { is_selected } | Status::Hovered { is_selected }) = status;
    Style {
        background: Background::Color(Color::TRANSPARENT),
        dot_color: palette.accent,
        border_width: metrics::MARK_RING,
        border_color: if is_selected {
            palette.accent
        } else {
            palette.secondary
        },
        text_color: Some(palette.text),
    }
}
