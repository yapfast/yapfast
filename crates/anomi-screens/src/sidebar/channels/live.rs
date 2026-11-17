//! Someone sharing their screen, marked as Discord's red LIVE tag does:
//! `LivePill` (`ui/voice.slint:41`), clickable where it opens the stream.
//! The tiles' own (V03) draw it over a call; the list and its card draw
//! this one, the same look.

use iced::font::Weight;
use iced::widget::{button, container, text};
use iced::{Background, Border};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::sidebar::channels::Intent;
use yapfast_lang::tr;

/// 16 px tall, its text 5 px from its ends, round (`:45`), the text 10 px
/// and extra bold (`:50`).
const HEIGHT: f32 = 16.0;
const SIDE: f32 = 5.0;
const SIZE: f32 = 10.0;
const FONT: iced::Font = typography::weight(Weight::ExtraBold);
/// `letter-spacing: 0.4px` (`:50`), which iced lacks: it goes into the
/// padding, half each side.
const TRACKING: f32 = 0.4;

/// The tag; a click sends what `watch` makes when there is one, made only
/// then, and it lights under the pointer then.
pub fn live<'a>(watch: Option<impl Fn() -> Intent + 'a>) -> Element<'a, Intent> {
    let label = tr!("LIVE");
    let side = SIDE + TRACKING * label.chars().count() as f32 / 2.0;
    let face = container(text(label).size(SIZE).font(FONT).line_height(1.0).style(
        |theme: &Theme| text::Style {
            color: Some(theme.palette().on_danger),
            ..text::Style::default()
        },
    ))
    .padding([0.0, side])
    .center_y(HEIGHT);
    match watch {
        Some(watch) => button(face)
            .padding(0)
            .on_press_with(watch)
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
