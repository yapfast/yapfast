//! A font file in Appearance's list: a sample of its first face, or a warning
//! sign when it gave none, its name, its faces or why it gave none, and the
//! button that takes it out. The Slint side: `FontFileRow`
//! (`ui/settings.slint:1318`).

use iced::font::{Style, Weight};
use iced::widget::{column, container, row, svg, text};
use iced::{Border, Font, Length, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::appearance::{FontFile, Intent};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

/// The sample's box, and the glyph inside it.
const SAMPLE: f32 = 40.0;
const SIGN: f32 = 18.0;

fn weight(of: u16) -> Weight {
    match of {
        0..=149 => Weight::Thin,
        150..=249 => Weight::ExtraLight,
        250..=349 => Weight::Light,
        350..=449 => Weight::Normal,
        450..=549 => Weight::Medium,
        550..=649 => Weight::Semibold,
        650..=749 => Weight::Bold,
        750..=849 => Weight::ExtraBold,
        _ => Weight::Black,
    }
}

/// One file of the list; `index` is what its remove button names.
pub fn font_row(file: &FontFile, index: usize, motion: Motion) -> Element<'_, Intent> {
    let inside: Element<'_, Intent> = if file.problem {
        icon(Icon::TriangleAlert, SIGN)
            .style(|theme: &Theme, _| svg::Style {
                color: Some(theme.palette().danger_text()),
            })
            .boxed()
    } else {
        text("Aa")
            .size(18)
            .font(Font {
                weight: weight(file.weight),
                style: if file.italic {
                    Style::Italic
                } else {
                    Style::Normal
                },
                ..typography::UI
            })
            .style(catalog::text::primary)
            .boxed()
    };
    let sample = container(inside)
        .width(SAMPLE)
        .height(SAMPLE)
        .center_x(SAMPLE)
        .center_y(SAMPLE)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            container::Style::default()
                .background(palette.sunken)
                .border(Border {
                    width: 1.0,
                    color: palette.line,
                    radius: 8.into(),
                })
        });
    let detail = text(file.detail.as_str())
        .size(13)
        .width(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.palette();
            iced::widget::text::Style {
                color: Some(if file.problem {
                    palette.danger_text()
                } else {
                    palette.muted
                }),
                ..Default::default()
            }
        });
    let mut words = column![
        text(file.name.as_str())
            .size(typography::size::BODY)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .style(catalog::text::primary)
            .boxed(),
        detail.boxed(),
    ]
    .spacing(2)
    .width(Length::Fill);
    if !file.warning.is_empty() {
        // The sign in the warning colour, the words in one every theme reads
        // at this size.
        let sign = icon(Icon::TriangleAlert, 14.0).style(|theme: &Theme, _| svg::Style {
            color: Some(theme.palette().loud),
        });
        words = words.push(
            row![
                container(sign).padding(Padding {
                    top: 2.0,
                    ..Padding::ZERO
                }),
                text(file.warning.as_str())
                    .size(13)
                    .width(Length::Fill)
                    .style(catalog::text::secondary),
            ]
            .spacing(6)
            .boxed(),
        );
    }
    let remove = icon_button(Icon::X, tr!("Remove font file"), motion)
        .size(32.0)
        .glyph(16.0)
        .on_press(Intent::RemoveFont(index))
        .view();
    row![sample, words, remove]
        .spacing(12)
        .align_y(alignment::Vertical::Center)
        .padding(Padding {
            left: 12.0,
            right: 8.0,
            top: 10.0,
            bottom: 10.0,
        })
        .boxed()
}
