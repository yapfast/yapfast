//! A colour of the theme: a swatch, its name with "Not a colour" under it
//! when its text is none, and its hex in a field that keeps a wrong draft.
//! The Slint side: `ThemeColourRow` (`ui/settings.slint:1285`).

use iced::widget::{Column, column, container, row, text};
use iced::{Border, Color, Length, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::settings::theme_editor::{Colour, Intent};
use anomi_widgets::field::field;
use yapfast_lang::tr;

const SWATCH: f32 = 28.0;
const FIELD: f32 = 116.0;
const SPACING: f32 = 10.0;

/// The row of colour `index`.
pub fn colour_row(colour: &Colour, index: usize, motion: Motion) -> Element<'_, Intent> {
    let shown = colour.colour;
    let swatch = container(iced::widget::space())
        .width(SWATCH)
        .height(SWATCH)
        .style(move |theme: &Theme| container::Style {
            background: Some(shown.into()),
            border: Border::default()
                .rounded(metrics::RADIUS_ROW)
                .width(1)
                .color(theme.palette().line),
            ..Default::default()
        });
    let mut names: Column<Element<'_, Intent>> = column![
        text(&colour.name)
            .size(typography::size::SMALL)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
    ];
    if colour.wrong {
        names = names.push(
            text(tr!("Not a colour"))
                .size(typography::size::CAPTION)
                .wrapping(text::Wrapping::None)
                .ellipsis(text::Ellipsis::End)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().danger),
                    ..Default::default()
                })
                .boxed(),
        );
    }
    let wrong = colour.wrong;
    let mut input = field("", &colour.text)
        .on_input(move |text| Intent::Colour(index, text))
        .text_size(typography::size::SMALL)
        .pad(8.0)
        .height(34.0)
        .motion(motion);
    if wrong {
        // The danger tint is the container's, which has the theme.
        input = input.fill(Color::TRANSPARENT);
    }
    let well = container(input.view())
        .width(FIELD)
        .style(move |theme: &Theme| container::Style {
            background: wrong.then(|| theme.palette().danger.scale_alpha(0.16).into()),
            border: Border::default().rounded(metrics::RADIUS_ROW),
            ..Default::default()
        });
    row![swatch, container(names).width(Length::Fill), well,]
        .spacing(SPACING)
        .align_y(alignment::Vertical::Center)
        .height(Length::Fill)
        .boxed()
}
