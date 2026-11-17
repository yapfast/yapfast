//! The emoji's well beside the words (`ui/dialogs.slint:77`): the smile with
//! a plus while there is none, the emoji itself once there is, a press
//! opening the picker beside it, and a small badge on its corner that takes
//! the emoji off.

use iced::widget::{button, container, hover, stack, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_vm::icon::Icon;
use anomi_vm::kind::Expected;
use anomi_vm::menu::Spot;
use anomi_vm::people::status::{Emoji, Intent};
use anomi_widgets::icon::icon;
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{Fit, picture};

use super::dialog::{OVERHANG, WELL};

/// The well's corner (`border-radius: 6px`).
const RADIUS: f32 = 6.0;
/// The emoji's size, the smile's width (`side: 22px`, `width: 20px`).
const FACE: f32 = 22.0;
const SMILE: f32 = 20.0;
/// The badge: `x: parent.width - 13px; y: -5px; 18px`, a 2 px ring of the
/// card's colour, its cross 10 px.
const BADGE: f32 = 18.0;
const BADGE_RING: f32 = 2.0;
const CROSS: f32 = 10.0;
const BADGE_LEFT: f32 = WELL - 13.0;

/// The well and its badge, in a box that keeps the badge's overhang.
pub(super) fn emoji_button(emoji: Option<&Emoji>) -> Element<'_, Intent> {
    // At rest and under the pointer, as iced's `hover` lays one over the
    // other; a press carries where the well is, for the picker to open
    // beside.
    let rest = well(face(emoji), false);
    let lit = well(face(emoji), true);
    let well = opener(hover(rest, lit)).on_press(|bounds| Intent::PickEmoji(Spot::beside(bounds)));
    let mut layers = stack![
        container(well)
            .padding(Padding {
                top: OVERHANG,
                ..Padding::ZERO
            })
            .boxed()
    ];
    if emoji.is_some() {
        layers = layers.push(
            container(remove())
                .padding(Padding {
                    left: BADGE_LEFT,
                    ..Padding::ZERO
                })
                .boxed(),
        );
    }
    layers
        .width(WELL + OVERHANG)
        .height(WELL + OVERHANG)
        .boxed()
}

/// What lies in the well: the emoji, or the smile with a plus.
fn face(emoji: Option<&Emoji>) -> Element<'_, Intent> {
    match emoji {
        Some(emoji) => emoji_face(emoji),
        None => icon(Icon::SmilePlus, SMILE)
            .style(|theme: &Theme, _| iced::widget::svg::Style {
                color: Some(theme.palette().secondary),
            })
            .boxed(),
    }
}

/// The well, in its fill at rest or under the pointer.
fn well(face: Element<'_, Intent>, hovered: bool) -> Element<'_, Intent> {
    container(face)
        .center(Fill)
        .width(WELL)
        .height(WELL)
        .style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(if hovered {
                    palette.hover
                } else {
                    palette.sunken
                })),
                border: Border::default().rounded(RADIUS),
                ..container::Style::default()
            }
        })
        .boxed()
}

/// The emoji: a Unicode one as text, which Twemoji draws, a server's as its
/// picture, or nothing while that is on its way.
fn emoji_face(emoji: &Emoji) -> Element<'_, Intent> {
    if emoji.glyph.is_empty() {
        match &emoji.picture {
            Expected::Ready(handle) => picture(handle.clone()).side(FACE).fit(Fit::Contain).boxed(),
            _ => container(text("")).width(FACE).height(FACE).boxed(),
        }
    } else {
        text(emoji.glyph.as_str())
            .size(FACE)
            .align_x(Alignment::Center)
            .boxed()
    }
}

/// The badge on the corner: an 18 px disc in the card's colour with a 14 px
/// one in it, faint, red under the pointer. No border: the faint fill would
/// show through a ring's anti-aliased outer edge.
fn remove<'a>() -> Element<'a, Intent> {
    let disc = |lit: bool| -> Element<'a, Intent> {
        let cross = icon(Icon::X, CROSS).style(|theme: &Theme, _| iced::widget::svg::Style {
            color: Some(theme.palette().text),
        });
        let inner = container(cross).center(Fill).style(move |theme: &Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(Background::Color(if lit {
                    palette.danger
                } else {
                    palette.faint
                })),
                border: Border::default().rounded((BADGE - 2.0 * BADGE_RING) / 2.0),
                ..container::Style::default()
            }
        });
        container(inner)
            .padding(BADGE_RING)
            .width(BADGE)
            .height(BADGE)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().raised)),
                border: Border::default().rounded(BADGE / 2.0),
                ..container::Style::default()
            })
            .boxed()
    };
    button(hover(disc(false), disc(true)))
        .width(BADGE)
        .height(BADGE)
        .padding(0)
        .on_press(Intent::RemoveEmoji)
        .style(|_: &Theme, _| button::Style::default())
        .boxed()
}
