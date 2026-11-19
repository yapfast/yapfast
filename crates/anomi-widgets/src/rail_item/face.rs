//! What fills a rail item's tile, and how a dragged server changes it: the
//! copy that follows the pointer is lifted off the rail (larger, over a
//! shadow), a server something is dragged onto shows a folder forming (a
//! folder's frame grows around it and its tile shrinks into the folder's
//! first place, the next one waiting), and a closed folder shows it takes
//! it in (it grows, ringed in its colour).

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::advanced::svg as vector;
use iced::advanced::svg::Renderer as _;
use iced::border::Radius;
use iced::{Background, Border, Color, Point, Rectangle, Shadow, Size, Vector};

use anomi_theme::{Palette, Renderer, metrics, rgba};
use anomi_vm::rail::FOLDER_COLOR;

use super::{FOLDER_RADIUS, Face, RailItem, paint};
use crate::picture::{Player, Shape};

/// The shadow it casts on the rail.
const LIFT_SHADOW: Shadow = Shadow {
    color: rgba(0x0000_0080),
    offset: Vector::new(0.0, 4.0),
    blur_radius: 14.0,
};
/// A closed folder taking a server grows this much each side.
const TAKE_GROWTH: f32 = 3.0;
/// The ring around it, in its colour.
const TAKE_RING: f32 = 2.0;
/// A forming folder's place for the server dragged in is the raised colour
/// this strong.
const WAITING: f32 = 0.6;

/// Where an item's tile is drawn: `lift` times as large while carried, or
/// grown while a closed folder takes a server (`aim` from 0 to 1).
pub(super) fn tile<Message>(
    item: &RailItem<'_, Message>,
    tile: Rectangle,
    lift: f32,
    aim: f32,
) -> Rectangle {
    let tile = scaled(tile, lift);
    if item.folder() {
        tile.expand(TAKE_GROWTH * aim)
    } else {
        tile
    }
}

fn scaled(tile: Rectangle, by: f32) -> Rectangle {
    let size = Size::new(tile.width * by, tile.height * by);
    Rectangle::new(
        tile.center() - Vector::new(size.width / 2.0, size.height / 2.0),
        size,
    )
}

/// The shadow a lifted tile casts.
pub(super) fn shadow(renderer: &mut Renderer, tile: Rectangle, radius: f32, palette: &Palette) {
    renderer.fill_quad(
        Quad {
            bounds: tile,
            border: Border::default().rounded(radius),
            shadow: LIFT_SHADOW,
            ..Quad::default()
        },
        Background::Color(palette.raised),
    );
}

/// What draws the face: the tile, its corners and how lit it is.
pub(super) struct Look<'p> {
    pub tile: Rectangle,
    pub radius: f32,
    pub lit: f32,
    pub fade: f32,
    pub palette: &'p Palette,
    pub viewport: &'p Rectangle,
}

/// A server's tile with something dragged onto it, `aim` of the way to a
/// folder: the folder's frame grows around it, it shrinks into the first
/// place and the second waits for the server dragged in.
pub(super) fn forming<Message>(
    renderer: &mut Renderer,
    item: &RailItem<'_, Message>,
    players: &[Player],
    look: Look<'_>,
    aim: f32,
) {
    let side = metrics::RAIL_TILE + (metrics::FOLDER_TILE - metrics::RAIL_TILE) * aim;
    let frame = scaled(look.tile, side / look.tile.width);
    let corner = metrics::RADIUS_TILE + (FOLDER_RADIUS - metrics::RADIUS_TILE) * aim;
    let tint = Color {
        a: 0.4 * aim,
        ..FOLDER_COLOR
    };
    renderer.fill_quad(
        Quad {
            bounds: frame,
            border: Border::default().rounded(corner),
            ..Quad::default()
        },
        Background::Color(paint::fading(tint, look.fade)),
    );
    let first = paint::preview_bounds(frame, 0);
    let inner = mix(look.tile, first, aim);
    let radius = look.radius + (paint::PREVIEW_CORNER - look.radius) * aim;
    let second = paint::preview_bounds(frame, 1);
    renderer.fill_quad(
        Quad {
            bounds: second,
            border: Border::default().rounded(paint::PREVIEW_CORNER),
            ..Quad::default()
        },
        Background::Color(paint::fading(
            look.palette.raised,
            WAITING * aim * look.fade,
        )),
    );
    draw(
        renderer,
        item,
        players,
        Look {
            tile: inner,
            radius,
            ..look
        },
    );
}

/// `from` at 0, `to` at 1.
fn mix(from: Rectangle, to: Rectangle, amount: f32) -> Rectangle {
    let lerp = |a: f32, b: f32| a + (b - a) * amount;
    Rectangle::new(
        Point::new(lerp(from.x, to.x), lerp(from.y, to.y)),
        Size::new(lerp(from.width, to.width), lerp(from.height, to.height)),
    )
}

/// The ring of a closed folder taking a server, `aim` of the way.
pub(super) fn taking(renderer: &mut Renderer, tile: Rectangle, tint: Color, aim: f32, fade: f32) {
    renderer.fill_quad(
        Quad {
            bounds: tile,
            border: Border {
                color: paint::fading(Color { a: aim, ..tint }, fade),
                width: TAKE_RING,
                radius: (FOLDER_RADIUS + TAKE_GROWTH * aim).into(),
            },
            ..Quad::default()
        },
        Background::Color(Color::TRANSPARENT),
    );
}

/// The face in `look.tile`: a picture or initials on its ground, a symbol,
/// or a folder.
pub(super) fn draw<Message>(
    renderer: &mut Renderer,
    item: &RailItem<'_, Message>,
    players: &[Player],
    look: Look<'_>,
) {
    let Look {
        tile,
        radius,
        lit,
        fade,
        palette,
        viewport,
    } = look;
    let ground = |renderer: &mut Renderer, colour: Color| {
        renderer.fill_quad(
            Quad {
                bounds: tile,
                border: Border::default().rounded(radius),
                ..Quad::default()
            },
            Background::Color(paint::fading(colour, fade)),
        );
    };
    match &item.face {
        Face::Picture(handle) => {
            // Slint puts the raised ground under a picture.
            ground(renderer, palette.raised);
            let shape = Shape {
                radius: Radius::from(radius),
                ..Shape::default()
            };
            if let Some(player) = players.first() {
                player.draw(renderer, handle, tile, shape, fade, viewport);
            }
        }
        Face::Folder {
            open,
            tint,
            previews,
        } => {
            if *open {
                let side = metrics::ICON_HEADER;
                let at = tile.center() - Vector::new(side / 2.0, side / 2.0);
                let mut icon = vector::Svg::new(anomi_vm::icon::Icon::Folder.handle());
                icon.color = Some(Color { a: 1.0, ..*tint });
                icon.opacity = fade;
                renderer.draw_svg(icon, Rectangle::new(at, Size::new(side, side)), *viewport);
            } else {
                let alpha = 0.4 + 0.15 * lit;
                ground(renderer, Color { a: alpha, ..*tint });
                paint::previews(renderer, previews, players, tile, palette, fade, viewport);
            }
        }
        face => {
            ground(renderer, paint::blend(palette.raised, palette.accent, lit));
            let rest = if item.accent_icon {
                palette.accent
            } else {
                palette.text
            };
            let ink = paint::fading(paint::blend(rest, palette.on_accent, lit), fade);
            if let Face::Initials(initials) = face {
                paint::draw_initials(renderer, initials, tile, ink);
            } else if let Face::Icon(handle) = face {
                let side = metrics::ICON_HEADER;
                let at = tile.center() - Vector::new(side / 2.0, side / 2.0);
                let mut icon = vector::Svg::new(handle.clone());
                icon.color = Some(Color { a: 1.0, ..ink });
                icon.opacity = fade;
                renderer.draw_svg(icon, Rectangle::new(at, Size::new(side, side)), *viewport);
            }
        }
    }
}
