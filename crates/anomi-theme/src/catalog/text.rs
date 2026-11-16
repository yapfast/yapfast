//! Text. The default inherits the colour of what holds it; the named styles
//! are the palette's text colours.

use iced::Color;
use iced::widget::text::{Catalog, Style, StyleFn};

use crate::Theme;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(inherit)
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }

    /// Selected text: the accent, faint enough to read through.
    fn selection(&self) -> Color {
        self.palette().accent.scale_alpha(0.35)
    }
}

pub fn inherit(_theme: &Theme) -> Style {
    Style::default()
}

fn colored(color: Color) -> Style {
    Style {
        color: Some(color),
        selection: None,
    }
}

pub fn primary(theme: &Theme) -> Style {
    colored(theme.palette().text)
}

pub fn secondary(theme: &Theme) -> Style {
    colored(theme.palette().secondary)
}

pub fn muted(theme: &Theme) -> Style {
    colored(theme.palette().muted)
}

pub fn faint(theme: &Theme) -> Style {
    colored(theme.palette().faint)
}

pub fn danger(theme: &Theme) -> Style {
    colored(theme.palette().danger_text())
}

pub fn link(theme: &Theme) -> Style {
    colored(theme.palette().link)
}
