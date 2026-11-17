//! How a row draws: its ground, ring, outline and unread mark, its body in
//! its ink and strength, the trailing part or the icon fading over it, and
//! a heading's chevron. The
//! renderer at this pin can't draw a rotated svg (`tiny_skia/src/vector.rs`
//! sizes the raster by the transform's `sx`, 0 at a quarter turn), so the
//! chevron is the menu caret's Lucide `chevron-down`, stroked and turned
//! (`anomi_widgets::button::chevron`).

use std::f32::consts::FRAC_PI_2;

use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::svg::{self as vector, Renderer as _};
use iced::advanced::widget::{Tree, Widget as _};
use iced::advanced::{Layout, mouse};
use iced::border::Radius;
use iced::{Border, Color, Point, Radians, Rectangle, Size};

use anomi_theme::{Renderer, Theme};
use anomi_widgets::button::chevron;

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
            // Down at rest, right while folded (`transform-rotation:
            // -90deg`, `ui/rail.slint:874`).
            let angle = Radians(-FRAC_PI_2 * state.turned());
            chevron(renderer, at, CHEVRON, angle, ink);
        }
    }
}
