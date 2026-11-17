//! What hangs under the server's name: Discord's boost goal bar and its
//! "N Events" row (`ui/yapfast.slint:2866` and `:2902`). Each ends in a
//! rule of its own, inset as Slint's is.

use iced::widget::{Space, column, container, row, rule, stack, text};
use iced::{Alignment, Color, Fill, Gradient, Length, Padding, gradient};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::menu::Spot;
use anomi_vm::sidebar::server_header::{BOOST_GOAL, Intent, ServerHeader};
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use yapfast_lang::{tr, trn};

use super::lit::lit;

/// The boost bar's height, its pill's, and the pill's place in it.
const BOOST_HEIGHT: f32 = 39.0;
const PILL_HEIGHT: f32 = 26.0;
const PILL_X: f32 = 12.0;
const PILL_TOP: f32 = 4.0;
/// The fill's inset in the pill.
const FILL_INSET: f32 = 2.0;
/// The events row's height, and its fill's inset from the top and sides.
const EVENTS_HEIGHT: f32 = 60.0;
const EVENTS_INSET: f32 = 8.0;
/// The fill of the boost goal: Discord's pink, strong at the right.
const PINK: (f32, f32, f32) = (1.0, 0.45, 0.98);

/// The rows under the name, each with its own rule.
pub fn rows(header: &ServerHeader, motion: Motion) -> Vec<Element<'_, Intent>> {
    let mut rows: Vec<Element<'_, Intent>> = Vec::new();
    if let Some(boosts) = header.boosts {
        // The boost bar's rule runs from x 8 to the right edge.
        rows.push(column![boost(boosts, header.boost_share()), inset(8.0, 0.0)].boxed());
    }
    if let Some(events) = header.events {
        // The events row's rule is inset 16 on both sides.
        rows.push(
            column![
                events_row(events, motion),
                inset(metrics::SPACE_L, metrics::SPACE_L)
            ]
            .boxed(),
        );
    }
    rows
}

/// A rule inset from the sides.
fn inset<'a>(left: f32, right: f32) -> impl View<Intent> + 'a {
    container(rule::horizontal(1).style(catalog::rule::outline))
        .padding(Padding::new(0.0).left(left).right(right))
}

/// The goal bar: a pill whose fill reaches the share of the goal.
fn boost(boosts: u32, share: f32) -> impl View<Intent> {
    let pink = |alpha: f32| Color::from_rgba(PINK.0, PINK.1, PINK.2, alpha);
    let track = container(Space::new())
        .width(Fill)
        .height(PILL_HEIGHT)
        .style(|theme: &Theme| iced::widget::container::Style {
            background: Some(theme.palette().hover.into()),
            border: iced::Border::default().rounded(PILL_HEIGHT / 2.0),
            ..Default::default()
        });
    // The fill's width is a portion of what the track has left inside it.
    let fill = (share * 1000.0).round() as u16;
    let filled = row![
        container(Space::new())
            .width(Length::FillPortion(fill.max(1)))
            .height(Fill)
            .style(move |_: &Theme| iced::widget::container::Style {
                background: Some(
                    Gradient::Linear(
                        gradient::Linear::new(3.0 * std::f32::consts::FRAC_PI_2)
                            .add_stop(0.0, pink(0.4))
                            .add_stop(1.0, pink(0.1)),
                    )
                    .into(),
                ),
                border: iced::Border::default().rounded(PILL_HEIGHT / 2.0 - FILL_INSET),
                ..Default::default()
            }),
        Space::new().width(Length::FillPortion(1000 - fill.min(999)))
    ]
    .padding(FILL_INSET)
    .height(Fill);
    let counted = tr!("{}/33 Boosts", boosts);
    debug_assert_eq!(BOOST_GOAL, 33);
    let words = row![
        text(tr!("Boost Goal"))
            .size(typography::size::SMALL)
            .font(typography::SEMIBOLD)
            .width(Fill)
            .wrapping(iced::widget::text::Wrapping::None)
            .ellipsis(iced::widget::text::Ellipsis::End),
        text(counted)
            .size(typography::size::SMALL)
            .font(typography::MEDIUM)
            .style(|theme: &Theme| iced::widget::text::Style {
                color: Some(theme.palette().text.scale_alpha(0.7)),
                ..Default::default()
            }),
        icon(Icon::ChevronRight, 16.0).style(|theme: &Theme, _| iced::widget::svg::Style {
            color: Some(theme.palette().muted),
        }),
    ]
    .spacing(2.0)
    .align_y(Alignment::Center)
    .padding(
        Padding::new(0.0)
            .left(metrics::SPACE_S)
            .right(metrics::SPACE_S),
    )
    .height(Fill);
    let pill = stack![track, filled, words].height(PILL_HEIGHT);
    container(pill)
        .padding(
            Padding::new(0.0)
                .top(PILL_TOP)
                .left(PILL_X)
                .right(metrics::SPACE_L - PILL_X),
        )
        .width(Fill)
        .height(BOOST_HEIGHT - 1.0)
}

/// "N Events": a row that opens the events menu below it.
fn events_row(events: u32, motion: Motion) -> impl View<Intent> {
    let words = if events == 0 {
        tr!("Events").to_owned()
    } else {
        trn!("{n} Event", "{n} Events", events as usize)
    };
    let content = row![
        icon(Icon::Calendar, metrics::ICON_ROW).style(|theme: &Theme, _| {
            iced::widget::svg::Style {
                color: Some(theme.palette().muted),
            }
        }),
        text(words)
            .size(typography::size::SMALL)
            .font(typography::MEDIUM)
            .style(catalog::text::muted)
            .wrapping(iced::widget::text::Wrapping::None)
            .ellipsis(iced::widget::text::Ellipsis::End)
    ]
    .spacing(metrics::SPACE_S)
    .align_y(Alignment::Center)
    .width(Fill)
    .height(Fill)
    .padding(Padding::new(0.0).left(metrics::SPACE_S));
    let button = opener(content).on_press(|bounds| {
        Intent::Events(Spot {
            gap: 0.0,
            ..Spot::below(bounds)
        })
    });
    container(lit(button, false, motion))
        .padding(Padding::new(EVENTS_INSET).bottom(EVENTS_INSET - 1.0))
        .width(Fill)
        .height(EVENTS_HEIGHT - 1.0)
}
