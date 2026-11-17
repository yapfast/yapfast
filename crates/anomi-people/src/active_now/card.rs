//! A card: a head with the first friend's face, their names and what they
//! do, and under it a row apiece. A click on the head asks for the first
//! friend's profile. The Slint side: `ActiveCard` (`ui/active_now.slint:140`).

use std::borrow::Cow;

use iced::border::Radius;
use iced::widget::{column, container, row, space, stack};
use iced::{Alignment, Fill, Length, Point};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::kind::Expected;
use anomi_vm::menu::Spot;
use anomi_vm::people::active_now::{Card, Intent};
use anomi_widgets::avatar::portrait;
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{Fit, picture};

use super::item::{line, view as row_view};
use super::lit::lit;
use super::time;

/// The head's height, its picture, and the picture at its end.
const HEAD: f32 = 62.0;
const FACE: f32 = 32.0;
const ICON_RADIUS: f32 = 8.0;
/// The head's lines: the names 15 px, what they do 13.
const NAMES: f32 = 15.0;
const NAMES_LINE: f32 = 19.0;
const DOING_LINE: f32 = 17.0;

pub fn view<'a>(card: &'a Card, now: i64, motion: Motion) -> Element<'a, Intent> {
    let mut parts = column![head(card, now, motion)];
    let last = card.rows.len().saturating_sub(1);
    for (at, item) in card.rows.iter().enumerate() {
        parts = parts.push(
            container(space())
                .width(Fill)
                .height(1)
                .style(|theme: &Theme| container::Style {
                    background: Some(theme.palette().line.into()),
                    ..container::Style::default()
                })
                .boxed(),
        );
        parts = parts.push(row_view(item, now, at == last, motion));
    }
    // The border is a layer over the content, which Slint's `clip: true`
    // keeps inside it: the separators and the fills stop at the edge.
    let ground = container(parts)
        .width(Fill)
        .clip(true)
        .style(|theme: &Theme| {
            let mut style = catalog::container::raised(theme);
            style.border.width = 0.0;
            style
        });
    let edge = container(space().width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .style(|theme: &Theme| container::Style {
            background: None,
            border: catalog::container::raised(theme).border,
            ..container::Style::default()
        });
    stack![ground, edge].width(Fill).boxed()
}

/// The head: lit under the pointer, a pointer's cursor over it.
fn head<'a>(card: &'a Card, now: i64, motion: Motion) -> Element<'a, Intent> {
    // The fill spans the card's outer edge, so it takes the card's radius;
    // a card with no rows has the head for its foot too.
    let foot = if card.rows.is_empty() {
        metrics::RADIUS_CARD
    } else {
        0.0
    };
    let corners = Radius {
        top_left: metrics::RADIUS_CARD,
        top_right: metrics::RADIUS_CARD,
        bottom_right: foot,
        bottom_left: foot,
    };
    // Slint opens the menu at the head's bottom left, with no gap.
    opener(lit(body(card, now), corners, motion))
        .on_press(move |bounds| {
            Intent::OpenPerson(
                card.person.clone(),
                Spot::pointer(Point::new(bounds.x, bounds.y + bounds.height)),
            )
        })
        .boxed()
}

fn body<'a>(card: &'a Card, now: i64) -> Element<'a, Intent> {
    let doing: Cow<'a, str> = if card.start > 0 {
        Cow::Owned(format!(
            "{} – {}",
            card.doing,
            time::since(now - card.start)
        ))
    } else {
        Cow::Borrowed(card.doing.as_str())
    };
    let names = line(card.title.as_str(), NAMES, NAMES_LINE)
        .font(typography::BOLD)
        .style(catalog::text::primary);
    let what = line(doing, typography::size::LABEL, DOING_LINE).style(catalog::text::muted);
    let mut content = row![
        portrait(&card.face, FACE).view(),
        column![names, what].width(Fill)
    ]
    .spacing(metrics::SPACE_M)
    .align_y(Alignment::Center);
    match &card.icon {
        Expected::None => {}
        Expected::Waiting => content = content.push(space().width(FACE).height(FACE).boxed()),
        Expected::Ready(handle) => {
            content = content.push(
                picture(handle.clone())
                    .side(FACE)
                    .radius(ICON_RADIUS)
                    .fit(Fit::Cover)
                    .boxed(),
            );
        }
    }
    container(content)
        .padding([0.0, metrics::SPACE_L])
        .width(Fill)
        .height(Length::Fixed(HEAD))
        .center_y(HEAD)
        .boxed()
}
