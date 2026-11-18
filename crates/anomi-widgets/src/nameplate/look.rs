//! What a plate draws, as `Nameplate` has it (`ui/controls.slint:663-696`):
//! its picture at the right end, as tall as the row, at Discord's strength;
//! under a list row's text a fade to the ground, letting the picture through
//! at .3 there and in full 50 px after; over both its palette's tint, from
//! nothing at the left edge.

use std::f32::consts::FRAC_PI_2;

use iced::advanced::image::Renderer as _;
use iced::advanced::renderer::{Quad, Renderer as _};
use iced::border::Radius;
use iced::gradient::Linear;
use iced::widget::image;
use iced::{Background, Border, Color, Rectangle};

use anomi_theme::{Palette, Renderer};

use crate::picture::{Fit, Shape};

use super::widget::State;
use super::{Nameplate, Place};

/// The fade lets this much of the ground through under the text
/// (`ui/controls.slint:689`), and ends this far after its start.
const FADE: f32 = 0.7;
const FADE_LENGTH: f32 = 50.0;

impl Place {
    /// The picture's opacity: Discord's .6 at rest and .9 selected or
    /// pointed at; the account card's .4 rises to .6 under the pointer.
    pub(super) fn strength(self, hovered: bool, selected: bool) -> f32 {
        match self {
            Self::Account if hovered => 0.6,
            Self::Account => 0.4,
            _ if selected || hovered => 0.9,
            _ => 0.6,
        }
    }

    /// Where the fade under the text starts and ends, from the plate's left,
    /// for a plate `width` wide over text found at `text` (its left and
    /// width): just after the text, a little before it in a DM row. A member
    /// row's plate reads the text's width alone, as the member list handed
    /// it, and fades in from there.
    pub(super) fn fade(self, width: f32, text: (f32, f32)) -> (f32, f32) {
        let (x, wide) = text;
        let (end, after) = match self {
            Self::Dm => (x + wide, -5.0),
            Self::Member | Self::Account => (wide, 10.0),
        };
        let from = width.min(end + after).max(0.0);
        (from, width.min(from + FADE_LENGTH))
    }

    /// The tint from the palette's colour: from a tenth to .4 on the account
    /// card; elsewhere from nothing, .08 from a fifth to half the way, and at
    /// the right end .5 selected, .3 under the pointer, else .2. A DM row's
    /// stays at .2 under the pointer.
    pub(super) fn tint(self, colour: Color, hovered: bool, selected: bool) -> Linear {
        let line = Linear::new(FRAC_PI_2);
        if self == Self::Account {
            return line
                .add_stop(0.0, colour.scale_alpha(0.1))
                .add_stop(1.0, colour.scale_alpha(0.4));
        }
        let end = if selected {
            0.5
        } else if hovered && self != Self::Dm {
            0.3
        } else {
            0.2
        };
        line.add_stop(0.0, colour.scale_alpha(0.0))
            .add_stop(0.2, colour.scale_alpha(0.08))
            .add_stop(0.5, colour.scale_alpha(0.08))
            .add_stop(1.0, colour.scale_alpha(end))
    }
}

impl<Message> Nameplate<'_, Message> {
    /// Whether the plate shows: a DM row's only while the row is selected,
    /// pointed at or focused.
    pub(super) fn shows(&self, hovered: bool) -> bool {
        (self.picture.is_some() || self.palette.is_some())
            && (self.place != Place::Dm || self.selected || self.focused || hovered)
    }

    /// Pointed at, or held so.
    pub(super) fn lit(&self, state: &State) -> bool {
        self.held || state.hovered
    }

    /// The plate's box in the row's.
    pub(super) fn plate_bounds(&self, bounds: Rectangle) -> Rectangle {
        bounds.shrink(self.inset)
    }

    fn corners(&self) -> (f32, f32) {
        let radius = (self.radius - self.inset).max(0.0);
        let top = self
            .top_radius
            .map_or(radius, |top| (top - self.inset).max(0.0));
        (top, radius)
    }

    /// Where the picture lies: as tall as the plate, as wide as that makes
    /// it, against the plate's right end.
    pub(super) fn picture_bounds(
        &self,
        renderer: &Renderer,
        plate: Rectangle,
    ) -> Option<Rectangle> {
        let size = renderer.measure_image(self.picture.as_ref()?)?;
        if size.height == 0 {
            return None;
        }
        let width = plate.height * size.width as f32 / size.height as f32;
        Some(Rectangle {
            x: plate.x + plate.width - width,
            width,
            ..plate
        })
    }

    /// Draws the plate, then `content` over it. tiny-skia draws a layer's
    /// pictures after its fills, whatever order they came in, and a layer
    /// opened later over all of an earlier one: over a picture, the fade,
    /// the tint and an offline plate's [`dim`] go in a layer of their own
    /// and the content in one after it. A plate with no picture yet draws
    /// its tint where it is.
    ///
    /// A layer is clipped to its own bounds only, not to the one around it,
    /// so each is opened on the part of the plate within `viewport`: a row
    /// half scrolled out of a list draws none of its plate over what's
    /// above the list.
    pub(super) fn paint(
        &self,
        state: &State,
        renderer: &mut Renderer,
        palette: &Palette,
        (bounds, viewport): (Rectangle, &Rectangle),
        content: impl FnOnce(&mut Renderer),
    ) {
        let lit = self.lit(state);
        let plate = self.plate_bounds(bounds);
        // None for a plate with no width, as for one out of sight.
        let shown = plate.intersection(viewport).filter(|_| self.shows(lit));
        let Some(shown) = shown else {
            content(renderer);
            return;
        };
        let (top, radius) = self.corners();
        let border = Border {
            radius: Radius {
                top_left: top,
                top_right: top,
                bottom_right: radius,
                bottom_left: radius,
            },
            ..Border::default()
        };
        let picture = self
            .picture
            .as_ref()
            .zip(self.picture_bounds(renderer, plate));
        let tint = self.tint(palette, lit);
        let Some((still, at)) = picture else {
            if let Some(tint) = tint {
                fill(renderer, plate, border, tint.scale_alpha(self.opacity));
            }
            content(renderer);
            return;
        };
        let strength = self.place.strength(lit, self.selected) * self.opacity;
        let corners = border.radius;
        self.draw_picture(
            state,
            renderer,
            still,
            (plate, at, shown),
            corners,
            strength,
            viewport,
        );
        renderer.with_layer(shown, |renderer| {
            let ground = self.beneath(palette, lit);
            if self.place != Place::Account {
                self.fade(state, renderer, ground, (plate, border));
            }
            if let Some(tint) = tint {
                if self.opacity < 1.0 {
                    fill(renderer, plate, border, dim(tint, ground, self.opacity));
                }
                fill(renderer, plate, border, tint.scale_alpha(self.opacity));
            }
        });
        renderer.with_layer(*viewport, content);
    }

    /// What lies under the plate, which the fade lets through: `active`
    /// chosen, `hover` pointed at, else its ground.
    fn beneath(&self, palette: &Palette, lit: bool) -> Color {
        if self.selected {
            palette.active
        } else if lit {
            palette.hover
        } else {
            (self.ground)(palette)
        }
    }

    /// The fade under the text: `ground` at .7 up to just after the text,
    /// clear 50 px later. A fade that would start at the plate's right end,
    /// as for a row with no mark, is the shade alone: a clear stop at 1
    /// would take the shade's place there and fade the whole plate out.
    ///
    /// It stays at .7 at any opacity. Slint fades the plate as one layer
    /// (`Opacity::need_layer`, `vendor/i-slint-core/items.rs:959`), which
    /// leaves every point `opacity` as far from the ground as at 1; the fade
    /// is the ground's own colour, so over the picture at `strength *
    /// opacity` it does the same. The tint over both, at `opacity` of its
    /// own, lets more of them through than Slint's layer does, and [`dim`]
    /// takes that back first.
    fn fade(
        &self,
        state: &State,
        renderer: &mut Renderer,
        ground: Color,
        (plate, border): (Rectangle, Border),
    ) {
        let text = state.extent.unwrap_or((0.0, plate.width));
        let (from, to) = self.place.fade(plate.width, text);
        let shade = ground.scale_alpha(FADE);
        if from >= plate.width {
            fill(renderer, plate, border, shade);
            return;
        }
        let clear = ground.scale_alpha(0.0);
        let fade = Linear::new(FRAC_PI_2)
            .add_stop(0.0, shade)
            .add_stop(from / plate.width, shade)
            .add_stop(to / plate.width, clear)
            .add_stop(1.0, clear);
        fill(renderer, plate, border, fade);
    }

    /// The palette's tint at full opacity, in the theme's darkness, when it
    /// has a colour.
    fn tint(&self, palette: &Palette, lit: bool) -> Option<Linear> {
        let [dark, light] = self.palette?;
        let colour = if palette.dark { dark } else { light };
        (colour.a > 0.0).then(|| self.place.tint(colour, lit, self.selected))
    }

    /// The picture, its right corners the plate's; cut to the plate's part
    /// within the viewport, `shown`, when it's wider than the plate, whose
    /// left corners it then fills square.
    #[allow(clippy::too_many_arguments)]
    fn draw_picture(
        &self,
        state: &State,
        renderer: &mut Renderer,
        still: &image::Handle,
        (plate, at, shown): (Rectangle, Rectangle, Rectangle),
        corners: Radius,
        strength: f32,
        viewport: &Rectangle,
    ) {
        let overflows = at.x < plate.x;
        let shape = Shape {
            radius: if overflows {
                corners
            } else {
                Radius {
                    top_left: 0.0,
                    bottom_left: 0.0,
                    ..corners
                }
            },
            round: false,
            fit: Fit::Fill,
            exact: false,
        };
        if overflows {
            renderer.with_layer(shown, |renderer| {
                state
                    .player
                    .draw(renderer, still, at, shape, strength, viewport);
            });
        } else {
            state
                .player
                .draw(renderer, still, at, shape, strength, viewport);
        }
    }
}

/// What a plate at `opacity` lays over its picture and fade, toward
/// `ground`, before its `tint`: so much that as little of them comes through
/// the tint as through Slint's layer.
///
/// Where the tint is `t`, the layer lets `1 - t` of what's under it through
/// and then fades the whole; the tint drawn at `opacity * t` over a picture
/// already faded lets `1 - opacity * t` through, half again as much at the
/// selected .5 end at half. Taking `1 - (1 - t) / (1 - opacity * t)` of it
/// back at each of the tint's stops leaves it half as far from the ground
/// there, and close to it between.
fn dim(tint: Linear, ground: Color, opacity: f32) -> Linear {
    // The tint's stops lie in order, as `add_stop` wants them: it replaces
    // the stop at an offset it has, and holds eight.
    tint.stops
        .iter()
        .flatten()
        .fold(Linear::new(tint.angle), |dim, stop| {
            let through = (1.0 - stop.color.a) / (1.0 - opacity * stop.color.a);
            dim.add_stop(stop.offset, ground.scale_alpha(1.0 - through))
        })
}

fn fill(
    renderer: &mut Renderer,
    bounds: Rectangle,
    border: Border,
    background: impl Into<Background>,
) {
    renderer.fill_quad(
        Quad {
            bounds,
            border,
            ..Quad::default()
        },
        background,
    );
}
