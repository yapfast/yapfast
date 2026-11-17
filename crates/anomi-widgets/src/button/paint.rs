//! What a face draws with: the lines of text it keeps shaped, and a blend of
//! two colours.

use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{
    Alignment, Ellipsis, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::{Color, Font, Pixels, Point, Rectangle, Size, alignment};

use anomi_theme::Renderer;

/// One line of `content`, left aligned and centred on its height, elided at
/// its end when it doesn't fit `bounds`. Hold it in a
/// [`Line`](iced::advanced::text::paragraph::Plain) and `update` it in layout,
/// so it is shaped only when it changes.
pub(super) fn text(content: &str, size: f32, font: Font, bounds: Size) -> Text<&str> {
    Text {
        content,
        bounds,
        size: Pixels(size),
        line_height: LineHeight::default(),
        font,
        align_x: Alignment::Left,
        align_y: alignment::Vertical::Center,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
        ellipsis: Ellipsis::End,
        hint_factor: None,
    }
}

/// A shaped line, kept in a face's state.
pub(super) type Line = Plain<<Renderer as iced::advanced::text::Renderer>::Paragraph>;

/// Draws a shaped `line` with its left edge at `x`, centred on `area`'s
/// height, clipped to `area`.
pub(super) fn draw_line(renderer: &mut Renderer, line: &Line, x: f32, area: Rectangle, ink: Color) {
    let at = Rectangle::new(
        Point::new(x, area.y),
        Size::new(line.min_width(), area.height),
    );
    let position = at.anchor(
        line.min_bounds(),
        Alignment::Left,
        alignment::Vertical::Center,
    );
    renderer.fill_paragraph(line.raw(), position, ink, area);
}

/// `from` at 0, `to` at 1. A colour with no alpha has no hue of its own, so
/// a fill that appears on hover grows from its own colour, not from black.
pub(super) fn blend(from: Color, to: Color, amount: f32) -> Color {
    if amount <= 0.0 {
        return from;
    }
    if amount >= 1.0 {
        return to;
    }
    let (a, b) = match (from.a == 0.0, to.a == 0.0) {
        (true, false) => (Color { a: 0.0, ..to }, to),
        (false, true) => (from, Color { a: 0.0, ..from }),
        _ => (from, to),
    };
    let mix = |x: f32, y: f32| x + (y - x) * amount;
    Color {
        r: mix(a.r, b.r),
        g: mix(a.g, b.g),
        b: mix(a.b, b.b),
        a: mix(a.a, b.a),
    }
}
