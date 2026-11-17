//! An emoji at a size: Twemoji's picture, a custom emoji's own, or a quiet
//! square while that comes (`EmojiFace`, `ui/messages.slint:64`); and what
//! shows a section in the rail (`PickerRail`, `:145`-`:171`) and in the
//! foot (its icon's `EmojiCell`, `src/emoji.rs:527`).

use iced::widget::text::LineHeight;
use iced::widget::{container, space, svg, text};
use iced::{Border, Color, Size};

use anomi_theme::fonts::{self, twemoji};
use anomi_theme::prelude::*;
use anomi_theme::rgb;
use anomi_vm::chat::emoji_picker::{Emoji, Section, SectionIcon};
use anomi_vm::kind::Expected;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::picture::{Fit, picture};

/// The quiet square of a custom emoji on its way: of the face's side, its
/// corners, and the overlay's alpha over its ground (`:95`-`:98`).
const SQUARE: f32 = 0.8;
const SQUARE_RADIUS: f32 = 0.2;
const SQUARE_ALPHA: f32 = 0.07;
/// An emoji font's line box is taller than its size: Twemoji's text gets
/// one and a half times it, so nothing is cut (`:89`-`:92`).
const LINE: f32 = 1.5;
/// Twemoji's pictures are 72 pixels in a font of 53 pixels per em, 1.36 em
/// square (`anomi_fonts::tables::TWEMOJI`); Slint's font drew them one em
/// square. A picture `side` across is text of this much of it.
const TWEMOJI_EM: f32 = 53.0 / 72.0;
/// A server's picture in the rail: 8 px corners, as the server rail rounds
/// its own (`:165`).
const SERVER_RADIUS: f32 = 8.0;
/// Favourites' star is gold in every theme (`colorize: #f0b232`, `:150`).
pub const GOLD: Color = rgb(0xf0b232);

/// The icon set's glyph of Favourites and Frequently Used: the star in
/// gold, the clock in the secondary text colour.
pub fn glyph<'a, M: 'a>(icon_of: &SectionIcon, side: f32) -> Option<Element<'a, M>> {
    let (glyph, gold) = match icon_of {
        SectionIcon::Favorites => (Icon::StarFilled, true),
        SectionIcon::Frequent => (Icon::Clock, false),
        _ => return None,
    };
    let svg = icon(glyph, side).style(move |theme: &Theme, _| svg::Style {
        color: Some(if gold {
            GOLD
        } else {
            theme.palette().secondary
        }),
    });
    Some(svg.boxed())
}

/// Twemoji's picture of `glyph`, `side` across, centred in `room`.
pub fn unicode<'a, M: 'a>(glyph: &'a str, side: f32, room: Size) -> Element<'a, M> {
    let face = text(twemoji::text(glyph))
        .font(fonts::EMOJI)
        .size(side * TWEMOJI_EM)
        .line_height(LineHeight::Relative(LINE));
    container(face)
        .center_x(room.width)
        .center_y(room.height)
        .boxed()
}

/// A picture on its way: a quiet square in its place, at `fade`.
fn square<'a, M: 'a>(side: f32, fade: f32, room: Size) -> Element<'a, M> {
    let square = container(space())
        .width(side * SQUARE)
        .height(side * SQUARE)
        .style(move |theme: &Theme| container::Style {
            background: Some(theme.palette().lift(SQUARE_ALPHA * fade).into()),
            border: Border::default().rounded(side * SQUARE_RADIUS),
            ..container::Style::default()
        });
    container(square)
        .center_x(room.width)
        .center_y(room.height)
        .boxed()
}

/// A picture at `side`, whole, centred in `room`: moving while `motion`
/// plays, at `fade`.
fn pictured<'a, M: 'a>(
    expected: &Expected,
    side: f32,
    room: Size,
    fade: f32,
    motion: Motion,
) -> Element<'a, M> {
    match expected.ready() {
        Some(handle) => container(
            picture(handle.clone())
                .side(side)
                .fit(Fit::Contain)
                .opacity(fade)
                .motion(motion),
        )
        .center_x(room.width)
        .center_y(room.height)
        .boxed(),
        None => square(side, fade, room),
    }
}

/// `emoji` at `side`, centred in `room`, at `fade`: a custom one's picture
/// moves while `motion` plays, as the grid's do and the foot's doesn't.
pub fn emoji<'a, M: 'a>(
    emoji: &'a Emoji,
    side: f32,
    room: Size,
    fade: f32,
    motion: Motion,
) -> Element<'a, M> {
    match emoji {
        Emoji::Unicode(unicode_emoji) => unicode(unicode_emoji.as_str(), side, room),
        Emoji::Custom(custom) => pictured(&custom.picture, side, room, fade, motion),
    }
}

/// A section's button face in the rail, at `side` in `room`: the icon
/// set's glyph, a category's emoji, or a server's picture with rounded
/// corners on a tile of the active colour while it comes.
pub fn rail<'a, M: 'a>(section: &'a Section, side: f32, room: Size) -> Element<'a, M> {
    let face = match &section.icon {
        SectionIcon::Category(emoji) => return unicode(emoji.as_str(), side, room),
        SectionIcon::Server {
            picture: expected, ..
        } => match expected.ready() {
            Some(handle) => picture(handle.clone())
                .side(side)
                .radius(SERVER_RADIUS)
                .fit(Fit::Contain)
                .boxed(),
            None => container(space())
                .width(side)
                .height(side)
                .style(|theme: &Theme| container::Style {
                    background: Some(theme.palette().active.into()),
                    border: Border::default().rounded(SERVER_RADIUS),
                    ..container::Style::default()
                })
                .boxed(),
        },
        icon_of => glyph(icon_of, side).unwrap_or_else(|| space().boxed()),
    };
    container(face)
        .center_x(room.width)
        .center_y(room.height)
        .boxed()
}

/// A section in the foot, as its rail button is pointed at: the emoji its
/// icon is (a star for Favourites, a clock for Frequently Used), or its
/// server's picture, still.
pub fn foot<'a, M: 'a>(section: &'a Section, side: f32, room: Size) -> Element<'a, M> {
    match &section.icon {
        SectionIcon::Favorites => unicode("⭐", side, room),
        SectionIcon::Frequent => unicode("🕒", side, room),
        SectionIcon::Category(emoji) => unicode(emoji.as_str(), side, room),
        SectionIcon::Server {
            picture: expected, ..
        } => pictured(expected, side, room, 1.0, Motion::default()),
    }
}
