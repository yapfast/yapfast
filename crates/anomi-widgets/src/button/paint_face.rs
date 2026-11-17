//! A face's drawing: its fill and focus ring, its icons and text in a row,
//! and its caption.

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::advanced::svg as vector;
use iced::advanced::svg::Renderer as _;
use iced::{Border, Color, Point, Radians, Rectangle, Size};

use anomi_theme::{Palette, Renderer, Theme};

use super::caret;
use super::face::{CAPTION_GAP, CAPTION_HEIGHT, Face, RING};
use super::look::Colours;
use super::paint;
use super::state::State;

impl<Message> Face<'_, Message> {
    pub(super) fn paint(
        &self,
        state: &State,
        renderer: &mut Renderer,
        theme: &Theme,
        bounds: Rectangle,
        viewport: &Rectangle,
    ) {
        let palette = theme.palette();
        let colours = self.colours(theme, state);
        let shape = self.shape(bounds);

        // A press gives to the shape and everything in it, about its centre.
        let give = State::amount(&state.pressed_by, state.now);
        let scale = 1.0 - (1.0 - self.press) * give;
        if scale < 1.0 {
            let centre = shape.center();
            let gives = iced::Transformation::translate(centre.x, centre.y)
                * iced::Transformation::scale(scale)
                * iced::Transformation::translate(-centre.x, -centre.y);
            renderer.with_transformation(gives, |renderer| {
                self.paint_shape(state, renderer, palette, colours, shape, viewport);
            });
        } else {
            self.paint_shape(state, renderer, palette, colours, shape, viewport);
        }

        if self.caption.is_some() {
            let area = Rectangle::new(
                Point::new(bounds.x, bounds.y + self.min.height + CAPTION_GAP),
                Size::new(bounds.width, CAPTION_HEIGHT),
            );
            let wide = state.caption.min_width().min(area.width);
            paint::draw_line(
                renderer,
                &state.caption,
                area.center_x() - wide / 2.0,
                area,
                colours.caption,
            );
        }
    }

    /// The fill, the ring, then the icons and text in a row centred in
    /// `shape`; the text takes only the room the icons and padding leave.
    fn paint_shape(
        &self,
        state: &State,
        renderer: &mut Renderer,
        palette: &Palette,
        colours: Colours,
        shape: Rectangle,
        viewport: &Rectangle,
    ) {
        let ring = state.focused && self.live();
        renderer.fill_quad(
            Quad {
                bounds: shape,
                border: Border {
                    color: palette.accent,
                    width: if ring { RING } else { 0.0 },
                    radius: self.radius.into(),
                },
                ..Quad::default()
            },
            colours.fill,
        );

        let room = (shape.width - self.chrome()).max(0.0);
        let text_width = self.text_width(state).min(room);
        let mut x = shape.center_x() - self.content_width(text_width) / 2.0;
        if let Some(icon) = self.leading {
            let at = Point::new(x, shape.center_y() - self.glyph / 2.0);
            if self.turns {
                let turn = State::amount(&state.turn, state.now);
                let angle = Radians(std::f32::consts::PI * turn);
                caret::chevron(renderer, at, self.glyph, angle, colours.ink);
            } else {
                self.icon(renderer, icon, at, self.glyph, colours.ink, viewport);
            }
            x += self.glyph + self.spacing;
        }
        if !self.text.is_empty() {
            let area = Rectangle::new(Point::new(x, shape.y), Size::new(text_width, shape.height));
            paint::draw_line(renderer, &state.text, x, area, colours.ink);
            x += text_width + self.spacing;
        }
        if let Some(icon) = self.trailing {
            let at = Point::new(x, shape.center_y() - self.trailing_glyph / 2.0);
            self.icon(
                renderer,
                icon,
                at,
                self.trailing_glyph,
                colours.ink,
                viewport,
            );
        }
    }

    fn icon(
        &self,
        renderer: &mut Renderer,
        icon: crate::icon::Icon,
        at: Point,
        side: f32,
        ink: Color,
        viewport: &Rectangle,
    ) {
        let mut svg = vector::Svg::new(icon.handle());
        // tiny-skia keeps only the rgb of an svg colour, so the alpha goes by opacity.
        svg.color = Some(Color { a: 1.0, ..ink });
        svg.opacity = ink.a;
        renderer.draw_svg(svg, Rectangle::new(at, Size::new(side, side)), *viewport);
    }
}
