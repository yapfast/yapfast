//! A found message's first pictures, laid out as the timeline's gallery but
//! smaller (`FoundCard`'s `images`, `ui/header.slint:221`). Clicks go through
//! to the card, which shows the message: a spoiler stays covered.

use iced::Fill;
use iced::widget::{container, pin, space};

use anomi_theme::prelude::*;
use anomi_theme::rgba;
use anomi_vm::chat::search::{Cell, Found};
use anomi_vm::kind::Expected;
use anomi_widgets::cover::spoiler_cover;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::picture::{Fit, blurred, picture};
use anomi_widgets::skeleton::skeleton;

/// A cell's corner.
const RADIUS: f32 = 8.0;
/// The play mark over a video's poster: `#000000b3`.
const MARK: f32 = 40.0;
const MARK_GLYPH: f32 = 20.0;
const MARK_GROUND: u32 = 0x0000_00b3;

/// The cells in a box the gallery's size, each at its place.
pub fn view<'a, Message: 'a>(found: &'a Found, motion: Motion) -> Element<'a, Message> {
    let (width, height) = found.gallery;
    let cells = found
        .images
        .iter()
        .map(|cell| pin(one(cell, motion)).x(cell.x).y(cell.y).boxed());
    iced::widget::stack(cells)
        .width(width)
        .height(height)
        .boxed()
}

fn one<'a, Message: 'a>(cell: &'a Cell, motion: Motion) -> Element<'a, Message> {
    let (width, height) = (cell.width, cell.height);
    let ground = container(space().width(Fill).height(Fill))
        .width(width)
        .height(height)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().sunken.into()),
            border: iced::Border::default().rounded(RADIUS),
            ..container::Style::default()
        });
    let mut layers: Vec<Element<'a, Message>> = vec![ground.boxed()];
    match &cell.picture {
        Expected::Ready(handle) => {
            // A spoiler's picture shows blurred, as the timeline's does.
            let shown = if cell.spoiler {
                blurred(handle).unwrap_or_else(|| handle.clone())
            } else {
                handle.clone()
            };
            layers.push(
                picture(shown)
                    .width(width)
                    .height(height)
                    .radius(RADIUS)
                    .fit(Fit::Cover)
                    .motion(motion)
                    .boxed(),
            );
            if cell.spoiler {
                layers.push(spoiler_cover(RADIUS).boxed());
            } else if cell.video {
                layers.push(play_mark());
            }
        }
        // Only the sunken ground for a picture that will never come.
        Expected::None => {}
        Expected::Waiting => layers.push(
            skeleton(motion)
                .width(width)
                .height(height)
                .radius(RADIUS)
                .boxed(),
        ),
    }
    iced::widget::stack(layers)
        .width(width)
        .height(height)
        .boxed()
}

/// The disc with a play triangle, centred on a video's poster.
fn play_mark<'a, Message: 'a>() -> Element<'a, Message> {
    let glyph =
        icon(Icon::Play, MARK_GLYPH).style(|_theme: &Theme, _status| iced::widget::svg::Style {
            color: Some(iced::Color::WHITE),
        });
    container(
        container(glyph)
            .center(MARK)
            .style(|_theme: &Theme| container::Style {
                background: Some(rgba(MARK_GROUND).into()),
                border: iced::Border::default().rounded(MARK / 2.0),
                ..container::Style::default()
            }),
    )
    .center(Fill)
    .boxed()
}
