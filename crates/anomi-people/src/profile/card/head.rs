//! The card's head: the banner, its picture or colour, with the avatar in
//! a ring of the card's ground hanging over its lower edge, its presence
//! dot and decoration (`ui/people.slint:1511`-`:1520`, `:1582`-`:1602`).
//! The picture opens the full profile, but on the full profile's own and
//! in the editor's preview.

use iced::widget::{Column, container, mouse_area, pin, space, stack};
use iced::{Background, Border, Fill, mouse};

use anomi_theme::prelude::*;
use anomi_vm::kind::{Expected, Presence};
use anomi_vm::people::profile::{Card, Intent, Place};
use anomi_vm::people::{self};
use anomi_widgets::avatar::portrait;
use anomi_widgets::picture::{Fit, picture};

use super::look::{Inks, Size, Tone};

/// The presence dot's slot on the card's 80 px avatar (16 px, 10 px in
/// from the ring's corner) and on the full profile's 120 px one (Discord's
/// 24 px dot in a 7 px ring, 8 px in), as offsets from the avatar's corner.
const DOT: f32 = 16.0;
const DOT_IN: f32 = 10.0;
const SIDE_DOT: f32 = 38.0;
const SIDE_DOT_IN: f32 = 8.0;

/// The banner and the avatar over its edge, `size.head()` tall. The
/// banner's top corners are the card's, less the theme's 2 px border.
pub fn view<'a>(
    card: &'a Card,
    place: Place,
    size: Size,
    inks: &Inks,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let inset = if inks.theme.is_some() { 2.0 } else { 0.0 };
    let corner = iced::border::top(size.radius - inset);
    let banner: Element<'a, people::Intent> = match &card.banner {
        Expected::Ready(handle) => picture(handle.clone())
            .width(Fill)
            .height(size.banner)
            .fit(Fit::Cover)
            .radius(corner)
            .motion(motion)
            .boxed(),
        _ => {
            let fill = card
                .accent
                .or(card.theme.map(|theme| theme.primary))
                .map_or(Tone::Token(|palette| palette.raised), Tone::Fixed);
            container(space())
                .width(Fill)
                .height(size.banner)
                .style(move |theme: &Theme| container::Style {
                    background: Some(Background::Color(fill.at(theme.palette()))),
                    border: Border {
                        radius: corner,
                        ..Border::default()
                    },
                    ..container::Style::default()
                })
                .boxed()
        }
    };
    let base = Column::<Element<'a, people::Intent>>::new()
        .push(banner)
        .push(space().height(size.head() - size.banner).boxed());
    let at = (
        size.pad - size.ring,
        size.banner - size.avatar / 2.0 - size.ring,
    );
    // The ring is a quad and the avatar a picture: each in a layer of the
    // stack, or the banner's picture would cover the ring.
    stack![
        base,
        pin(avatar(card, place, size, inks, motion)).x(at.0).y(at.1)
    ]
    .width(Fill)
    .boxed()
}

/// The avatar in its ring, the dot cut from the same ground. An animated
/// one plays while the pointer is on the card, as Discord's does.
fn avatar<'a>(
    card: &'a Card,
    place: Place,
    size: Size,
    inks: &Inks,
    motion: Motion,
) -> Element<'a, people::Intent> {
    let (dot, dot_in) = if place.large() {
        (SIDE_DOT, SIDE_DOT_IN)
    } else {
        (DOT, DOT_IN)
    };
    // The ring's corner is the avatar's less the ring; the dot sits
    // `dot_in` in from the ring's.
    let corner = size.avatar + size.ring - dot - dot_in;
    let presence = match card.presence {
        Presence::Offline => Presence::None,
        presence => presence,
    };
    let face = portrait(&card.avatar, size.avatar)
        .ground(inks.ring)
        .presence(presence)
        .dot(dot, (corner, corner))
        .decoration(card.decoration.as_ref())
        .motion(if card.touch.pointed {
            motion
        } else {
            Motion::default()
        })
        .view();
    let ground = inks.ground;
    let side = size.avatar + 2.0 * size.ring;
    let ringed = container(face)
        .center(side)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(ground.at(theme.palette()))),
            border: Border::default().rounded(side / 2.0),
            ..container::Style::default()
        });
    if matches!(place, Place::Popout | Place::Panel) {
        mouse_area(ringed)
            .on_press(Intent::Enlarge(place).into())
            .interaction(mouse::Interaction::Pointer)
            .boxed()
    } else {
        ringed.boxed()
    }
}
