//! How a sidebar row looks in each of its states: the fill, and the ink of
//! its words. The Slint side: `ChannelRow`'s `background` and `ink`
//! (`ui/rail.slint:509`, `:517`).

use iced::{Background, Color};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, metrics};
use iced::widget::button;

/// What a row is doing: chosen, under the pointer, with news.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look {
    pub selected: bool,
    pub hovered: bool,
    pub unread: bool,
}

impl Look {
    /// The row's fill: the active one when chosen, the hover one under the
    /// pointer, none at rest.
    pub fn fill(self, palette: &Palette) -> Option<Color> {
        if self.selected {
            Some(palette.active)
        } else if self.hovered {
            Some(palette.hover)
        } else {
            None
        }
    }

    /// The colour of its name: the text colour when chosen or unread, the
    /// secondary one under the pointer, the muted one at rest.
    pub fn ink(self, palette: &Palette) -> Color {
        if self.selected || self.unread {
            palette.text
        } else if self.hovered {
            palette.secondary
        } else {
            palette.muted
        }
    }
}

/// A row's button style in `look`: the fill and the ink, with the row's
/// corners.
pub fn style(theme: &Theme, look: Look) -> button::Style {
    let palette = theme.palette();
    button::Style {
        background: look.fill(palette).map(Background::Color),
        text_color: look.ink(palette),
        border: iced::Border::default().rounded(metrics::RADIUS_ROW),
        ..button::Style::default()
    }
}
