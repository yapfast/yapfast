//! What the cell and full screen share: white over black in every theme, as
//! the Slint player draws it (`ui/messages.slint:412`-`:600`, `:2731`-`:2830`):
//! the frame on its black ground, the play disc, the "Loading..." pill and
//! the box that says why it can't play.

use std::sync::LazyLock;

use iced::widget::{container, svg, text};
use iced::{Background, Border, Color, Fill, Length, Padding};
use yapfast_lang::tr;

use anomi_theme::prelude::*;
use anomi_theme::rgba;
use anomi_theme::typography::size;
use anomi_vm::kind::Live;
use anomi_widgets::picture::{Fit, live};

/// The ink of everything over the video.
pub const INK: Color = Color::WHITE;
/// Behind a frame, where it doesn't fill the room.
pub const GROUND: Color = Color::BLACK;
/// The play disc's and the loading pill's ground (`#000000b3`).
pub const DISC_GROUND: Color = rgba(0x0000_00b3);
/// The error box's and the volume panel's (`#000000cc`).
pub const DARK: Color = rgba(0x0000_00cc);
/// A track's rail (`#ffffff4d`).
pub const RAIL: Color = rgba(0xffff_ff4d);

/// The play disc: 48 across, its sharp triangle 18 by 20 lying 17 from its
/// left and 14 from its top, so it looks centred (`:579`).
const DISC: f32 = 48.0;
const TRIANGLE: (f32, f32) = (18.0, 20.0);
const TRIANGLE_AT: (f32, f32) = (17.0, 14.0);
/// The pills: 28 high, 10 either side of their text (`:587`, `:592`).
const PILL: f32 = 28.0;
const PILL_PAD: f32 = 10.0;
const ERROR_RADIUS: f32 = 6.0;
/// The error box keeps 8 from the cell's sides and bottom (`:592`).
const ERROR_MARGIN: f32 = 8.0;

/// The newest frame, fitted whole into the room on black, its corners and
/// the room's `radius`, read as it's drawn: a frame redraws its rectangle
/// and builds no view, and is drawn as it is, never copied, the renderer
/// cutting its corners (`picture::Player` draws such a feed straight).
pub fn frame<'a, Message: 'a>(frames: &Live, radius: f32) -> Element<'a, Message> {
    container(
        live(frames)
            .width(Fill)
            .height(Fill)
            .fit(Fit::Contain)
            .radius(radius),
    )
    .width(Fill)
    .height(Fill)
    .style(move |_: &Theme| container::Style {
        background: Some(Background::Color(GROUND)),
        border: Border::default().rounded(radius),
        ..container::Style::default()
    })
    .boxed()
}

/// The disc with its play triangle, in the middle of the room.
pub fn disc<'a, Message: 'a>() -> Element<'a, Message> {
    static PATH: LazyLock<svg::Handle> = LazyLock::new(|| {
        svg::Handle::from_memory(
            &b"<svg xmlns='http://www.w3.org/2000/svg' width='18' height='20' viewBox='0 0 18 20'>\
<path d='M0 0L18 10L0 20Z' fill='#fff'/></svg>"[..],
        )
    });
    let glyph = svg(PATH.clone())
        .width(TRIANGLE.0)
        .height(TRIANGLE.1)
        .style(|_: &Theme, _| svg::Style { color: Some(INK) });
    let disc = container(glyph)
        .width(DISC)
        .height(DISC)
        .padding(Padding::ZERO.left(TRIANGLE_AT.0).top(TRIANGLE_AT.1))
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(DISC_GROUND)),
            border: Border::default().rounded(DISC / 2.0),
            ..container::Style::default()
        });
    container(disc).center(Fill).boxed()
}

/// "Loading..." in a round pill in the middle of the room.
pub fn loading<'a, Message: 'a>() -> Element<'a, Message> {
    container(pill(
        tr!("Loading..."),
        DISC_GROUND,
        PILL / 2.0,
        Length::Shrink,
    ))
    .center(Fill)
    .boxed()
}

/// Why it can't play, in a box: at the cell's bottom (`bottom`), or in the
/// middle full screen. Long words end in an ellipsis.
pub fn failure<'a, Message: 'a>(why: &'a str, bottom: bool) -> Element<'a, Message> {
    let card = pill(why, DARK, ERROR_RADIUS, Length::Shrink);
    let room = container(card)
        .width(Fill)
        .height(Fill)
        .padding(ERROR_MARGIN)
        .center_x(Fill);
    if bottom {
        room.align_bottom(Fill).boxed()
    } else {
        room.center_y(Fill).boxed()
    }
}

fn pill<'a, Message: 'a>(
    words: &'a str,
    ground: Color,
    radius: f32,
    width: Length,
) -> Element<'a, Message> {
    let words = text(words)
        .size(size::HEADING)
        .color(INK)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End);
    container(words)
        .height(PILL)
        .width(width)
        .center_y(PILL)
        .padding(Padding::ZERO.left(PILL_PAD).right(PILL_PAD))
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(ground)),
            border: Border::default().rounded(radius),
            ..container::Style::default()
        })
        .boxed()
}
