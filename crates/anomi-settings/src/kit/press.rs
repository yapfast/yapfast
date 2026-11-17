//! What the kit's pressable parts share: the pointer, the keys' focus and the
//! fade between rest and hover (`animate ... { duration: Theme.quick; }`),
//! and the colours they paint with.

use std::time::Instant;

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::advanced::svg::{self as vector, Renderer as _};
use iced::advanced::widget::operation::Focusable;
use iced::animation::{Animation, Easing};
use iced::{Border, Color, Point, Rectangle, Size};

use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::{Motion, Renderer};
use anomi_vm::icon::Icon;

/// The focus ring's width, the accent round a part the keys reached
/// (`border-width: focus.has-focus ? 2px : 0px`).
pub(super) const RING: f32 = 2.0;

/// The pointer and the keys on a part, and its hover fade.
pub(super) struct Hot {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub now: Instant,
    lit: Animation<bool>,
}

impl Hot {
    pub fn new() -> Self {
        Self {
            hovered: false,
            pressed: false,
            focused: false,
            now: Instant::now(),
            lit: Animation::new(false),
        }
    }

    /// Moves the fade to the frame at `now`; whether another frame is due.
    pub fn advance(&mut self, now: Instant, lit: bool, motion: Motion) -> bool {
        self.now = now;
        self.lit = if motion.still() {
            Animation::new(lit)
        } else if self.lit.value() != lit {
            self.lit
                .clone()
                .duration(QUICK)
                .easing(Easing::Linear)
                .go(lit, now)
        } else {
            self.lit.clone()
        };
        self.lit.is_animating(now)
    }

    /// How lit it is, 0 at rest and 1 under the pointer.
    pub fn amount(&self) -> f32 {
        self.lit.interpolate(0.0f32, 1.0, self.now)
    }
}

impl Focusable for Hot {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// `from` at 0 and `to` at 1. A colour with no alpha has no hue of its own,
/// so a fill that appears grows from its own colour, not from black.
pub fn mix(from: Color, to: Color, amount: f32) -> Color {
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
    let blend = |x: f32, y: f32| x + (y - x) * amount;
    Color {
        r: blend(a.r, b.r),
        g: blend(a.g, b.g),
        b: blend(a.b, b.b),
        a: blend(a.a, b.a),
    }
}

/// A rounded quad in one colour, with no border.
pub(super) fn fill(renderer: &mut Renderer, bounds: Rectangle, radius: f32, colour: Color) {
    if colour.a <= 0.0 {
        return;
    }
    renderer.fill_quad(
        Quad {
            bounds,
            border: Border::default().rounded(radius),
            ..Quad::default()
        },
        colour,
    );
}

/// A ring of `width` inside `bounds`.
pub(super) fn ring(
    renderer: &mut Renderer,
    bounds: Rectangle,
    radius: f32,
    width: f32,
    colour: Color,
) {
    renderer.fill_quad(
        Quad {
            bounds,
            border: Border::default().rounded(radius).width(width).color(colour),
            ..Quad::default()
        },
        Color::TRANSPARENT,
    );
}

/// An icon in one colour at `at`, `side` square. tiny-skia keeps only the
/// rgb of an svg's colour, so the alpha goes by opacity.
pub(super) fn glyph(
    renderer: &mut Renderer,
    icon: Icon,
    at: Point,
    side: f32,
    ink: Color,
    viewport: &Rectangle,
) {
    let mut svg = vector::Svg::new(icon.handle());
    svg.color = Some(Color { a: 1.0, ..ink });
    svg.opacity = ink.a;
    renderer.draw_svg(svg, Rectangle::new(at, Size::new(side, side)), *viewport);
}

/// The easing the knobs glide on (`Theme.glide`).
pub(super) const GLIDE: Easing = anomi_theme::motion::GLIDE;
/// The easing of the quick fades.
pub(super) const FADE: Easing = ARRIVE;
