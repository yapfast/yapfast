//! A theme to choose and the grid of them: a small window drawn in the
//! theme's own colours, with its name below. The current one has an accent
//! ring and a check; the person's own themes have a pencil that edits them.
//! The Slint side: `ThemeTile` and the `Rectangle` of cards in
//! `AppearanceSettings` (`ui/settings.slint:1209`, `:1371`).
//!
//! The grid draws each tile's ground and rim (`grid`), which tint under the
//! pointer; the tile is the rest.

mod grid;

pub use grid::grid;

use iced::border::Radius;
use iced::widget::{column, container, row, space, stack, svg, text};
use iced::{Background, Border, Color, Length, Padding, alignment};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::icon::Icon;
use anomi_vm::settings::appearance::{Intent, ThemeCard};
use anomi_widgets::button::icon_button;
use anomi_widgets::icon::icon;
use yapfast_lang::tr;

use crate::kit::pressable;

/// A tile is this high; the grid fits as many 150 px or wider across as it
/// can, 12 px apart.
const HEIGHT: f32 = 124.0;
const MIN_WIDTH: f32 = 150.0;
const GAP: f32 = 12.0;
/// The preview's inset and height, and the name's line below it.
const INSET: f32 = 8.0;
const PREVIEW: f32 = 78.0;
const NAME: f32 = 24.0;
const RADIUS: f32 = 10.0;
/// The pencil's side, and the room the name leaves for it.
const PENCIL: f32 = 24.0;

fn quad<'a>(width: f32, height: f32, radius: f32, colour: Color) -> Element<'a, Intent> {
    container(space().width(width).height(height))
        .style(move |_| container::Style {
            background: Some(Background::Color(colour)),
            border: Border {
                radius: radius.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .boxed()
}

/// `child` with its corner `x`, `y` in from the preview's, and `right` short
/// of its end.
fn at<'a>(x: f32, y: f32, right: f32, child: Element<'a, Intent>) -> Element<'a, Intent> {
    container(child)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            left: x,
            top: y,
            right,
            bottom: 0.0,
        })
        .boxed()
}

/// The window in miniature: the rail with three servers, the sidebar's
/// channels, two messages and the composer.
fn preview(card: &ThemeCard) -> Element<'_, Intent> {
    let mut layers: Vec<Element<'_, Intent>> = Vec::new();
    layers.push(
        container(space())
            .width(16)
            .height(Length::Fill)
            // The preview's clip is square: the rail rounds its own corners.
            .style(move |_| {
                container::Style::default()
                    .background(card.rail)
                    .border(Border::default().rounded(Radius {
                        top_left: 6.0,
                        bottom_left: 6.0,
                        ..Radius::default()
                    }))
            })
            .boxed(),
    );
    for index in 0..3u8 {
        let (radius, colour) = if index == 0 {
            (3.0, card.accent)
        } else {
            (5.0, card.raised)
        };
        let y = 6.0 + f32::from(index) * 13.0;
        layers.push(at(3.0, y, 0.0, quad(10.0, 10.0, radius, colour)));
    }
    layers.push(at(
        16.0,
        0.0,
        0.0,
        container(space())
            .width(38)
            .height(Length::Fill)
            .style(move |_| container::Style::default().background(card.sidebar))
            .boxed(),
    ));
    for index in 0..4u8 {
        let (width, colour) = if index == 1 {
            (24.0, card.text)
        } else {
            (18.0 + f32::from(index % 2) * 4.0, card.muted)
        };
        let y = 8.0 + f32::from(index) * 11.0;
        layers.push(at(22.0, y, 0.0, quad(width, 4.0, 2.0, colour)));
    }
    for index in 0..2u8 {
        let first = index == 0;
        let who = if first { card.accent } else { card.muted };
        let body = stack![
            at(0.0, 0.0, 0.0, quad(10.0, 10.0, 5.0, who)),
            at(
                14.0,
                1.0,
                0.0,
                quad(if first { 22.0 } else { 16.0 }, 4.0, 2.0, card.text)
            ),
            at(
                14.0,
                8.0,
                0.0,
                quad(if first { 52.0 } else { 40.0 }, 3.0, 1.5, card.muted)
            ),
        ]
        .width(Length::Fill)
        .height(16);
        layers.push(at(60.0, 8.0 + f32::from(index) * 22.0, 6.0, body.boxed()));
    }
    let dot = container(quad(6.0, 6.0, 3.0, card.accent))
        .width(Length::Fill)
        .align_x(alignment::Horizontal::Right)
        .padding(Padding {
            right: 4.0,
            ..Padding::ZERO
        });
    let composer = container(dot)
        .width(Length::Fill)
        .height(12)
        .align_y(alignment::Vertical::Center)
        .style(move |_| {
            container::Style::default()
                .background(card.raised)
                .border(Border::default().rounded(4))
        });
    layers.push(
        container(composer)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                left: 60.0,
                right: 6.0,
                bottom: 6.0,
                top: 0.0,
            })
            .align_y(alignment::Vertical::Bottom)
            .boxed(),
    );
    container(stack(layers).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(PREVIEW)
        .clip(true)
        .style(move |_| {
            container::Style::default()
                .background(card.chat)
                .border(Border::default().rounded(6))
        })
        .boxed()
}

/// A theme's tile: a click or Space chooses it; a pencil edits one of the
/// person's own.
pub fn tile(card: &ThemeCard, current: bool, motion: Motion) -> Element<'_, Intent> {
    let name = text(card.name.as_str())
        .size(typography::size::SMALL)
        .font(if current {
            typography::BOLD
        } else {
            typography::MEDIUM
        })
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .width(Length::Fill)
        .style(catalog::text::inherit);
    let check = icon(Icon::Check, 16.0).style(|theme: &Theme, _| svg::Style {
        color: Some(theme.palette().accent),
    });
    let mut line = row![name.boxed()]
        .spacing(6)
        .align_y(alignment::Vertical::Center);
    if current {
        line = line.push(check.boxed());
    }
    let face = container(column![
        container(preview(card)).padding(Padding {
            bottom: 6.0,
            ..Padding::new(INSET)
        }),
        container(line)
            .height(NAME)
            .align_y(alignment::Vertical::Center)
            .padding(Padding {
                left: 10.0,
                right: 10.0 + if card.custom { PENCIL + 6.0 } else { 0.0 },
                top: 0.0,
                bottom: 0.0,
            }),
    ])
    .width(Length::Fill)
    .height(HEIGHT);
    let button = pressable(
        face.boxed(),
        &card.name,
        Intent::ChooseTheme(card.id.clone()),
        RADIUS,
        move |theme: &Theme, _| {
            let palette = theme.palette();
            if current {
                palette.text
            } else {
                palette.secondary
            }
        },
        motion,
    );
    if !card.custom {
        return button;
    }
    let pencil = container(
        icon_button(Icon::Pencil, tr!("Edit theme"), motion)
            .size(PENCIL)
            .glyph(14.0)
            .on_press(Intent::EditTheme(card.id.clone()))
            .view(),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(alignment::Horizontal::Right)
    .align_y(alignment::Vertical::Bottom)
    .padding(Padding {
        right: INSET,
        bottom: INSET,
        top: 0.0,
        left: 0.0,
    });
    stack![button, pencil].boxed()
}
