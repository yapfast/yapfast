//! Anomi: a frame drawn from its damage, over the frame before it as the
//! window's buffer holds it, lays the same pixels as the frame drawn whole.
//! Each case below left pixels as the frame before had them, or laid them
//! twice, before its fix (`src/reach.rs`, `engine::unmasked`,
//! `engine::touches`).

use iced_tiny_skia::Renderer;
use iced_tiny_skia::core::alignment;
use iced_tiny_skia::core::renderer::{self, Quad, Renderer as _};
use iced_tiny_skia::core::text::{self, Renderer as _};
use iced_tiny_skia::core::{Border, Color, Pixels, Point, Rectangle, Size};
use iced_tiny_skia::graphics::Viewport;
use iced_tiny_skia::window::compositor;
use iced_tiny_skia::window::offscreen::Offscreen;

const BACKGROUND: Color = Color::from_rgb(0.1, 0.1, 0.12);
const WINDOW: Size = Size::new(120.0, 80.0);
/// How far a pixel may be off the frame drawn whole: a shape drawn through
/// the clip mask comes out up to three levels off the same shape drawn
/// without it (tiny-skia's pipelines round apart), as a region's edge
/// crossing it makes it; a pixel left from the frame before is further off.
const LEVELS: u8 = 3;

/// Draws `frames` frames, each by `draw`, into one buffer that keeps the
/// last, redrawing what its damage says, and checks each against the frame
/// drawn whole, within `LEVELS`. Returns how many pixels of the last whole frame aren't the
/// background, so a case can tell it drew something.
fn each_frame_as_drawn_whole(
    scale: f32,
    frames: usize,
    draw: impl Fn(&mut Renderer, usize),
) -> usize {
    let physical = Size::new(
        (WINDOW.width * scale).round() as u32,
        (WINDOW.height * scale).round() as u32,
    );
    let viewport = Viewport::with_physical_size(
        physical,
        renderer::Scale {
            window: scale,
            ..renderer::Scale::default()
        },
    );
    let mut renderer = Renderer::new(renderer::Settings::default());
    let mut buffer = Offscreen::new(physical.width, physical.height, 1);
    let mut shown = 0;
    for frame in 0..frames {
        renderer.reset(Rectangle::with_size(WINDOW));
        draw(&mut renderer, frame);
        let _ = buffer.draw(&mut renderer, &viewport, BACKGROUND);
        let whole = compositor::screenshot(&mut renderer, &viewport, BACKGROUND);
        let held = buffer.rgba();
        let wrong: Vec<_> = whole
            .chunks(4)
            .zip(held.chunks(4))
            .enumerate()
            .filter(|(_, (a, b))| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > LEVELS))
            .map(|(at, (a, b))| {
                (
                    at % physical.width as usize,
                    at / physical.width as usize,
                    a[..3].to_vec(),
                    b[..3].to_vec(),
                )
            })
            .take(4)
            .collect();
        assert!(
            wrong.is_empty(),
            "frame {frame} at scale {scale}: pixels {wrong:?} differ"
        );
        let ground = whole[..4].to_vec();
        shown = whole.chunks(4).filter(|pixel| *pixel != ground).count();
    }
    shown
}

fn quad(bounds: Rectangle, radius: f32, color: Color, renderer: &mut Renderer) {
    renderer.fill_quad(
        Quad {
            bounds,
            border: Border::default().rounded(radius),
            ..Quad::default()
        },
        color,
    );
}

/// A square changing colour above a layer, its damage ending on y = 23.5,
/// and in the layer, from 23.4, a box from 23.5: the region leaves the
/// layer the pixel row 23, which it clears, and the box covers half of it.
/// Taken as clear of the region by its rectangle, the box wasn't drawn
/// there, and the row stayed the ground's colour.
#[test]
fn a_box_touching_a_region_by_half_a_pixel_is_drawn_there() {
    for scale in [1.0, 1.25] {
        let shown = each_frame_as_drawn_whole(scale, 4, |renderer, frame| {
            let square = if frame % 2 == 0 {
                Color::from_rgb(0.6, 0.2, 0.2)
            } else {
                Color::from_rgb(0.2, 0.6, 0.2)
            };
            let top = 23.5 / scale;
            quad(
                Rectangle::new(Point::new(10.0, 5.0), Size::new(50.0, top - 6.0)),
                0.0,
                square,
                renderer,
            );
            let layer =
                Rectangle::new(Point::new(0.0, 23.4 / scale), Size::new(WINDOW.width, 40.0));
            renderer.with_layer(layer, |renderer| {
                let ground = Color::from_rgb(0.5, 0.5, 0.55);
                quad(
                    Rectangle::new(Point::new(5.0, top), Size::new(100.0, 20.0)),
                    4.0,
                    ground,
                    renderer,
                );
            });
        });
        assert!(shown > 0);
    }
}

/// A row's ground changing colour under a 3 px dot: the region its damage
/// starts half a pixel above the ground, where the dot starts too, and is
/// cleared from the pixel below. The dot, within the region but for that
/// pixel's half, was laid unmasked on it again each frame, a level lighter.
#[test]
fn a_dot_on_the_edge_of_a_region_is_laid_once() {
    let shown = each_frame_as_drawn_whole(1.0, 6, |renderer, frame| {
        let ground = if frame % 2 == 0 {
            Color::from_rgb(0.2, 0.2, 0.25)
        } else {
            Color::from_rgb(0.25, 0.2, 0.2)
        };
        quad(
            Rectangle::new(Point::new(30.0, 24.5), Size::new(50.0, 20.0)),
            0.0,
            ground,
            renderer,
        );
        let dot = Color::from_rgba(0.8, 0.8, 0.9, 0.75);
        quad(
            Rectangle::new(Point::new(40.0, 23.5), Size::new(3.0, 3.0)),
            1.5,
            dot,
            renderer,
        );
    });
    assert!(shown > 0);
}

/// Cached text, as menus and names draw it, size 20 in a line 6 px tall:
/// its glyph passes its bounds by several pixels each way, as an emoji
/// passes its line. Moved down a few pixels a frame, the glyph's top rows
/// stayed where it had been, and at the left of a centred line the half
/// stock took for outside it.
#[test]
fn glyphs_past_their_line_go_with_it() {
    for (scale, align_x) in [
        (1.0, text::Alignment::Left),
        (2.0, text::Alignment::Left),
        (1.0, text::Alignment::Center),
    ] {
        let shown = each_frame_as_drawn_whole(scale, 5, |renderer, frame| {
            let line = icons(text::LineHeight::Absolute(Pixels(6.0)), align_x);
            let at = Point::new(40.0, 20.0 + 3.0 * frame as f32);
            renderer.fill_text(line, at, Color::WHITE, Rectangle::with_size(WINDOW));
        });
        assert!(shown > 0, "at scale {scale}, {align_x:?}: the glyphs drew");
    }
}

/// A line inside a layer, its glyphs passing the layer's lower edge, and a
/// region crossing it as a square beside it changes. Within its clip, the
/// line's glyphs are cut by the region alone in the frame drawn whole;
/// stock asked whether it was within its clip cut by the region, so there
/// they were cut at the layer's edge.
#[test]
fn a_region_across_a_line_cuts_it_as_the_whole_frame_does() {
    let shown = each_frame_as_drawn_whole(1.0, 4, |renderer, frame| {
        let square = if frame % 2 == 0 {
            Color::from_rgb(0.6, 0.2, 0.2)
        } else {
            Color::from_rgb(0.2, 0.6, 0.2)
        };
        quad(
            Rectangle::new(Point::new(50.0, 30.0), Size::new(10.0, 20.0)),
            0.0,
            square,
            renderer,
        );
        renderer.with_layer(
            Rectangle::with_size(Size::new(WINDOW.width, 40.0)),
            |renderer| {
                let line = icons(
                    text::LineHeight::Absolute(Pixels(6.0)),
                    text::Alignment::Left,
                );
                renderer.fill_text(
                    line,
                    Point::new(20.0, 33.0),
                    Color::WHITE,
                    Rectangle::with_size(WINDOW),
                );
            },
        );
    });
    assert!(shown > 0);
}

/// Four checkmarks of iced's own icon font, which the renderer always has.
fn icons(line_height: text::LineHeight, align_x: text::Alignment) -> text::Text {
    text::Text {
        content: "\u{f00c}\u{f00c}\u{f00c}\u{f00c}".into(),
        bounds: Size::new(90.0, 6.0),
        size: Pixels(20.0),
        line_height,
        font: <Renderer as text::Renderer>::ICON_FONT,
        align_x,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::Basic,
        wrapping: text::Wrapping::None,
        ellipsis: text::Ellipsis::default(),
        hint_factor: None,
    }
}
