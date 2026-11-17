//! QR codes: dark cells on white in every theme, so a phone's camera reads
//! them on a dark window as on a light one.

use iced::Color;
use iced::widget::qr_code::{Catalog, Style, StyleFn};

use crate::Theme;

impl Catalog for Theme {
    type Class<'a> = StyleFn<'a, Self>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default)
    }

    fn style(&self, class: &Self::Class<'_>) -> Style {
        class(self)
    }
}

pub fn default(_theme: &Theme) -> Style {
    Style {
        cell: Color::BLACK,
        background: Color::WHITE,
    }
}
