//! "Clear after": a trough of options, one chosen, filling the card's width
//! (`Choices` with `fill`, `ui/settings.slint:132`). The settings pages' own
//! is S01's, in `anomi-settings`, and not there yet; the invite dialog keeps
//! another for the same reason.

use iced::widget::{button, container, row, text};
use iced::{Background, Border, Length};

use anomi_theme::prelude::*;
use anomi_theme::{metrics, typography};

/// The options' height and the trough's padding and gap.
const OPTION: f32 = 28.0;
const INSET: f32 = 4.0;
/// An option's side padding (`ui/settings.slint:132`).
const OPTION_PAD: f32 = 10.0;

/// A trough of `labels` sharing its row, `current` lit; a press on one sends
/// `pick(index)`.
pub(super) fn choices<'a, M: Clone + 'a>(
    labels: &[&'a str],
    current: usize,
    pick: impl Fn(usize) -> M,
) -> Element<'a, M> {
    let mut line = row![].spacing(INSET);
    for (index, label) in labels.iter().enumerate() {
        let chosen = index == current;
        let name = text(*label)
            .size(typography::size::SMALL)
            .font(if chosen {
                typography::BOLD
            } else {
                typography::MEDIUM
            })
            .wrapping(text::Wrapping::None)
            .center();
        let option = button(container(name).center(Length::Fill))
            // Slint's options each take their text's width and an even
            // share of what is spare; a share by length comes close.
            .width(Length::FillPortion(label.chars().count() as u16 + 3))
            .height(OPTION)
            .padding([0.0, OPTION_PAD])
            .on_press(pick(index))
            .style(move |theme: &Theme, status| {
                let palette = theme.palette();
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                let fill = if chosen {
                    Some(palette.active)
                } else if hovered {
                    Some(palette.hover)
                } else {
                    None
                };
                button::Style {
                    background: fill.map(Background::Color),
                    text_color: if chosen {
                        palette.text
                    } else {
                        palette.secondary
                    },
                    border: Border::default().rounded(metrics::RADIUS_FIELD),
                    ..button::Style::default()
                }
            });
        line = line.push(option);
    }
    container(line)
        .padding(INSET)
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
        .boxed()
}
