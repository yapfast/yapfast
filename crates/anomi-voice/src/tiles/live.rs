//! Someone sharing their screen, marked as Discord's red LIVE tag does
//! (`LivePill`, `ui/voice.slint:41`): 16 px tall and round, its text 10 px
//! and extra bold in the danger colour's ink, 5 px from its ends. A tile's
//! chip and picture in picture show it still; where it opens the stream it
//! lights under the pointer.

use iced::font::Weight;
use iced::widget::{button, container, text};
use iced::{Background, Border, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use yapfast_lang::tr;

const HEIGHT: f32 = 16.0;
const SIDE: f32 = 5.0;
const SIZE: f32 = 10.0;
const FONT: iced::Font = typography::weight(Weight::ExtraBold);

/// The tag; a click sends `watch` when there is one, and it lights under
/// the pointer then.
pub fn live<'a, M: Clone + 'a>(watch: Option<M>) -> Element<'a, M> {
    let face = container(
        text(tr!("LIVE"))
            .size(SIZE)
            .font(FONT)
            .line_height(1.0)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.palette().on_danger),
                ..text::Style::default()
            }),
    )
    .padding(Padding::ZERO.left(SIDE).right(SIDE))
    .center_y(HEIGHT);
    match watch {
        Some(watch) => button(face)
            .padding(0)
            .on_press(watch)
            .style(|theme: &Theme, status| {
                let palette = theme.palette();
                let fill = match status {
                    button::Status::Hovered | button::Status::Pressed => palette.danger_hover,
                    _ => palette.danger,
                };
                button::Style {
                    background: Some(Background::Color(fill)),
                    text_color: palette.on_danger,
                    border: Border::default().rounded(HEIGHT / 2.0),
                    ..button::Style::default()
                }
            })
            .boxed(),
        None => face
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().danger)),
                border: Border::default().rounded(HEIGHT / 2.0),
                ..container::Style::default()
            })
            .boxed(),
    }
}
