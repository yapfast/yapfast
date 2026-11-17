//! One choice's look: `MenuItem` (`ui/controls.slint:1156`), measured and
//! painted for the menus that draw their rows themselves ([`super::item`],
//! [`super::ContextMenu`]). They keep a row's measure while its [`key`]
//! holds, so text is shaped when it changes and not at each layout or draw.
//!
//! A row is 32 px tall, 44 px with a second line; 8 px inside at its sides
//! with 8 px between its parts: an icon 16 px wide (or the check of the
//! current choice, or room for one unless the row is plain), its text and
//! subtitle, a box to tick, an arrow to its submenu. Under the pointer or
//! the keys it fills with the overlay at 9%, or red for a destructive one,
//! whose text and icon turn white.

use std::hash::{DefaultHasher, Hash, Hasher};

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::advanced::svg::{self as vector, Renderer as _};
use iced::{Border, Color, Font, Point, Radians, Rectangle, Size};

use anomi_theme::{Renderer, Theme, typography};
use anomi_vm::menu::Row;

use super::text;
use crate::icon::Icon;

/// A row's height, and with a second line (`:1185`).
pub(crate) const HEIGHT: f32 = 32.0;
pub(crate) const TALL: f32 = 44.0;
/// A wrapping second line's room above and below (`:1185`).
const WRAP_PAD: f32 = 12.0;
/// Inside the row at its sides, and between its parts (`:1201`).
const PAD: f32 = 8.0;
const SPACING: f32 = 8.0;
/// The icon's width (`:1205`), the box's side and its tick (`:1226`, `:1233`),
/// the arrow's width (`:1240`).
const ICON: f32 = 16.0;
const BOX: f32 = 18.0;
const TICK: f32 = 12.0;
const ARROW: f32 = 24.0;
/// The row's corners and the box's (`:1186`, `:1226`), the box's edge (`:1231`).
pub(crate) const RADIUS: f32 = 4.0;
const BOX_EDGE: f32 = 1.5;
/// The overlay's alpha under the pointer (`:1189`).
const HIGHLIGHT: f32 = 0.09;
/// What a disabled row's parts keep of their colour (`:1202`).
const DISABLED: f32 = 0.45;
/// The keyboard focus's ring (`:1190`).
const RING: f32 = 2.0;
const TEXT: f32 = typography::size::SMALL;
const SUBTITLE: f32 = typography::size::CAPTION;

/// The row's icon, kept with its measure.
pub(crate) fn icon<M>(row: &Row<M>) -> Option<Icon> {
    row.icon
}

/// What measuring a row found that drawing it needs: its subtitle's height,
/// and its icon.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Shape {
    pub below: f32,
    pub icon: Option<Icon>,
}

/// How a row's text looks: a menu's, or a list's at the body's size,
/// regular, in the secondary ink, as the account popover's status row
/// (`ui/yapfast/account-popover.slint:122`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Look {
    #[default]
    Menu,
    Body,
}

/// The text's size and face in a look.
fn style<M>(row: &Row<M>, look: Look) -> (f32, Font) {
    match look {
        Look::Menu => (TEXT, face(row)),
        Look::Body => (typography::size::BODY, typography::UI),
    }
}

/// The text's face: bold for the current choice (`:1214`).
fn face<M>(row: &Row<M>) -> Font {
    if row.current() {
        typography::BOLD
    } else {
        typography::MEDIUM
    }
}

/// Whether the row keeps room for an icon (`:1204`).
fn slot<M>(row: &Row<M>) -> f32 {
    if !row.plain || row.current() || row.icon.is_some() {
        ICON + SPACING
    } else {
        0.0
    }
}

/// What follows the text: the box, the arrow.
fn ends<M>(row: &Row<M>, arrow: bool) -> f32 {
    let mut width = 0.0;
    if row.checkbox {
        width += SPACING + BOX;
    }
    if arrow {
        width += SPACING + ARROW;
    }
    width
}

/// The width the row wants: as wide as its text, or its subtitle when that
/// is wider and doesn't wrap.
pub(crate) fn width<M>(row: &Row<M>, arrow: bool, look: Look) -> f32 {
    let (size, font) = style(row, look);
    let mut lines = text::width(&row.text, size, font);
    if !row.wraps {
        lines = lines.max(text::width(&row.subtitle, SUBTITLE, typography::UI));
    }
    PAD + slot(row) + lines + ends(row, arrow) + PAD
}

/// The width left for the text at a row's width.
fn column<M>(row: &Row<M>, arrow: bool, width: f32) -> f32 {
    width - 2.0 * PAD - slot(row) - ends(row, arrow)
}

/// The subtitle's height at a row's width: one line, or as many as a
/// wrapping one takes.
pub(crate) fn below<M>(row: &Row<M>, arrow: bool, width: f32) -> f32 {
    if row.subtitle.is_empty() {
        0.0
    } else if row.wraps {
        let column = column(row, arrow, width);
        text::wrapped_height(&row.subtitle, SUBTITLE, typography::UI, column)
    } else {
        text::line_height(SUBTITLE)
    }
}

/// The row's height with its subtitle's: a wrapping one may make it taller.
pub(crate) fn height<M>(row: &Row<M>, below: f32) -> f32 {
    if row.subtitle.is_empty() {
        HEIGHT
    } else if row.wraps {
        TALL.max(text::line_height(TEXT) + below + WRAP_PAD)
    } else {
        TALL
    }
}

/// Feeds `hasher` what a row's measure depends on.
pub(crate) fn hash<M>(row: &Row<M>, hasher: &mut impl Hasher) {
    row.text.hash(hasher);
    row.subtitle.hash(hasher);
    // The name, not only whether there is one: the measure keeps the icon.
    row.icon.hash(hasher);
    let flags = (
        row.checkbox,
        row.checked,
        row.plain,
        row.wraps,
        row.separated,
    );
    flags.hash(hasher);
}

/// What a row's measure depends on, with its arrow, hashed.
pub(crate) fn key<M>(row: &Row<M>, arrow: bool, look: Look) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash(row, &mut hasher);
    arrow.hash(&mut hasher);
    look.hash(&mut hasher);
    hasher.finish()
}

/// How a row is drawn this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Paint {
    /// Under the pointer or the keys.
    pub lit: bool,
    pub arrow: bool,
    /// How far the arrow has turned down, from 0 to 1 (`expanded`).
    pub turned: f32,
    /// How far the box has filled as it's ticked, from 0 to 1.
    pub ticked: f32,
    /// The keyboard's focus ring.
    pub focused: bool,
    /// The whole row's alpha, as a submenu fades in.
    pub alpha: f32,
    pub look: Look,
}

/// Draws a row in `bounds` with its measured [`Shape`]. `clip` is what its
/// icons may show in; its text is given the row's own bounds, so the layer
/// it's drawn in (the card's, a scrollable's) cuts what lies outside it:
/// tiny-skia masks cached text by the layer only when the text's own clip
/// crosses the layer's edge.
pub(crate) fn draw<M>(
    renderer: &mut Renderer,
    theme: &Theme,
    row: &Row<M>,
    (bounds, shape): (Rectangle, Shape),
    paint: Paint,
    clip: Rectangle,
) {
    let Shape { below, icon } = shape;
    let palette = theme.palette();
    let ground = if !paint.lit {
        Color::TRANSPARENT
    } else if row.danger {
        palette.danger
    } else {
        palette.lift(HIGHLIGHT)
    };
    let ring = if paint.focused { RING } else { 0.0 };
    if paint.lit || paint.focused {
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default()
                    .rounded(RADIUS)
                    .width(ring)
                    .color(text::faded(palette.accent, paint.alpha)),
                ..Quad::default()
            },
            text::faded(ground, paint.alpha),
        );
    }
    let alpha = paint.alpha * if row.disabled { DISABLED } else { 1.0 };
    let fade = |color: Color| text::faded(color, alpha);
    let warm = |lit: Color, plain: Color| if paint.lit { lit } else { plain };
    let ink = if row.danger {
        warm(palette.on_danger, palette.danger_text())
    } else if paint.look == Look::Body {
        palette.secondary
    } else {
        palette.text
    };

    let mut x = bounds.x + PAD;
    let middle = bounds.center_y();
    if slot(row) > 0.0 {
        let (glyph, tint) = if row.current() {
            (Some(Icon::Check), palette.accent)
        } else if row.danger {
            (icon, ink)
        } else {
            (icon, warm(palette.text, palette.secondary))
        };
        if let Some(glyph) = glyph {
            let place = Rectangle::new(Point::new(x, middle - ICON / 2.0), Size::new(ICON, ICON));
            svg(renderer, glyph, place, fade(tint), 0.0, clip);
        }
        x += ICON + SPACING;
    }

    let column = column(row, paint.arrow, bounds.width);
    let (size, font) = style(row, paint.look);
    let line = text::line_height(size);
    let top = middle - (line + below) / 2.0;
    text::line(
        renderer,
        &row.text,
        (size, font),
        Point::new(x, top),
        column,
        fade(ink),
        bounds,
    );
    if !row.subtitle.is_empty() {
        let at = Point::new(x, top + line);
        let style = (SUBTITLE, typography::UI);
        let muted = fade(palette.muted);
        if row.wraps {
            text::wrapped(renderer, &row.subtitle, style, at, column, muted, bounds);
        } else {
            text::line(renderer, &row.subtitle, style, at, column, muted, bounds);
        }
    }

    let mut end = bounds.x + bounds.width - PAD;
    if paint.arrow {
        end -= ARROW;
        let tint = if row.danger && paint.lit {
            palette.on_danger
        } else {
            palette.text
        };
        let place = Rectangle::new(
            Point::new(end, middle - ARROW / 2.0),
            Size::new(ARROW, ARROW),
        );
        // Turned fully, tiny-skia draws the svg at no width: the arrow
        // pointing down is its own glyph.
        let (glyph, turn) = if paint.turned >= 1.0 {
            (Icon::ChevronDown, 0.0)
        } else {
            (
                Icon::ChevronRight,
                paint.turned * std::f32::consts::FRAC_PI_2,
            )
        };
        svg(renderer, glyph, place, fade(tint), turn, clip);
        end -= SPACING;
    }
    if row.checkbox {
        let place = Rectangle::new(
            Point::new(end - BOX, middle - BOX / 2.0),
            Size::new(BOX, BOX),
        );
        let edge = if row.checked {
            palette.accent
        } else {
            warm(palette.text, palette.muted)
        };
        renderer.fill_quad(
            Quad {
                bounds: place,
                border: Border::default()
                    .rounded(RADIUS)
                    .width(BOX_EDGE)
                    .color(fade(edge)),
                ..Quad::default()
            },
            fade(palette.accent.scale_alpha(paint.ticked)),
        );
        if row.checked {
            let tick = Rectangle::new(
                Point::new(place.center_x() - TICK / 2.0, place.center_y() - TICK / 2.0),
                Size::new(TICK, TICK),
            );
            svg(
                renderer,
                Icon::Check,
                tick,
                fade(palette.on_accent),
                0.0,
                clip,
            );
        }
    }
}

fn svg(
    renderer: &mut Renderer,
    icon: Icon,
    bounds: Rectangle,
    tint: Color,
    turn: f32,
    clip: Rectangle,
) {
    let mut glyph = vector::Svg::new(icon.handle());
    glyph.color = Some(tint);
    glyph.rotation = Radians(turn);
    renderer.draw_svg(glyph, bounds, clip);
}
