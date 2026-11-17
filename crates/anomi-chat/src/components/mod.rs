//! A bot message's components: Discord's containers (framed with a bar of
//! their colour) and the components outside one, as rows of text with a
//! thumbnail or a button beside it, galleries, file cards, gaps, buttons and
//! menus. Ports `CardView` and `CardButtonView` (`ui/messages.slint:2225`,
//! `:2257`); the line of why the last control failed is the message's
//! (`message::notice::failed`). Task C06.

mod button;
mod media;
mod rows;
#[cfg(feature = "shots")]
mod shots;

use iced::widget::{column, container, space, stack};
use iced::{Border, Fill, Length, Padding, border};

use anomi_theme::prelude::*;
use anomi_vm::chat::components::{Card, Components, Intent};

#[cfg(feature = "shots")]
pub use shots::shots;

/// A framed container's widest: `max-width: 600px`.
const FRAMED_WIDTH: f32 = 600.0;
const RADIUS: f32 = 8.0;
/// Its padding, and the space between its rows.
const PADDING: f32 = 16.0;
const SPACING: f32 = 8.0;
/// The accent bar's width.
const BAR: f32 = 4.0;
/// Over each card: `padding-top: 4px`.
const TOP: f32 = 4.0;

/// Every card of a message, in a column; nothing for a message without.
pub fn view(components: &Components, motion: Motion) -> impl View<Intent> + '_ {
    let mut cards = column![].width(Fill);
    for card in &components.cards {
        cards = cards.push(
            container(card_view(components, card, motion))
                .padding(Padding::default().top(TOP))
                .width(Fill),
        );
    }
    cards
}

fn card_view<'a>(
    components: &'a Components,
    card: &'a Card,
    motion: Motion,
) -> Element<'a, Intent> {
    let mut inner = column![].spacing(SPACING).width(Fill);
    for row in &card.rows {
        inner = inner.push(rows::view(components, row, card.framed, motion));
    }
    if !card.framed {
        return inner.boxed();
    }
    let frame = container(inner)
        .padding(PADDING)
        .width(Fill)
        .style(|theme: &Theme| container::Style::default().background(theme.palette().sidebar));
    let framed = match card.color {
        // The bar lies over the left padding, under the frame's rim, its
        // corners the frame's.
        Some(color) => stack![
            frame,
            container(space().width(BAR).height(Fill)).style(move |_theme: &Theme| {
                container::Style {
                    background: Some(color.into()),
                    border: Border::default().rounded(border::left(RADIUS)),
                    ..container::Style::default()
                }
            }),
            rim(),
        ]
        .width(Fill)
        .boxed(),
        None => stack![frame, rim()].width(Fill).boxed(),
    };
    // At most 600 wide: the rest of the line stays empty.
    container(framed)
        .width(Length::Fill.max(FRAMED_WIDTH))
        .boxed()
}

/// A framed card's rim, over all it holds as Slint draws a border.
fn rim<'a>() -> Element<'a, Intent> {
    container(space())
        .width(Fill)
        .height(Fill)
        .style(|theme: &Theme| container::Style {
            border: Border {
                color: theme.palette().edge,
                width: 1.0,
                radius: RADIUS.into(),
            },
            ..container::Style::default()
        })
        .boxed()
}
