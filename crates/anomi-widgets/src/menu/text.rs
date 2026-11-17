//! Text as the menus and the tooltip draw it themselves: measured once at
//! layout, painted in one line elided at its end, or wrapped at words.

use iced::advanced::text::{
    Alignment, Ellipsis, LineHeight, Paragraph as _, Renderer as _, Shaping, Text, Wrapping,
};
use iced::{Color, Font, Pixels, Point, Rectangle, Size, alignment};

use anomi_theme::Renderer;

type Paragraph = <Renderer as iced::advanced::text::Renderer>::Paragraph;

fn text<C>(content: C, size: f32, font: Font, bounds: Size, wrapping: Wrapping) -> Text<C> {
    Text {
        content,
        bounds,
        size: Pixels(size),
        line_height: LineHeight::default(),
        font,
        align_x: Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping,
        ellipsis: Ellipsis::None,
        hint_factor: None,
    }
}

/// The width one line of text takes at a size and face.
pub fn width(content: &str, size: f32, font: Font) -> f32 {
    if content.is_empty() {
        return 0.0;
    }
    Paragraph::with_text(text(content, size, font, Size::INFINITE, Wrapping::None))
        .min_bounds()
        .width
        .ceil()
}

/// The height text takes wrapped at words to `width`.
pub(crate) fn wrapped_height(content: &str, size: f32, font: Font, width: f32) -> f32 {
    if content.is_empty() {
        return 0.0;
    }
    let bounds = Size::new(width.max(1.0), f32::INFINITY);
    Paragraph::with_text(text(content, size, font, bounds, Wrapping::Word))
        .min_bounds()
        .height
        .ceil()
}

/// One line's height at a size.
pub(crate) fn line_height(size: f32) -> f32 {
    LineHeight::default().to_absolute(Pixels(size)).0
}

/// One line from `at`'s left, its top at `at`, elided at its end to fit
/// `width`.
pub(crate) fn line(
    renderer: &mut Renderer,
    content: &str,
    (size, font): (f32, Font),
    at: Point,
    width: f32,
    ink: Color,
    clip: Rectangle,
) {
    let mut line = text(
        content.to_owned(),
        size,
        font,
        Size::new(width.max(0.0), line_height(size)),
        Wrapping::None,
    );
    line.ellipsis = Ellipsis::End;
    renderer.fill_text(line, at, ink, clip);
}

/// Text from `at`, wrapped at words to `width`.
pub(crate) fn wrapped(
    renderer: &mut Renderer,
    content: &str,
    (size, font): (f32, Font),
    at: Point,
    width: f32,
    ink: Color,
    clip: Rectangle,
) {
    let block = text(
        content.to_owned(),
        size,
        font,
        Size::new(width.max(1.0), f32::INFINITY),
        Wrapping::Word,
    );
    renderer.fill_text(block, at, ink, clip);
}

/// A colour faded to `alpha` of its own.
pub(crate) fn faded(color: Color, alpha: f32) -> Color {
    color.scale_alpha(alpha)
}
