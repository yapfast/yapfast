//! A tile moving to its new place: `StageTile`'s `animate x, y, width,
//! height { duration: Theme.slide; easing: Theme.glide; }`
//! (`ui/voice.slint:772`), none full screen.
//!
//! A place changes in a layout, which has no clock; the glide starts at the
//! next frame's time, as `RedrawRequested` brings it, and is laid out again
//! at each frame's until it lands. A tile new to the grid, hidden or shown
//! again, a room that changed size around the same tiles and a still window
//! go to their place at once: only what the person did moves the tiles (a
//! tile shown large, its others hidden, someone coming or going), with the
//! room too when it changes with it, as a DM's call does.

use std::time::Instant;

use iced::{Point, Rectangle, Size};

use anomi_theme::motion::{GLIDE, SLIDE};

/// One tile's way to its place.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Glide {
    from: Rectangle,
    to: Rectangle,
    /// When it set off: `None` landed, or waiting for a frame to start.
    start: Option<Instant>,
    waiting: bool,
    placed: bool,
}

impl Glide {
    /// Where it is at `now`.
    pub fn at(&self, now: Option<Instant>) -> Rectangle {
        match (self.start, now) {
            (Some(start), Some(now)) => {
                let done = now.saturating_duration_since(start).as_secs_f32() / SLIDE.as_secs_f32();
                between(self.from, self.to, GLIDE.value(done.min(1.0)))
            }
            _ if self.waiting => self.from,
            _ => self.to,
        }
    }

    /// Heads for `target`: at once when `snap` or it has no place yet, else
    /// from where it is at `now`.
    pub fn aim(&mut self, target: Rectangle, snap: bool, now: Option<Instant>) {
        if snap || !self.placed {
            *self = Self {
                from: target,
                to: target,
                start: None,
                waiting: false,
                placed: true,
            };
        } else if target != self.to {
            self.from = self.at(now);
            self.to = target;
            self.start = None;
            self.waiting = true;
        }
    }

    /// It moves, or waits for a frame to set off.
    pub fn active(&self) -> bool {
        self.waiting || self.start.is_some()
    }

    /// The frame at `now`: a waiting glide sets off, one done lands. Whether
    /// it still moves after it.
    pub fn frame(&mut self, now: Instant) -> bool {
        if self.waiting {
            self.waiting = false;
            self.start = Some(now);
        }
        if let Some(start) = self.start
            && now.saturating_duration_since(start) >= SLIDE
        {
            self.start = None;
            self.from = self.to;
        }
        self.start.is_some()
    }

    /// Lands where it was going.
    pub fn land(&mut self) {
        self.aim(self.to, true, None);
    }
}

/// From `a` to `b` by `amount`.
fn between(a: Rectangle, b: Rectangle, amount: f32) -> Rectangle {
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    Rectangle::new(
        Point::new(mix(a.x, b.x), mix(a.y, b.y)),
        Size::new(mix(a.width, b.width), mix(a.height, b.height)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LENGTH: std::time::Duration = SLIDE;

    fn rect(x: f32, width: f32) -> Rectangle {
        Rectangle::new(Point::new(x, 0.0), Size::new(width, width * 0.5625))
    }

    #[test]
    fn a_new_tile_lands_at_once() {
        let mut glide = Glide::default();
        glide.aim(rect(10.0, 100.0), false, None);
        assert_eq!(glide.at(None), rect(10.0, 100.0));
        assert!(!glide.active());
    }

    #[test]
    fn a_moved_tile_glides_from_the_next_frame() {
        let mut glide = Glide::default();
        glide.aim(rect(0.0, 100.0), false, None);
        let start = Instant::now();
        glide.aim(rect(200.0, 300.0), false, Some(start));
        assert!(glide.active());
        assert_eq!(glide.at(Some(start)), rect(0.0, 100.0), "waits for a frame");
        assert!(glide.frame(start));
        let half = glide.at(Some(start + LENGTH / 2));
        assert!((half.x - 100.0).abs() < 0.5, "half way: {half:?}");
        assert!((half.width - 200.0).abs() < 0.5);
        assert!(!glide.frame(start + LENGTH), "lands");
        assert_eq!(glide.at(Some(start + LENGTH)), rect(200.0, 300.0));
        assert!(!glide.active());
    }

    #[test]
    fn a_glide_turned_round_sets_off_from_where_it_is() {
        let mut glide = Glide::default();
        glide.aim(rect(0.0, 100.0), false, None);
        let start = Instant::now();
        glide.aim(rect(200.0, 100.0), false, Some(start));
        glide.frame(start);
        let middle = start + LENGTH / 2;
        let there = glide.at(Some(middle));
        glide.aim(rect(0.0, 100.0), false, Some(middle));
        assert_eq!(glide.at(Some(middle)), there);
        glide.aim(rect(300.0, 100.0), true, Some(middle));
        assert_eq!(glide.at(Some(middle)), rect(300.0, 100.0), "snapped");
        glide.aim(rect(0.0, 100.0), false, Some(middle));
        glide.land();
        assert_eq!(glide.at(None), rect(0.0, 100.0));
        assert!(!glide.active());
    }
}
