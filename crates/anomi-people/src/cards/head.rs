//! A server card's head: a banner with the server's icon hanging over its
//! lower edge (`ui/people.slint:894`-`:918`, `:985`-`:1006`), and the small
//! parts of the body the cards share: a count's dot, a heading.

use iced::widget::{Column, container, space, stack, svg, text};
use iced::{Alignment, Background, Border, Fill, Padding};

use anomi_theme::metrics;
use anomi_theme::prelude::*;
use anomi_theme::{Palette, typography};
use anomi_vm::icon::Icon;
use anomi_vm::kind::Expected;
use anomi_vm::people::cards::Intent;
use anomi_widgets::icon::icon;
use anomi_widgets::picture::{Fit, picture};

/// The banner's height, and the icon's: its box (the ring that cuts it from
/// the banner), the picture inside, and their corners.
const BANNER: f32 = 90.0;
const RING: f32 = 76.0;
const SIDE: f32 = 64.0;
const RING_RADIUS: f32 = 22.0;
const SIDE_RADIUS: f32 = 18.0;
/// The icon's inset from the left, and how far it hangs below the banner's
/// edge (`y: 90px - 32px - 6px`).
const INSET: f32 = 10.0;
const TOP: f32 = BANNER - 32.0 - 6.0;
/// The head's height: the banner, half the icon's box and 8 px.
const HEIGHT: f32 = BANNER + 32.0 + 8.0;
/// The banner's top corners: the card's.
const CORNER: f32 = metrics::RADIUS_DIALOG;
/// A count's dot and its space to the words (`ui/people.slint:903`).
const COUNT_DOT: f32 = 8.0;
const COUNT_DOT_GAP: f32 = 6.0;
/// The glyph a server without a picture shows when it is private or failed.
const GLYPH: f32 = 24.0;
/// The initial's size.
const INITIAL: f32 = 24.0;

/// What stands in for a server's icon that is not there.
pub enum Stand<'a> {
    /// Its initial, as a server with no picture shows.
    Initial(&'a str),
    /// A lock or a warning, for a profile Discord wouldn't show.
    Glyph(Icon),
}

/// The head: the banner, if there is one, over the raised ground, and the
/// icon over it.
pub fn view<'a>(
    banner: &'a Expected,
    icon_picture: &'a Expected,
    stand: Stand<'a>,
    motion: Motion,
) -> Element<'a, Intent> {
    let banner: Element<'a, Intent> = match banner {
        Expected::Ready(handle) => picture(handle.clone())
            .width(Fill)
            .height(BANNER)
            .fit(Fit::Cover)
            .radius(iced::border::top(CORNER))
            .motion(motion)
            .boxed(),
        _ => container(space())
            .width(Fill)
            .height(BANNER)
            .style(|theme: &Theme| container::Style {
                background: Some(Background::Color(theme.palette().raised)),
                border: Border {
                    radius: iced::border::top(CORNER),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .boxed(),
    };
    let base = Column::<Element<'a, Intent>>::new()
        .push(banner)
        .push(space().height(HEIGHT - BANNER).boxed())
        .boxed();
    // The ring is a quad and the picture over it a picture: the stack puts
    // each in a layer of its own, or the banner's picture would cover it.
    let hanging = container(ring(icon_picture, stand, motion)).padding(Padding {
        top: TOP,
        left: INSET,
        ..Padding::ZERO
    });
    stack![base, hanging].width(Fill).boxed()
}

fn ring<'a>(icon_picture: &'a Expected, stand: Stand<'a>, motion: Motion) -> Element<'a, Intent> {
    let inside: Element<'a, Intent> = match icon_picture {
        Expected::Ready(handle) => picture(handle.clone())
            .side(SIDE)
            .fit(Fit::Cover)
            .radius(SIDE_RADIUS)
            .motion(motion)
            .boxed(),
        _ => container(match stand {
            Stand::Initial(letter) => text(letter)
                .size(INITIAL)
                .font(typography::BOLD)
                .style(anomi_theme::catalog::text::primary)
                .boxed(),
            Stand::Glyph(glyph) => icon(glyph, GLYPH)
                .style(|theme: &Theme, _| svg::Style {
                    color: Some(theme.palette().secondary),
                })
                .boxed(),
        })
        .center(SIDE)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().raised)),
            border: Border::default().rounded(SIDE_RADIUS),
            ..container::Style::default()
        })
        .boxed(),
    };
    container(inside)
        .center(RING)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().sidebar)),
            border: Border::default().rounded(RING_RADIUS),
            ..container::Style::default()
        })
        .boxed()
}

/// A count: an 8 px dot, 6 px, and the words in the second ink at 13 px
/// (`ui/people.slint:903`).
pub fn count<'a>(dot: fn(&Palette) -> iced::Color, words: &'a str) -> Element<'a, Intent> {
    iced::widget::row![
        container(space())
            .width(COUNT_DOT)
            .height(COUNT_DOT)
            .style(move |theme: &Theme| container::Style {
                background: Some(Background::Color(dot(theme.palette()))),
                border: Border::default().rounded(COUNT_DOT / 2.0),
                ..container::Style::default()
            }),
        text(words)
            .size(typography::size::LABEL)
            .style(anomi_theme::catalog::text::secondary)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End),
    ]
    .spacing(COUNT_DOT_GAP)
    .align_y(Alignment::Center)
    .boxed()
}

/// A small heading over a value: "THEIR NICKNAME" (`ProfileHeading`,
/// `ui/people.slint:607`: 12 px, bold, the muted ink).
pub fn heading(words: &str) -> Element<'_, Intent> {
    text(words)
        .size(typography::size::CAPTION)
        .font(typography::BOLD)
        .style(anomi_theme::catalog::text::muted)
        .boxed()
}
