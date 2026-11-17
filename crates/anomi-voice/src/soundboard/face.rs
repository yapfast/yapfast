//! A sound's emoji at the chip's 18 px: Twemoji's picture of a Unicode one,
//! a custom one's own, or a quiet square while that comes
//! (`ui/messages.slint:94`, `ui/voice.slint:1663`).

use iced::widget::text::LineHeight;
use iced::widget::{container, space, stack, text};
use iced::{Border, Size};

use anomi_theme::fonts::{self, twemoji};
use anomi_theme::prelude::*;
use anomi_vm::voice::soundboard::Emoji;
use anomi_widgets::picture::{Fit, picture};

/// The emoji's box (`side: 18px; width: 18px`).
pub const SIDE: f32 = 18.0;
/// The quiet square of a picture on its way: of the box's side, its
/// corners, and the overlay's alpha.
const SQUARE: f32 = 0.8;
const SQUARE_RADIUS: f32 = 0.2;
const SQUARE_ALPHA: f32 = 0.07;
/// Twemoji's pictures are 1.36 em square at 53 px per 72: a picture `SIDE`
/// across is text of this much of it.
const TWEMOJI_EM: f32 = 53.0 / 72.0;
/// Rounding sets a glyph up to a pixel below the middle of its line at 1x
/// (none at 2x): half a pixel up fills the box as a custom picture does at
/// 1x and leaves a physical pixel to spare at 2x.
const LIFT: f32 = 0.5;

/// `emoji` in its 18 px box, a custom one's picture at `fade`.
pub fn emoji<'a, M: 'a>(emoji: &'a Emoji, fade: f32, motion: Motion) -> Element<'a, M> {
    let room = Size::new(SIDE, SIDE);
    match emoji {
        Emoji::Glyph(glyph) => {
            let face = text(twemoji::text(glyph.as_str()))
                .font(fonts::EMOJI)
                .size(SIDE * TWEMOJI_EM)
                .line_height(LineHeight::Absolute((SIDE - 2.0 * LIFT).into()));
            // Top-aligned, the shorter line lifts the glyph `LIFT` to fill
            // the box as a custom picture does.
            let face = container(face).center_x(room.width).height(room.height);
            if fade < 1.0 {
                // Glyphs ignore a text colour's alpha here: veil it with the
                // disabled chip's ground (`button_default`).
                let veil =
                    container(space())
                        .width(SIDE)
                        .height(SIDE)
                        .style(move |theme: &Theme| container::Style {
                            background: Some(
                                theme
                                    .palette()
                                    .button_default
                                    .scale_alpha(1.0 - fade)
                                    .into(),
                            ),
                            ..container::Style::default()
                        });
                stack![face, veil].boxed()
            } else {
                face.boxed()
            }
        }
        Emoji::Custom(expected) => match expected.ready() {
            Some(handle) => container(
                picture(handle.clone())
                    .side(SIDE)
                    .fit(Fit::Contain)
                    .opacity(fade)
                    .motion(motion),
            )
            .center_x(room.width)
            .center_y(room.height)
            .boxed(),
            None => square(fade, room),
        },
        Emoji::None => container(space()).width(SIDE).height(SIDE).boxed(),
    }
}

fn square<'a, M: 'a>(fade: f32, room: Size) -> Element<'a, M> {
    let square = container(space())
        .width(SIDE * SQUARE)
        .height(SIDE * SQUARE)
        .style(move |theme: &Theme| container::Style {
            background: Some(theme.palette().lift(SQUARE_ALPHA * fade).into()),
            border: Border::default().rounded(SIDE * SQUARE_RADIUS),
            ..container::Style::default()
        });
    container(square)
        .center_x(room.width)
        .center_y(room.height)
        .boxed()
}
