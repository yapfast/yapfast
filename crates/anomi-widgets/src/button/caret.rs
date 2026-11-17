//! A turning chevron drawn as a path. The renderer at this pin can't draw a
//! rotated svg (`tiny_skia/src/vector.rs` sizes the raster by the transform's
//! `sx`, which is 0 at a quarter turn and -1 at a half), so the menu caret
//! strokes Lucide's `chevron-down` itself and turns the path; so do the
//! rows and headers that turn one (`anomi_widgets::button::chevron`).

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::renderer::Renderer as _;
use iced::widget::canvas::{Frame, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Point, Radians, Size, Vector};

use anomi_theme::Renderer;

/// The icon's own box, in which its path is given (`ui/icons/chevron-down.svg`).
const BOX: f32 = 24.0;
/// Its stroke, in that box (the frame scales the path, not the width).
const STROKE: f32 = 2.0;

/// The chevron in a `side` square at `at`, pointing down turned by
/// `angle` (clockwise), in `ink`. A menu's caret turns by a half turn as it
/// opens, a folding row's by a quarter the other way.
pub fn chevron(renderer: &mut Renderer, at: Point, side: f32, angle: Radians, ink: Color) {
    let mut frame = Frame::new(renderer, Size::new(side, side));
    frame.translate(Vector::new(side / 2.0, side / 2.0));
    frame.rotate(angle);
    frame.scale(side / BOX);
    frame.translate(Vector::new(-BOX / 2.0, -BOX / 2.0));
    // `m6 9 6 6 6-6`.
    let path = Path::new(|path| {
        path.move_to(Point::new(6.0, 9.0));
        path.line_to(Point::new(12.0, 15.0));
        path.line_to(Point::new(18.0, 9.0));
    });
    frame.stroke(
        &path,
        Stroke::default()
            .with_color(ink)
            .with_width(STROKE * side / BOX)
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round),
    );
    let geometry = frame.into_geometry();
    renderer.with_translation(at - Point::ORIGIN, |renderer| {
        renderer.draw_geometry(geometry)
    });
}
