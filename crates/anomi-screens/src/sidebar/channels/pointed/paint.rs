//! How a row draws: its ground, ring, outline and unread mark, its body in
//! its ink and strength, the trailing part or the icon fading over it, and
//! a heading's chevron. The
//! renderer at this pin can't draw a rotated svg (`tiny_skia/src/vector.rs`
//! sizes the raster by the transform's `sx`, 0 at a quarter turn), so the
//! chevron is Lucide's `chevron-down` stroked and turned, as the menu caret
//! is (`anomi_widgets::button`'s `caret.rs`).

use std::f32::consts::FRAC_PI_2;

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::svg::{self as vector, Renderer as _};
use iced::advanced::widget::{Tree, Widget as _};
use iced::advanced::{Layout, mouse};
use iced::border::Radius;
use iced::widget::canvas::{Frame, LineCap, LineJoin, Path, Stroke};
use iced::{Border, Color, Point, Radians, Rectangle, Size, Vector};

use anomi_theme::{Renderer, Theme};

use super::state::State;
use super::{CHEVRON, FADING, LEADING, MARK, PAD, Pointed, RING};

impl<M> Pointed<'_, M> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let palette = theme.palette();
        let look = &self.look;
        let lit = state.lit();
        let fill = look.fill(palette, lit);
        let ring = |width: f32, colour: Color, radius: f32| Quad {
            bounds,
            border: Border {
                color: colour,
                width,
                radius: radius.into(),
            },
            ..Quad::default()
        };
        if fill.a > 0.0 {
            renderer.fill_quad(ring(0.0, Color::TRANSPARENT, look.radius), fill);
        }
        if look.aimed {
            let live = palette.live.scale_alpha(look.dim);
            renderer.fill_quad(ring(RING, live, look.radius), Color::TRANSPARENT);
        }
        if let Some(radius) = look.ring
            && state.focused
        {
            renderer.fill_quad(ring(RING, palette.accent, radius), Color::TRANSPARENT);
        }
        if let Some(offset) = look.mark
            && look.unread
            && !look.selected
        {
            let mark = Rectangle::new(
                Point::new(bounds.x - offset, bounds.center_y() - MARK.1 / 2.0),
                Size::new(MARK.0, MARK.1),
            );
            let round = Radius::default().top_right(MARK.0).bottom_right(MARK.0);
            renderer.fill_quad(
                Quad {
                    bounds: mark,
                    border: Border::default().rounded(round),
                    ..Quad::default()
                },
                palette.text.scale_alpha(look.dim),
            );
        }
        let strength = look.strength(lit);
        let ink = look.ink(palette, lit, style.text_color);
        let inked = renderer::Style {
            text_color: ink.scale_alpha(strength),
        };
        let faded = state.faded(theme, strength);
        if let Some((glyph, tint)) = self.leading {
            let at = Point::new(
                bounds.x + PAD,
                bounds.y + self.top + (bounds.height - self.top - LEADING) / 2.0,
            );
            let svg = vector::Svg::new(glyph.handle())
                .color(tint(palette, look))
                .opacity(strength);
            renderer.draw_svg(
                svg,
                Rectangle::new(at, Size::new(LEADING, LEADING)),
                *viewport,
            );
        }
        let mut children = layout.iter(&tree.children);
        let (child, child_tree) = children.next().expect("the body");
        self.body.draw(
            child_tree, renderer, &faded, &inked, child, cursor, viewport,
        );
        if let (Some(trailing), Some((child, child_tree))) = (&self.trailing, children.next())
            && state.trailing
        {
            match look.fading {
                // The part is only the icon's click target; the renderer
                // drops an svg's colour alpha, so the row draws the icon,
                // `lit` as strong.
                Some(glyph) if lit > 0.0 => {
                    let area = child.bounds();
                    let tint = if cursor.is_over(area) {
                        palette.text
                    } else {
                        palette.muted
                    };
                    let at = Point::new(
                        area.center_x() - FADING / 2.0,
                        area.center_y() - FADING / 2.0,
                    );
                    let svg = vector::Svg::new(glyph.handle())
                        .color(tint)
                        .opacity(lit * strength);
                    renderer.draw_svg(
                        svg,
                        Rectangle::new(at, Size::new(FADING, FADING)),
                        *viewport,
                    );
                }
                Some(_) => {}
                None => trailing.draw(
                    child_tree, renderer, &faded, &inked, child, cursor, viewport,
                ),
            }
        }
        if self.chevron.is_some() {
            let at = Point::new(
                bounds.x + state.chevron_x,
                bounds.y + self.top + (bounds.height - self.top - CHEVRON) / 2.0,
            );
            let ink = ink.scale_alpha(strength);
            chevron(renderer, at, CHEVRON, state.turned(), ink);
        }
    }
}

/// The icon's own box, in which its path is given
/// (`ui/icons/chevron-down.svg`), and its stroke there.
const BOX: f32 = 24.0;
const STROKE: f32 = 2.0;

/// The chevron in a `side` square at `at`, pointing down at `turn` 0 and
/// right at 1 (`transform-rotation: -90deg`, `ui/rail.slint:874`), in `ink`.
pub(super) fn chevron(renderer: &mut Renderer, at: Point, side: f32, turn: f32, ink: Color) {
    let mut frame = Frame::new(renderer, Size::new(side, side));
    frame.translate(Vector::new(side / 2.0, side / 2.0));
    frame.rotate(Radians(-FRAC_PI_2 * turn));
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
