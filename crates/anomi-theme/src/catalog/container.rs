//! Containers: the grounds of the window's parts. The default draws nothing.

use iced::widget::container::Style;
use iced::{Background, Border, Color};

use super::catalog;
use crate::Theme;

catalog!(container, transparent);

pub fn transparent(_theme: &Theme) -> Style {
    Style::default()
}

fn ground(color: Color) -> Style {
    Style {
        background: Some(Background::Color(color)),
        ..Style::default()
    }
}

/// The server rail.
pub fn rail(theme: &Theme) -> Style {
    ground(theme.palette().rail)
}

/// The channel and DM list.
pub fn sidebar(theme: &Theme) -> Style {
    ground(theme.palette().sidebar)
}

/// The conversation.
pub fn chat(theme: &Theme) -> Style {
    ground(theme.palette().chat)
}

/// The account card and the voice dock.
pub fn panel(theme: &Theme) -> Style {
    Style {
        border: Border::default().rounded(crate::metrics::RADIUS_CARD),
        ..ground(theme.palette().panel)
    }
}

/// A card, a popover, a dialog: what's raised, with its edge.
pub fn raised(theme: &Theme) -> Style {
    let palette = theme.palette();
    Style {
        border: Border::default()
            .rounded(crate::metrics::RADIUS_CARD)
            .width(1)
            .color(palette.edge),
        ..ground(palette.raised)
    }
}

/// The window's tooltip.
pub fn tooltip(theme: &Theme) -> Style {
    let palette = theme.palette();
    Style {
        text_color: Some(palette.text),
        border: Border::default().rounded(crate::metrics::RADIUS_FIELD),
        ..ground(palette.tooltip)
    }
}
