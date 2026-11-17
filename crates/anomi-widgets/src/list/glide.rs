//! Wheel scrolling: each notch glides the list on over a short ease-out, a
//! notch while one glides adds to what's left of it, and a wheel spun fast
//! goes farther a notch. The Slint side: `process_wheel_event`,
//! `accelerate_wheel` and `WHEEL_SCROLL_DURATION` in
//! `vendor/i-slint-core/items/flickable.rs`; its 180 ms is
//! [`anomi_theme::motion::SLIDE`].

use std::time::{Duration, Instant};

use anomi_theme::motion::ARRIVE;

/// How far one notch of a wheel scrolls, in logical pixels.
pub const NOTCH: f32 = 60.0;
/// A wheel spun faster than this, in notches a second, scrolls farther a
/// notch, in proportion.
const ACCELERATION_START: f32 = 12.0;
/// The most a notch scales.
const MOST_ACCELERATION: f32 = 6.0;
/// How long a notch counts towards the wheel's speed: its weight falls by e
/// in this time.
const SPEED_MEMORY: Duration = Duration::from_millis(200);

/// The wheel's recent notches, for its speed.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wheel {
    /// When the last notch came, how many recent ones count, and their way.
    notches: Option<(Instant, f32, f32)>,
}

impl Wheel {
    /// Scales `pixels` (whole notches of [`NOTCH`]) by how fast the wheel
    /// spins. A turn the other way starts afresh.
    pub fn accelerate(&mut self, pixels: f32, now: Instant) -> f32 {
        let notches = pixels.abs() / NOTCH;
        if notches < 1.0 {
            self.notches = None;
            return pixels;
        }
        let earlier = match self.notches {
            Some((time, count, way)) if way * pixels > 0.0 => {
                let age = now.saturating_duration_since(time).as_secs_f32();
                count * (-age / SPEED_MEMORY.as_secs_f32()).exp()
            }
            _ => 0.0,
        };
        let count = earlier + notches;
        self.notches = Some((now, count, pixels.signum()));
        let speed = count / SPEED_MEMORY.as_secs_f32();
        pixels * (speed / ACCELERATION_START).clamp(1.0, MOST_ACCELERATION)
    }
}

/// A glide under way: `total` pixels over `duration` from `started`, of
/// which `done` are scrolled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glide {
    started: Instant,
    duration: Duration,
    total: f32,
    done: f32,
}

impl Glide {
    pub fn new(total: f32, duration: Duration, now: Instant) -> Self {
        Self {
            started: now,
            duration,
            total,
            done: 0.0,
        }
    }

    /// What's still to scroll.
    pub fn remaining(&self) -> f32 {
        self.total - self.done
    }

    /// Adds `more` to what's left, gliding it all from `now`.
    pub fn extend(&mut self, more: f32, now: Instant) {
        *self = Self::new(self.remaining() + more, self.duration, now);
    }

    /// How far to scroll for the frame at `now`, and whether that ends it.
    pub fn step(&mut self, now: Instant) -> (f32, bool) {
        let elapsed = now.saturating_duration_since(self.started).as_secs_f32();
        let length = self.duration.as_secs_f32();
        let progress = if length <= 0.0 {
            1.0
        } else {
            (elapsed / length).clamp(0.0, 1.0)
        };
        let reached = self.total * ease(progress);
        let delta = reached - self.done;
        self.done = reached;
        (delta, progress >= 1.0)
    }
}

/// [`ARRIVE`]'s curve: quick at first, then settling, as a flick
/// decelerates; exactly the whole distance at the end.
fn ease(x: f32) -> f32 {
    if x >= 1.0 { 1.0 } else { ARRIVE.value(x) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glide_covers_its_distance_and_a_notch_adds_to_what_is_left() {
        let start = Instant::now();
        let mut glide = Glide::new(120.0, Duration::from_millis(180), start);
        let (first, done) = glide.step(start + Duration::from_millis(30));
        assert!(first > 30.0, "ease-out goes fast first: {first}");
        assert!(!done);
        glide.extend(60.0, start + Duration::from_millis(30));
        assert!((glide.remaining() - (180.0 - first)).abs() < 1e-3);
        let mut moved = first;
        let (rest, done) = glide.step(start + Duration::from_secs(1));
        moved += rest;
        assert!(done);
        assert!((moved - 180.0).abs() < 1e-3);
    }

    #[test]
    fn a_wheel_spun_fast_scrolls_farther_a_notch() {
        let start = Instant::now();
        let mut wheel = Wheel::default();
        assert_eq!(wheel.accelerate(-NOTCH, start), -NOTCH);
        let mut last = 0.0;
        for step in 1..8 {
            last = wheel.accelerate(-NOTCH, start + Duration::from_millis(10 * step));
        }
        assert!(last < -NOTCH * 2.0, "spun fast: {last}");
        assert!(last >= -NOTCH * MOST_ACCELERATION);
        // The other way starts afresh, and a touchpad's small steps keep their pace.
        assert_eq!(
            wheel.accelerate(NOTCH, start + Duration::from_millis(90)),
            NOTCH
        );
        assert_eq!(
            wheel.accelerate(3.0, start + Duration::from_millis(91)),
            3.0
        );
    }
}
