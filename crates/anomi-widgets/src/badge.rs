//! A count of mentions or requests: white on red, round.

use iced::widget::{container, text};
use iced::{Background, Border};

use anomi_theme::{Theme, View, typography};

/// A badge's height; it grows sideways with its text.
pub const HEIGHT: f32 = 16.0;

/// The count, as `kind::count_badge` writes it.
pub fn badge<'a, Message: 'a>(count: &'a str) -> impl View<Message> + 'a {
    container(
        text(count)
            .size(typography::size::BADGE)
            .font(typography::BOLD)
            .line_height(1.0),
    )
    .padding([0.0, 5.0])
    .height(HEIGHT)
    .center_y(HEIGHT)
    .style(|theme: &Theme| {
        let palette = theme.palette();
        container::Style {
            text_color: Some(palette.on_danger),
            background: Some(Background::Color(palette.danger)),
            border: Border::default().rounded(HEIGHT / 2.0),
            ..container::Style::default()
        }
    })
}
