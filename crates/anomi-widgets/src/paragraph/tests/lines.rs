//! Lines under and through a span lie on the device's pixels, as Slint's
//! do: whole rows, each fully covered, none smoothed half into the rows
//! above or below; at 1x and 2x, at the message size and the jumbo one.

use iced::advanced::renderer::{Headless as _, Renderer as _};
use iced::{Rectangle, Vector};

use crate::name::tests::support::renderer;

use super::super::draw::{Colors, Painter};
use super::*;

/// Where the paragraph sits: off the pixel grid, as a message's text may.
const ORIGIN: Vector = Vector::new(8.0, 7.3);
const WIDTH: f32 = 400.0;

/// One span struck and underlined, a wide gap of spaces in its middle
/// where nothing but its two lines draws.
fn struck() -> Text {
    anomi_vm::text::markdown(anomi_vm::text::Source::new("__~~a          b~~__"))
}

/// `text` laid out at `size` pixels a line's em.
fn laid_at(text: &Text, size: f32) -> State {
    fonts::install();
    let mut state = State::default();
    state.lay_out(
        text,
        Look {
            size,
            framed: false,
            max_lines: None,
            hint_factor: None,
        },
        WIDTH,
    );
    state.arrange(WIDTH);
    state.shown = state.full;
    state
}

/// The rows the struck span's lines cover in the column through its
/// middle, at `size` and `scale`, as runs of rows: where each starts, and
/// how many. Panics on a row only partly covered.
fn lines(size: f32, scale: f32) -> Vec<(u32, u32)> {
    let theme = Theme::built_in(Some("bonfire"));
    let palette = theme.palette();
    let text = struck();
    let state = laid_at(&text, size);
    let (column, rows) = {
        let shapes = state.shapes.borrow();
        let shaped = &shapes.blocks[0];
        let (_, spans) = text.blocks().next().expect("a block");
        let run = shaped
            .runs
            .iter()
            .find(|run| spans[run.span as usize].style.strike)
            .expect("the struck span's run");
        assert!(spans[run.span as usize].style.underline);
        let at = ORIGIN + Vector::new(shaped.origin.x, shaped.origin.y);
        let middle = at.x + (run.left + run.right) / 2.0;
        let top = at.y + run.top;
        let column = (middle * scale) as u32;
        let rows = (top * scale).floor() as u32..((top + run.height) * scale).ceil() as u32;
        (column, rows)
    };
    let window = Size::new(WIDTH + 2.0 * ORIGIN.x, state.full + 2.0 * ORIGIN.y);
    let viewport = Rectangle::with_size(window);
    let mut renderer = renderer();
    renderer.reset(viewport);
    Painter {
        state: &state,
        text: &state.text,
        theme: &theme,
        colors: Colors {
            text: palette.text,
            heading: palette.text,
        },
        origin: ORIGIN,
        viewport: &viewport,
        scale,
    }
    .draw(&mut renderer);
    let physical = Size::new(
        (window.width * scale).round() as u32,
        (window.height * scale).round() as u32,
    );
    let rgba = renderer.screenshot(physical, scale, palette.chat);
    let image = image::RgbaImage::from_raw(physical.width, physical.height, rgba)
        .expect("a pixel for each of the window's");
    let (ground, ink) = (palette.chat.into_rgba8(), palette.text.into_rgba8());
    let near = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2);
    let mut covered: Vec<(u32, u32)> = Vec::new();
    for row in rows {
        let pixel = image.get_pixel(column, row).0;
        if near(pixel, ground) {
            continue;
        }
        assert!(
            near(pixel, ink),
            "at {size} px and {scale}x, row {row} is only partly covered: {pixel:?}"
        );
        match covered.last_mut() {
            Some((start, count)) if *start + *count == row => *count += 1,
            _ => covered.push((row, 1)),
        }
    }
    covered
}

/// How many device pixels thick a line is at `size` and `scale`.
fn thick(lines: &[(u32, u32)]) -> Vec<u32> {
    lines.iter().map(|&(_, count)| count).collect()
}

#[test]
fn lines_through_and_under_cover_whole_rows_at_the_message_size() {
    // 0.05 em of 15 pixels: one pixel at 1x, 1.5 rounded to two at 2x.
    assert_eq!(thick(&lines(15.0, 1.0)), [1, 1]);
    assert_eq!(thick(&lines(15.0, 2.0)), [2, 2]);
}

#[test]
fn a_jumbo_lines_lines_are_two_pixels_thick() {
    // 0.05 em of 33 pixels: 1.65 rounded to two at 1x, 3.3 to three at 2x.
    assert_eq!(thick(&lines(33.0, 1.0)), [2, 2]);
    assert_eq!(thick(&lines(33.0, 2.0)), [3, 3]);
}
