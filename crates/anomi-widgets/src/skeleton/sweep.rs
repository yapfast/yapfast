//! The light band across a skeleton: where it is at a time, and the one
//! gradient quad it is drawn as, which follows a rounded corner.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::border::Radius;
use iced::{Color, Gradient, Radians, Rectangle, gradient};

use anomi_theme::Renderer;

/// One sweep takes this long: `1500` in `animation-tick() / 1ms % 1500`.
pub const PERIOD: Duration = Duration::from_millis(1500);
/// The band is half the skeleton wide, and at least this wide.
const MIN_BAND: f32 = 120.0;

/// The one clock every skeleton sweeps by.
fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

/// Where the band's left edge is and how wide it is, for a skeleton `width`
/// wide at `now`: it enters at the left, wholly out of sight, and leaves at
/// the right.
pub fn band(width: f32, now: Instant) -> (f32, f32) {
    let ms = now.saturating_duration_since(epoch()).as_millis() as u64 % PERIOD.as_millis() as u64;
    let phase = ms as f32 / PERIOD.as_millis() as f32;
    let span = MIN_BAND.max(width * 0.5);
    (phase * (width + span) - span, span)
}

/// How lit the band is at a place along it: 0 at its edges, 1 in the middle.
pub fn light(at: f32) -> f32 {
    1.0 - (2.0 * at - 1.0).abs()
}

/// The gradient stops of the band at `left` and `span` over a skeleton
/// `width` wide: each is an offset along the skeleton (0 to 1) and how lit
/// the band is there. None when the band is out of sight.
pub fn stops(width: f32, left: f32, span: f32) -> Option<Stops> {
    let (lo, hi) = (left.max(0.0).min(width), (left + span).max(0.0).min(width));
    if lo >= hi {
        return None;
    }
    let at = |x: f32| (x - left) / span;
    let offset = |x: f32| (x / width).clamp(0.0, 1.0);
    let mut all = Stops {
        all: [(0.0, 0.0); 3],
        len: 0,
    };
    all.push((offset(lo), light(at(lo))));
    let middle = left + span / 2.0;
    if middle > lo && middle < hi {
        all.push((offset(middle), 1.0));
    }
    all.push((offset(hi), light(at(hi))));
    Some(all)
}

/// Up to three stops, held inline: drawing a frame allocates nothing.
#[derive(Debug, Clone, Copy)]
pub struct Stops {
    all: [(f32, f32); 3],
    len: usize,
}

impl Stops {
    fn push(&mut self, stop: (f32, f32)) {
        self.all[self.len] = stop;
        self.len += 1;
    }

    pub fn as_slice(&self) -> &[(f32, f32)] {
        &self.all[..self.len]
    }
}

/// Draws the band over a skeleton at `bounds`: one quad as big as the
/// skeleton with its corners, filled by a left-to-right gradient (CSS
/// `90deg`) that is clear at the band's edges and brightest at its middle;
/// the gradient's ends carry over the rest.
pub fn draw(renderer: &mut Renderer, bounds: Rectangle, radius: f32, shimmer: Color, now: Instant) {
    let (left, span) = band(bounds.width, now);
    let Some(stops) = stops(bounds.width, left, span) else {
        return;
    };
    let mut line = gradient::Linear::new(Radians(std::f32::consts::FRAC_PI_2));
    for &(offset, light) in stops.as_slice() {
        line = line.add_stop(
            offset,
            Color {
                a: shimmer.a * light,
                ..shimmer
            },
        );
    }
    renderer.fill_quad(
        Quad {
            bounds,
            border: iced::Border {
                radius: Radius::new(radius),
                ..iced::Border::default()
            },
            ..Quad::default()
        },
        Gradient::Linear(line),
    );
}
