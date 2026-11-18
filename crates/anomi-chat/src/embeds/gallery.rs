//! A card's pictures, each at its place and shrunk together to fit the card
//! as a message's gallery is: a spoiler under its cover, a video's poster
//! with its play button, a video file playing in its cell.

use iced::Fill;
use iced::mouse::Interaction;
use iced::widget::{container, mouse_area, pin, space, stack};

use anomi_theme::prelude::*;
use anomi_theme::rgba;
use anomi_vm::chat::embeds::{Cell, Gallery, Intent};
use anomi_vm::kind::Expected;
use anomi_vm::menu::Spot;
use anomi_widgets::cover::spoiler;
use anomi_widgets::icon::{Icon, icon};
use anomi_widgets::menu::opener;
use anomi_widgets::picture::{Fit, blurred, picture};
use anomi_widgets::skeleton::skeleton;

/// A cell's corner: `border-radius: 4px`.
const RADIUS: f32 = 4.0;
/// The play button: 48 px, `#000000b3`, its triangle 18 by 20.
const BUTTON: f32 = 48.0;
const GLYPH: f32 = 22.0;
const BUTTON_GROUND: u32 = 0x0000_00b3;
/// What a card's inside holds across at its widest: 432 less its padding
/// (16 + 16), the bar lying over the left one.
const ROOM: f32 = 400.0;

/// The pictures in a box the gallery's size, scaled to `ROOM` less
/// `beside` (the thumbnail's share) when it is wider.
pub fn view<'a>(
    item: u64,
    url: &'a str,
    gallery: &'a Gallery,
    beside: f32,
    motion: Motion,
) -> Element<'a, Intent> {
    let scale = ((ROOM - beside) / gallery.width.max(1.0)).min(1.0);
    let cells = gallery.cells.iter().map(|cell| {
        pin(one(item, url, gallery.video, cell, scale, motion))
            .x(cell.x * scale)
            .y(cell.y * scale)
            .boxed()
    });
    stack(cells)
        .width(gallery.width * scale)
        .height(gallery.height * scale)
        .boxed()
}

fn one<'a>(
    item: u64,
    url: &'a str,
    poster: bool,
    cell: &'a Cell,
    scale: f32,
    motion: Motion,
) -> Element<'a, Intent> {
    let (width, height) = (cell.width * scale, cell.height * scale);
    let ground = container(space().width(Fill).height(Fill))
        .width(width)
        .height(height)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().sunken.into()),
            border: iced::Border::default().rounded(RADIUS),
            ..container::Style::default()
        });
    let index = cell.index;
    let mut face: Vec<Element<'a, Intent>> = vec![ground.boxed()];
    let ready = matches!(cell.picture, Expected::Ready(_));
    match &cell.picture {
        Expected::Ready(handle) => {
            let shown = if cell.spoiler {
                blurred(handle).unwrap_or_else(|| handle.clone())
            } else {
                handle.clone()
            };
            face.push(
                picture(shown)
                    .width(width)
                    .height(height)
                    .radius(RADIUS)
                    .fit(Fit::Cover)
                    .motion(motion)
                    .boxed(),
            );
        }
        Expected::Waiting => face.push(
            skeleton(motion)
                .width(width)
                .height(height)
                .radius(RADIUS)
                .boxed(),
        ),
        Expected::None => {}
    }
    // A video file the proxy has plays in the cell once its poster is in;
    // a link to one shows the play button, which opens its page.
    let playing = cell.playing.as_ref().filter(|_| !cell.spoiler && ready);
    if poster && !cell.spoiler && cell.playing.is_none() {
        face.push(play_button());
    }
    let click = if poster {
        Intent::Open(url.to_owned())
    } else {
        Intent::View { item, index }
    };
    let flat = stack(face).width(width).height(height);
    let covered = spoiler(flat, cell.spoiler, RADIUS, Intent::Reveal { item, index });
    let mut layers: Vec<Element<'a, Intent>> = vec![if cell.spoiler {
        covered
    } else {
        mouse_area(covered)
            .on_press(click)
            .interaction(Interaction::Pointer)
            .boxed()
    }];
    if let Some(video) = playing {
        layers.push(
            crate::video::view(video, RADIUS, motion)
                .map(Intent::Video)
                .boxed(),
        );
    }
    let video = cell.playing.is_some();
    opener(stack(layers).width(width).height(height))
        .on_right_press(move |at| Intent::Menu {
            item,
            index,
            video,
            at: Spot::pointer(at),
        })
        .boxed()
}

/// The disc with its play triangle, centred on a video's poster.
fn play_button<'a>() -> Element<'a, Intent> {
    let glyph = icon(Icon::Play, GLYPH).style(|_theme: &Theme, _status| iced::widget::svg::Style {
        color: Some(iced::Color::WHITE),
    });
    container(
        container(glyph)
            .center(BUTTON)
            .style(|_theme: &Theme| container::Style {
                background: Some(rgba(BUTTON_GROUND).into()),
                border: iced::Border::default().rounded(BUTTON / 2.0),
                ..container::Style::default()
            }),
    )
    .center(Fill)
    .boxed()
}
