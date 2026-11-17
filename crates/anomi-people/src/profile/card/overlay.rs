//! What lies over the card's contents, each in a layer of its own: their
//! custom status in a thought bubble beside the picture
//! (`ui/people.slint:1521`-`:1581`), and the friendship's and the menu's
//! round buttons over the banner's top right corner (`:1981`-`:2013`).
//!
//! The bubble shows two lines, and all of them while the pointer is on it,
//! over what lies below, as Discord's.

use iced::widget::{Row, button, container, hover, mouse_area, pin, space, stack};
use iced::{Background, Border, Color, Fill, Padding, Shadow};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_theme::rgba;
use anomi_vm::icon::Icon;
use anomi_vm::kind::Status;
use anomi_vm::menu::Spot;
use anomi_vm::people::profile::{Card, Friendship, Intent, Place};
use anomi_vm::people::{self};
use anomi_widgets::icon::icon;
use anomi_widgets::menu::opener;
use anomi_widgets::presence::status_text;

use super::look::{Inks, Size, Tone};

/// The card's bubble: its dots, where it hangs, its words' size, inset
/// and the height they keep while it's closed (`:1557`-`:1581`).
const DOTS: [(f32, f32, f32); 2] = [(7.0, 2.0, -16.0), (11.0, 11.0, -9.0)];
/// The full profile's, rising from below the banner (`:1526`-`:1555`).
const SIDE_DOTS: [(f32, f32, f32); 2] = [(8.0, 6.0, 3.0), (16.0, 18.0, 8.0)];
/// The full profile's bubble is no wider than this.
const SIDE_BUBBLE: f32 = 180.0;
/// Two lines, then the rest while the pointer is on it.
const CLOSED_LINES: u32 = 2;

/// The corner buttons' side, glyph, and ground at rest and under the
/// pointer: black at 50% and 70%, whatever the theme (`:1987`).
const ROUND: f32 = 32.0;
const GLYPH: f32 = 18.0;
const SHADE: u32 = 0x00000080;
const SHADE_HOVER: u32 = 0x000000b3;
const CORNER_GAP: f32 = 8.0;
const CORNER_IN: f32 = 10.0;

/// The thought bubble and its dots, placed from the card's top left
/// (`inset` is the theme's border).
pub fn bubble<'a>(
    card: &'a Card,
    status: &'a Status,
    place: Place,
    size: Size,
    inks: &Inks,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let large = place.large();
    let inset = if inks.theme.is_some() { 2.0 } else { 0.0 };
    let beside = size.pad + size.avatar + inset;
    let (dots, at, words, pad, radius) = if large {
        (
            SIDE_DOTS,
            (10.0, 14.0),
            14.0,
            Padding::from([8.0, 13.0]),
            16.0,
        )
    } else {
        (DOTS, (12.0, 4.0), 13.0, Padding::from([7.0, 12.0]), 14.0)
    };
    let x = beside + at.0;
    let mut room = size.width - x - 12.0;
    if large {
        room = room.min(SIDE_BUBBLE);
    }
    let (fill, edge) = (inks.bubble, inks.bubble_edge);
    let round = move |side: f32| {
        container(space().width(side).height(side)).style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(fill.at(palette))),
                border: Border::default()
                    .rounded(side / 2.0)
                    .width(1.0)
                    .color(edge.at(palette)),
                ..container::Style::default()
            }
        })
    };
    let open = card.touch.bubble;
    let mut text = status_text(status, motion)
        .size(words)
        .wrap(true)
        .max_lines(if open { 0 } else { CLOSED_LINES });
    text = match inks.ink {
        Tone::Fixed(ink) => text.ink(ink),
        Tone::Token(token) => text.color(token),
    };
    let body = container(text.view())
        .padding(pad)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(fill.at(palette))),
                border: Border::default()
                    .rounded(radius)
                    .width(1.0)
                    .color(edge.at(palette)),
                ..container::Style::default()
            }
        });
    let body = mouse_area(body)
        .on_enter(Intent::Bubble(place, true).into())
        .on_exit(Intent::Bubble(place, false).into());
    // As wide as its words, no wider than the room beside the picture.
    let body = container(body).width(room);
    let y = size.banner + at.1 + inset;
    let mut layers = stack![space().width(Fill).height(Fill)];
    for (side, dx, dy) in dots {
        layers = layers.push(
            pin(round(side))
                .x(beside + dx)
                .y(size.banner + dy + inset)
                .boxed(),
        );
    }
    layers = layers.push(pin(body).x(x).y(y).boxed());
    if large {
        // The bump joins the bubble with no line between it and its dot.
        let bump = container(space().width(12.0).height(2.0)).style(move |theme: &Theme| {
            container::Style {
                background: Some(Background::Color(fill.at(theme.palette()))),
                ..container::Style::default()
            }
        });
        layers = layers.push(pin(bump).x(x + 10.0).y(y).boxed());
    }
    layers.boxed()
}

/// The friendship's button and the person's menu over the banner's top
/// right corner, on a card that isn't the full profile's or a preview.
pub fn corner(card: &Card) -> Option<Element<'_, people::Intent>> {
    if card.friend == Friendship::None && card.id.is_empty() {
        return None;
    }
    let mut buttons = Row::<Element<'_, people::Intent>>::new().spacing(CORNER_GAP);
    let id = card.id.clone();
    match card.friend {
        Friendship::None => {}
        Friendship::Friends => {
            let id = id.clone();
            buttons = buttons.push(
                opener(hover(
                    round(Icon::Check, false, true),
                    round(Icon::Check, true, true),
                ))
                .on_press(move |at| Intent::More(id.clone(), Spot::below(at)).into())
                .boxed(),
            );
        }
        Friendship::Sent => buttons = buttons.push(round(Icon::UserPlus, false, false)),
        Friendship::Offer | Friendship::Theirs => {
            buttons = buttons.push(
                button(hover(
                    round(Icon::UserPlus, false, true),
                    round(Icon::UserPlus, true, true),
                ))
                .padding(0)
                .style(catalog::button::bare)
                .on_press(Intent::Befriend(id.clone()).into())
                .boxed(),
            );
        }
    }
    if !id.is_empty() {
        buttons = buttons.push(
            opener(hover(
                round(Icon::Ellipsis, false, true),
                round(Icon::Ellipsis, true, true),
            ))
            .on_press(move |at| Intent::More(id.clone(), Spot::below(at)).into())
            .boxed(),
        );
    }
    Some(
        container(buttons)
            .width(Fill)
            .align_right(Fill)
            .padding(CORNER_IN)
            .boxed(),
    )
}

/// A round button's face: white on black at half, darker under the
/// pointer; a request already sent is faded and does nothing.
fn round<'a>(glyph: Icon, hovered: bool, enabled: bool) -> Element<'a, people::Intent> {
    let shade = rgba(if hovered { SHADE_HOVER } else { SHADE });
    let ink = Color::WHITE.scale_alpha(if enabled { 1.0 } else { 0.5 });
    container(
        icon(glyph, GLYPH).style(move |_: &Theme, _| iced::widget::svg::Style { color: Some(ink) }),
    )
    .center(ROUND)
    .style(move |_: &Theme| container::Style {
        background: Some(Background::Color(shade)),
        border: Border::default().rounded(ROUND / 2.0),
        shadow: Shadow::default(),
        ..container::Style::default()
    })
    .boxed()
}
