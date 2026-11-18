//! What a rail item draws besides its tile: the marks at the tile's
//! corners (the count, a call, Discord's logo), the focus ring and a
//! closed folder's four servers. Each is `ui/rail.slint`'s
//! (`RailItem`, `FolderItem`, `CallBubble`) and `ui/discord.slint`'s.
//!
//! The marks draw in a layer of their own, over the picture: within one
//! layer a picture draws above every quad, which would bury a badge's ring.

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::advanced::svg as vector;
use iced::advanced::svg::Renderer as _;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use iced::advanced::text::{
    Alignment, Ellipsis, LineHeight, Paragraph as _, Renderer as _, Shaping, Text, Wrapping,
};
use iced::border::Radius;
use iced::{Background, Border, Color, Pixels, Point, Rectangle, Size, alignment};

use anomi_theme::Palette;
use anomi_theme::{Renderer, typography};
use anomi_vm::icon::Icon;

use crate::picture::{Player, Shape};

use super::Preview;

/// Discord's blurple, whatever the accent is.
const BLURPLE: Color = Color::from_rgb(
    0x58 as f32 / 255.0,
    0x65 as f32 / 255.0,
    0xf2 as f32 / 255.0,
);

/// A closed folder's servers: a 2 by 2 grid, each this big and this far
/// apart, `4` in from the tile's edge.
const PREVIEW: f32 = 19.0;
const PREVIEW_STEP: f32 = 21.0;
const PREVIEW_INSET: f32 = 4.0;
const PREVIEW_CORNER: f32 = 4.0;
/// A preview's initial: `size * 0.42` of 19, bold and white (`Avatar`,
/// `ui/controls.slint`).
const PREVIEW_INITIAL: f32 = 8.0;
/// The grid's outer corners follow the folder's own.
const PREVIEW_OUTER: f32 = 13.0;

/// The call speaker's shadow: `CallBubble`'s `opacity: 0.45`
/// (`ui/rail.slint:113`). It goes through `Svg::opacity`, as tiny-skia's
/// colour filter drops the colour's alpha.
const SHADOW: f32 = 0.45;

/// The width a count's pill needs.
fn pill_width(count: &str) -> f32 {
    (8.0 + 7.0 * count.chars().count() as f32).max(16.0)
}

pub(super) fn label(
    content: &str,
    size: f32,
    font: iced::Font,
    width: f32,
    height: f32,
) -> Text<String> {
    Text {
        content: content.to_owned(),
        bounds: Size::new(width, height),
        size: Pixels(size),
        line_height: LineHeight::default(),
        font,
        align_x: Alignment::Center,
        align_y: alignment::Vertical::Center,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
        ellipsis: Ellipsis::None,
        hint_factor: None,
    }
}

type Paragraph = <Renderer as iced::advanced::text::Renderer>::Paragraph;

thread_local! {
    /// The sizes the labels measured, by content, size and face: a few dozen.
    static MEASURED: RefCell<HashMap<u64, Size>> = RefCell::default();
}

/// Draws a [`label`] centred on `centre`, from the top left of the size it
/// measures: the renderer damages a text's box from the point it's given,
/// so a label centred on a point that changed (a badge's count) left the
/// old one's left and top halves drawn. Measured once a content, size and
/// face.
fn centred(
    renderer: &mut Renderer,
    mut text: Text<String>,
    centre: Point,
    ink: Color,
    clip: Rectangle,
) {
    let mut hasher = DefaultHasher::new();
    (&text.content, text.size.0.to_bits(), text.font).hash(&mut hasher);
    let key = hasher.finish();
    let size = MEASURED.with_borrow_mut(|measured| {
        if let Some(size) = measured.get(&key) {
            return *size;
        }
        if measured.len() >= 256 {
            measured.clear();
        }
        let size = Paragraph::with_text(text.as_ref()).min_bounds();
        measured.insert(key, size);
        size
    });
    text.align_x = Alignment::Left;
    text.align_y = alignment::Vertical::Top;
    let at = Point::new(centre.x - size.width / 2.0, centre.y - size.height / 2.0);
    renderer.fill_text(text, at, ink, clip);
}

pub(super) fn draw_initials(renderer: &mut Renderer, initials: &str, tile: Rectangle, ink: Color) {
    let text = label(
        initials,
        typography::size::RAIL_INITIALS,
        typography::MEDIUM,
        tile.width,
        tile.height,
    );
    centred(renderer, text, tile.center(), ink, tile);
}

fn disc(renderer: &mut Renderer, bounds: Rectangle, colour: Color) {
    renderer.fill_quad(
        Quad {
            bounds,
            border: Border::default().rounded(bounds.height / 2.0),
            ..Quad::default()
        },
        Background::Color(colour),
    );
}

/// A disc of `side` with a ring of `ring` around it, `grow` wide each side.
fn ringed(renderer: &mut Renderer, inner: Rectangle, grow: f32, ring: Color, fill: Color) {
    disc(renderer, inner.expand(grow), ring);
    disc(renderer, inner, fill);
}

/// The count at the tile's bottom right, on a ring of the rail's colour.
pub(super) fn count(
    renderer: &mut Renderer,
    count: &str,
    tile: Rectangle,
    palette: &Palette,
    fade: f32,
) {
    let height = 16.0;
    let width = pill_width(count);
    let inner = Rectangle::new(
        Point::new(
            tile.x + tile.width + 1.0 - width,
            tile.y + tile.height + 1.0 - height,
        ),
        Size::new(width, height),
    );
    ringed(
        renderer,
        inner,
        3.0,
        fading(palette.rail, fade),
        fading(palette.danger, fade),
    );
    let text = label(
        count,
        typography::size::BADGE,
        typography::BOLD,
        inner.width,
        inner.height,
    );
    centred(
        renderer,
        text,
        inner.center(),
        fading(palette.on_danger, fade),
        inner.expand(3.0),
    );
}

/// A call going on: a white speaker on green, a pixel of dark under it, at
/// the tile's top right (`CallBubble`).
pub(super) fn call(
    renderer: &mut Renderer,
    tile: Rectangle,
    palette: &Palette,
    fade: f32,
    viewport: &Rectangle,
) {
    let outer = Rectangle::new(
        Point::new(tile.x + tile.width + 3.0 - 20.0, tile.y - 3.0),
        Size::new(20.0, 20.0),
    );
    ringed(
        renderer,
        outer.shrink(2.0),
        2.0,
        fading(palette.rail, fade),
        fading(palette.live, fade),
    );
    let side = 12.0;
    let at = outer.center() - iced::Vector::new(side / 2.0, side / 2.0);
    let icon = |at: Point, ink: Color, opacity: f32| {
        let mut svg = vector::Svg::new(Icon::VolumeBold.handle());
        svg.color = Some(ink);
        svg.opacity = opacity;
        (svg, Rectangle::new(at, Size::new(side, side)))
    };
    let (shadow, shadow_at) = icon(
        at + iced::Vector::new(0.0, 1.0),
        Color::BLACK,
        SHADOW * fade,
    );
    renderer.draw_svg(shadow, shadow_at, *viewport);
    let (speaker, speaker_at) = icon(at, Color::WHITE, fade);
    renderer.draw_svg(speaker, speaker_at, *viewport);
}

/// Discord's logo, white on blurple, at the tile's top left on a ring of
/// the rail's colour (`DiscordBadge`, 16 and a ring of 2).
pub(super) fn discord(
    renderer: &mut Renderer,
    tile: Rectangle,
    palette: &Palette,
    fade: f32,
    viewport: &Rectangle,
) {
    let size = 16.0;
    let outer = Rectangle::new(
        Point::new(tile.x - 2.0, tile.y - 2.0),
        Size::new(size + 4.0, size + 4.0),
    );
    ringed(
        renderer,
        outer.shrink(2.0),
        2.0,
        fading(palette.rail, fade),
        fading(BLURPLE, fade),
    );
    let side = size * 0.62;
    let at = outer.center() - iced::Vector::new(side / 2.0, side / 2.0);
    let mut logo = vector::Svg::new(Icon::Discord.handle());
    logo.color = Some(Color::WHITE);
    logo.opacity = fade;
    renderer.draw_svg(logo, Rectangle::new(at, Size::new(side, side)), *viewport);
}

/// The focus ring over the tile, in the text's colour.
pub(super) fn ring(
    renderer: &mut Renderer,
    tile: Rectangle,
    radius: f32,
    colour: Color,
    fade: f32,
) {
    renderer.fill_quad(
        Quad {
            bounds: tile,
            border: Border {
                color: fading(colour, fade),
                width: 2.0,
                radius: radius.into(),
            },
            ..Quad::default()
        },
        Background::Color(Color::TRANSPARENT),
    );
}

/// The corners of the preview at `index`: small, but for the one that
/// meets the folder's own corner.
fn corners(index: usize) -> Radius {
    let mut radius = Radius::from(PREVIEW_CORNER);
    match index {
        0 => radius.top_left = PREVIEW_OUTER,
        1 => radius.top_right = PREVIEW_OUTER,
        2 => radius.bottom_left = PREVIEW_OUTER,
        _ => radius.bottom_right = PREVIEW_OUTER,
    }
    radius
}

/// Where the preview at `index` goes in a folder's `tile`.
pub(super) fn preview_bounds(tile: Rectangle, index: usize) -> Rectangle {
    Rectangle::new(
        Point::new(
            tile.x + PREVIEW_INSET + (index % 2) as f32 * PREVIEW_STEP,
            tile.y + PREVIEW_INSET + (index / 2) as f32 * PREVIEW_STEP,
        ),
        Size::new(PREVIEW, PREVIEW),
    )
}

/// A closed folder's first four servers, a picture or the initial on the
/// raised ground.
pub(super) fn previews(
    renderer: &mut Renderer,
    previews: &[Preview<'_>],
    players: &[Player],
    tile: Rectangle,
    palette: &Palette,
    fade: f32,
    viewport: &Rectangle,
) {
    for (index, preview) in previews.iter().take(4).enumerate() {
        let bounds = preview_bounds(tile, index);
        let radius = corners(index);
        match (&preview.picture, players.get(index)) {
            (Some(handle), Some(player)) => {
                let shape = Shape {
                    radius,
                    ..Shape::default()
                };
                player.draw(renderer, handle, bounds, shape, fade, viewport);
            }
            _ => {
                renderer.fill_quad(
                    Quad {
                        bounds,
                        border: Border {
                            radius,
                            ..Border::default()
                        },
                        ..Quad::default()
                    },
                    Background::Color(fading(palette.raised, fade)),
                );
                let text = label(
                    preview.initial,
                    PREVIEW_INITIAL,
                    typography::BOLD,
                    bounds.width,
                    bounds.height,
                );
                centred(
                    renderer,
                    text,
                    bounds.center(),
                    fading(Color::WHITE, fade),
                    bounds,
                );
            }
        }
    }
}

/// `colour` at `fade` of its opacity.
pub(super) fn fading(colour: Color, fade: f32) -> Color {
    Color {
        a: colour.a * fade,
        ..colour
    }
}

/// `from` at 0, `to` at 1.
pub(super) fn blend(from: Color, to: Color, amount: f32) -> Color {
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    Color {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
}
