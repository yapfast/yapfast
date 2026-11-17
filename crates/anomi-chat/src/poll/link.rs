//! "Add another answer" as a `LinkButton` with its icon (`ui/settings.slint:259`):
//! its words and a picture on the content edge, no padding to set them in.
//! anomi-widgets has no link button (the port is `anomi_settings::kit::link`,
//! which this crate can't depend on). No keyboard ring, as iced's button has
//! none.

use iced::widget::{button, row, text};
use iced::{Alignment, Background, Border};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::icon::Icon;
use anomi_widgets::icon::icon;

/// A small link's line, its words and its icon (`small: true`).
const LINE: f32 = 28.0;
const GLYPH: f32 = 16.0;
const GAP: f32 = 6.0;

pub(super) fn link<'a, M: Clone + 'a>(
    words: &'a str,
    picture: Icon,
    on_press: M,
) -> Element<'a, M> {
    button(
        row![
            icon(picture, GLYPH).style(|theme: &Theme, _| iced::widget::svg::Style {
                color: Some(theme.palette().secondary),
            }),
            text(words)
                .size(typography::size::SMALL)
                .font(typography::BOLD)
                .wrapping(text::Wrapping::None),
        ]
        .spacing(GAP)
        .align_y(Alignment::Center),
    )
    .height(LINE)
    .padding(0)
    .on_press(on_press)
    .style(|theme: &Theme, status| {
        let palette = theme.palette();
        button::Style {
            background: None::<Background>,
            text_color: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                palette.text
            } else {
                palette.secondary
            },
            border: Border::default(),
            ..button::Style::default()
        }
    })
    .boxed()
}
