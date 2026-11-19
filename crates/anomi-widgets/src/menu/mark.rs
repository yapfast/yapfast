//! The mark at a row's end: a box to tick (`MenuItem`'s, `ui/controls.slint:1226`),
//! filling with the accent and showing a tick as it's ticked, or the ring
//! of one choice of a set, as Discord's radio rows have, filling with the
//! accent around a dot while it's the one chosen. Either fills over the
//! row's `ticked`.

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::{Border, Point, Rectangle, Size};

use anomi_theme::{Renderer, Theme};
use anomi_vm::menu::Row;

use super::row::{self, Paint, RADIUS};
use super::text::faded;
use crate::icon::Icon;

/// The mark's side, the box's tick, the radio's dot, and their edge
/// (`:1226`, `:1233`, `:1231`).
pub(crate) const SIDE: f32 = 18.0;
const TICK: f32 = 12.0;
const DOT: f32 = 8.0;
const EDGE: f32 = 1.5;

/// Draws the row's mark in `place`, at `alpha` (the row's, a disabled one's
/// faded), its glyph within `clip`.
pub(crate) fn draw<M>(
    renderer: &mut Renderer,
    theme: &Theme,
    row: &Row<M>,
    place: Rectangle,
    paint: Paint,
    (alpha, clip): (f32, Rectangle),
) {
    let palette = theme.palette();
    let edge = if row.checked {
        palette.accent
    } else if paint.lit {
        palette.text
    } else {
        palette.muted
    };
    let corner = if row.radio { SIDE / 2.0 } else { RADIUS };
    renderer.fill_quad(
        Quad {
            bounds: place,
            border: Border::default()
                .rounded(corner)
                .width(EDGE)
                .color(faded(edge, alpha)),
            ..Quad::default()
        },
        faded(palette.accent.scale_alpha(paint.ticked), alpha),
    );
    if !row.checked {
        return;
    }
    let centre = |side: f32| {
        Rectangle::new(
            Point::new(place.center_x() - side / 2.0, place.center_y() - side / 2.0),
            Size::new(side, side),
        )
    };
    if row.radio {
        renderer.fill_quad(
            Quad {
                bounds: centre(DOT),
                border: Border::default().rounded(DOT / 2.0),
                ..Quad::default()
            },
            faded(palette.on_accent.scale_alpha(paint.ticked), alpha),
        );
    } else {
        let tint = faded(palette.on_accent, alpha);
        row::svg(renderer, Icon::Check, centre(TICK), tint, 0.0, clip);
    }
}
