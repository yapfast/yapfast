//! What a chip draws: its emoji, its two small buttons and the lines of text.

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::advanced::svg::{self as vector, Renderer as _};
use iced::advanced::text::{
    Alignment, Ellipsis, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::{Border, Color, Pixels, Point, Rectangle, Size, alignment};

use anomi_theme::{Palette, Renderer, typography};
use anomi_vm::icon::Icon;
use anomi_vm::voice::soundboard::Emoji;
use anomi_widgets::picture::{Fit, Shape};

use super::chip::{Chip, EMOJI, GLYPH, Line, PLAIN, Part, STAR, State};

pub(super) fn text(content: &str, size: f32, bounds: Size) -> Text<&str> {
    Text {
        content,
        bounds,
        size: Pixels(size),
        line_height: LineHeight::default(),
        font: typography::UI,
        align_x: Alignment::Left,
        align_y: alignment::Vertical::Center,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
        ellipsis: Ellipsis::End,
        hint_factor: None,
    }
}

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

pub(super) fn faded(colour: Color, alpha: f32) -> Color {
    Color {
        a: colour.a * alpha,
        ..colour
    }
}

impl Chip<'_> {
    pub(super) fn emoji_bounds(&self, bounds: Rectangle) -> Rectangle {
        Rectangle::new(
            Point::new(bounds.x + PLAIN, bounds.center_y() - EMOJI / 2.0),
            Size::new(EMOJI, EMOJI),
        )
    }

    pub(super) fn paint_emoji(
        &self,
        renderer: &mut Renderer,
        state: &State,
        bounds: Rectangle,
        alpha: f32,
        viewport: &Rectangle,
    ) {
        let at = self.emoji_bounds(bounds);
        match &self.sound.emoji {
            Emoji::Custom(picture) => {
                if let Some(handle) = picture.ready() {
                    let shape = Shape {
                        fit: Fit::Contain,
                        ..Shape::default()
                    };
                    state
                        .picture
                        .draw(renderer, handle, at, shape, alpha, viewport);
                }
            }
            Emoji::Glyph(_) => {
                let area = Rectangle::new(
                    Point::new(at.x, bounds.y),
                    Size::new(EMOJI + 8.0, bounds.height),
                );
                draw_line(
                    renderer,
                    &state.glyph,
                    at.x,
                    area,
                    faded(Color::WHITE, alpha),
                );
            }
            Emoji::None => {}
        }
    }

    pub(super) fn paint_hear(
        &self,
        renderer: &mut Renderer,
        palette: &Palette,
        state: &State,
        bounds: Rectangle,
        viewport: &Rectangle,
    ) {
        let face = self.hear(bounds);
        let ink = if state.over == Some(Part::Hear) {
            palette.text
        } else {
            palette.secondary
        };
        button(
            renderer,
            face,
            palette.button_default_hover,
            Icon::Volume,
            ink,
            viewport,
        );
    }

    pub(super) fn paint_star(
        &self,
        renderer: &mut Renderer,
        palette: &Palette,
        state: &State,
        bounds: Rectangle,
        viewport: &Rectangle,
    ) {
        let face = self.star(bounds);
        let fill = if state.over.is_some() {
            palette.button_default_hover
        } else {
            palette.button_default
        };
        let (icon, ink) = if self.sound.favorite {
            (Icon::StarFilled, STAR)
        } else if state.over == Some(Part::Star) {
            (Icon::Star, palette.text)
        } else {
            (Icon::Star, palette.secondary)
        };
        button(renderer, face, fill, icon, ink, viewport);
    }
}

/// A small icon button's face: its fill and its glyph in the middle.
fn button(
    renderer: &mut Renderer,
    face: Rectangle,
    fill: Color,
    icon: Icon,
    ink: Color,
    viewport: &Rectangle,
) {
    renderer.fill_quad(
        Quad {
            bounds: face,
            border: Border::default().rounded(6.0),
            ..Quad::default()
        },
        fill,
    );
    let mut svg = vector::Svg::new(icon.handle());
    svg.color = Some(Color { a: 1.0, ..ink });
    svg.opacity = ink.a;
    let at = Rectangle::new(
        Point::new(face.center_x() - GLYPH / 2.0, face.center_y() - GLYPH / 2.0),
        Size::new(GLYPH, GLYPH),
    );
    renderer.draw_svg(svg, at, *viewport);
}

pub(super) fn mix(from: Color, to: Color, amount: f32) -> Color {
    let blend = |a: f32, b: f32| a + (b - a) * amount;
    Color {
        r: blend(from.r, to.r),
        g: blend(from.g, to.g),
        b: blend(from.b, to.b),
        a: blend(from.a, to.a),
    }
}
