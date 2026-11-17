//! The quick pick of noise suppression (`Choices` in the input menu,
//! `ui/yapfast.slint:2092`, `ui/settings.slint:132`): the options side by
//! side in a well, the chosen one raised and bold. Each is as wide as its
//! text with room either side; the well is as wide as the menu leaves it.

use iced::widget::{container, row};
use iced::{Background, Border, Length};

use anomi_theme::metrics;
use anomi_theme::prelude::*;
use anomi_vm::voice::devices::{Intent, Noise};

use yapfast_lang::tr;

use super::measure;
use super::option;

/// The well's padding and the gap between options (`padding: 4px; spacing: 4px`).
const WELL: f32 = 4.0;
/// The room either side of an option's bold words.
const SIDE: f32 = 10.0;

/// The name an option wears.
pub fn name(noise: Noise) -> &'static str {
    match noise {
        Noise::DeepFilterNet => "DeepFilterNet",
        Noise::RNNoise => "RNNoise",
        Noise::Off => tr!("Off"),
    }
}

/// The options with `chosen` raised.
pub fn choices<'a>(chosen: Noise, card: f32, motion: Motion) -> impl View<Intent> + 'a {
    // The well spans the card less its padding and the part's inset; each
    // option is at least its bold words and sides, the spare shared evenly.
    let least = measure::bold().map(|words| (words + 2.0 * SIDE).ceil());
    let room = card - 2.0 * metrics::ROW_INSET - 2.0 * super::PADDING - 2.0 * WELL;
    let spare = room - least.iter().sum::<f32>() - WELL * 2.0;
    let share = (spare / least.len() as f32).max(0.0).floor();
    let rest = (spare - share * (least.len() - 1) as f32).max(0.0);
    let last = least.len() - 1;
    let options = Noise::ALL.into_iter().zip(least).enumerate().map(
        |(at, (noise, least))| -> Element<'a, Intent> {
            let width = least + if at == last { rest } else { share };
            let current = noise == chosen;
            option::pick(name(noise), width, current, Intent::Noise(noise), motion).boxed()
        },
    );
    container(row(options).spacing(WELL))
        .padding(WELL)
        .width(Length::Fill)
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
