//! The quick pick of noise suppression (`Choices` in the input menu,
//! `ui/yapfast.slint:2092`, `ui/settings.slint:132`): the options side by
//! side in a well, the chosen one raised and bold. Each is as wide as its
//! text with room either side; the well is as wide as the menu leaves it.

use iced::widget::{button, container, row, text};
use iced::{Background, Border, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::voice::devices::{Intent, Noise};

use yapfast_lang::tr;

/// The well's padding and the gap between options (`padding: 4px; spacing: 4px`).
const WELL: f32 = 4.0;
/// An option's height and its sides: Slint's 10 px plus a share of what the
/// well has spare, as the English and Russian texts fill the menu.
const HEIGHT: f32 = 28.0;
const SIDE: f32 = 14.0;
const RADIUS: f32 = 4.0;

/// The name an option wears.
pub fn name(noise: Noise) -> &'static str {
    match noise {
        Noise::DeepFilterNet => "DeepFilterNet",
        Noise::RNNoise => "RNNoise",
        Noise::Off => tr!("Off"),
    }
}

/// The options with `chosen` raised.
pub fn choices<'a>(chosen: Noise) -> impl View<Intent> + 'a {
    let options = Noise::ALL.map(|noise| -> Element<'a, Intent> {
        let current = noise == chosen;
        let label = text(name(noise))
            .size(typography::size::SMALL)
            .font(if current {
                typography::BOLD
            } else {
                typography::MEDIUM
            })
            .style(if current {
                catalog::text::primary
            } else {
                catalog::text::secondary
            });
        button(container(label).center_y(Length::Fill))
            .height(HEIGHT)
            .padding(Padding::new(0.0).left(SIDE).right(SIDE))
            .on_press(Intent::Noise(noise))
            .style(move |theme: &Theme, status| {
                let palette = theme.palette();
                let fill = match (current, status) {
                    (true, _) => Some(palette.active),
                    (_, button::Status::Hovered | button::Status::Pressed) => Some(palette.hover),
                    _ => None,
                };
                button::Style {
                    background: fill.map(Background::Color),
                    text_color: palette.text,
                    border: Border::default().rounded(RADIUS),
                    ..button::Style::default()
                }
            })
            .boxed()
    });
    container(row(options).spacing(WELL))
        .padding(WELL)
        .width(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(palette.sunken)),
                border: Border::default()
                    .rounded(metrics::RADIUS_CARD)
                    .width(1)
                    .color(palette.line),
                ..container::Style::default()
            }
        })
}
