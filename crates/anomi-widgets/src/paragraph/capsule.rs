//! Inline code's capsule: a translucent grey under the code, outlined a
//! little stronger, centred between the code's capital height and a
//! shallow descender. Ports `draw_inline_code_backgrounds`
//! (`sharedparley/draw.rs:335`), whose colours follow the text's
//! brightness rather than the theme's, so light and dark themes both get
//! a sensible one.

use iced::border::Border;
use iced::{Color, Rectangle, Size, Vector};

use anomi_theme::Renderer;

use super::decorate::Run;
use super::draw::fill;

/// The capsule's fill, the same on every theme (`sharedparley/draw.rs:354`).
const FILL: Color = Color::from_rgba8(128, 128, 128, 28.0 / 255.0);
/// Its outline under bright text, on a dark theme (`:356`).
const EDGE_ON_DARK: Color = Color::from_rgba8(170, 170, 170, 88.0 / 255.0);
/// Its outline under dark text, on a light theme (`:358`).
const EDGE_ON_LIGHT: Color = Color::from_rgba8(128, 128, 128, 56.0 / 255.0);
/// Text brighter than this, out of 255, is on a dark theme (`:355`).
const BRIGHT_TEXT: f32 = 140.0;
/// The outline's width (`BORDER_WIDTH`, `:363`).
const EDGE_WIDTH: f32 = 1.0;
/// Its corners: this share of its height, within these
/// (`sharedparley/draw.rs:431`, `MIN_RADIUS` and `MAX_RADIUS`, `:364`).
const RADIUS_RATIO: f32 = 0.22;
const MIN_RADIUS: f32 = 2.0;
const MAX_RADIUS: f32 = 5.0;
/// The usual monospace fonts' metrics, in ems, where the Slint patch read
/// the font's own: capitals, descender and ascent (`:397-399`).
const CAP_HEIGHT: f32 = 0.73;
const DESCENT: f32 = 0.24;
const ASCENT: f32 = 0.93;
/// How far below the baseline it reaches, as a share of the descender:
/// where parentheses, commas and dots end (`lower_extent`, `:406`).
const DESCENT_SHARE: f32 = 0.4;
/// The space above and below the code, as a share of the ascent
/// (`VERTICAL_PADDING_RATIO`, `:368`).
const VERTICAL_PADDING_RATIO: f32 = 0.15;

/// Draws the capsule under `run`, inline code over text in `text`.
pub fn draw(renderer: &mut Renderer, run: &Run, at: Vector, text: Color) {
    let em = run.size;
    let (upper, lower) = (CAP_HEIGHT * em, DESCENT * em * DESCENT_SHARE);
    let center = run.baseline + (lower - upper) / 2.0;
    let half = (upper + lower) / 2.0 + ASCENT * em * VERTICAL_PADDING_RATIO;
    let height = (half * 2.0).max(1.0);
    let bounds = Rectangle::new(
        iced::Point::new(run.left, center - half),
        Size::new(run.right - run.left, height),
    );
    let edge = if luma(text) * 255.0 > BRIGHT_TEXT {
        EDGE_ON_DARK
    } else {
        EDGE_ON_LIGHT
    };
    let border = Border {
        color: edge,
        width: EDGE_WIDTH,
        radius: (height * RADIUS_RATIO).clamp(MIN_RADIUS, MAX_RADIUS).into(),
    };
    fill(renderer, bounds + at, FILL, border.radius, Some(border));
}

/// How bright `color` reads, from 0 to 1, by the weights of BT.601's luma.
fn luma(color: Color) -> f32 {
    0.299 * color.r + 0.587 * color.g + 0.114 * color.b
}
