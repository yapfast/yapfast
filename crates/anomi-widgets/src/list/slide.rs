//! Rows sliding to their new places in a list that makes room for a
//! dragged row ([`List::live`](super::List::live)): a layout that puts a row
//! somewhere else than it was drawn starts it where it was drawn, and it
//! glides there over [`SLIDE`], as icons make room on a phone's home
//! screen. Only drawing moves: the pointer meets each row at its place.
//! With a still [`Motion`](anomi_theme::Motion) rows jump, and behind
//! another window they land at once.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use iced::animation::Animation;

use anomi_theme::motion::ARRIVE;

/// A move shorter than this is no move.
const LEAST: f32 = 0.5;

#[derive(Debug, Default)]
pub struct Slides {
    /// Each sliding row's distance from its place, by key: down when
    /// positive, going to nothing.
    rows: HashMap<u64, Animation<f32>>,
    /// The frame they're drawn at.
    now: Option<Instant>,
}

impl Slides {
    /// How far from its place row `key` is drawn now.
    pub fn offset(&self, key: u64) -> f32 {
        match (self.rows.get(&key), self.now) {
            (Some(slide), Some(now)) => slide.interpolate(now),
            (Some(slide), None) => slide.interpolate(Instant::now()),
            _ => 0.0,
        }
    }

    /// A layout put the rows `keys` at `tops`, where `drawn` says where
    /// each was drawn before, if it was: those that moved slide from there.
    pub fn moved(
        &mut self,
        drawn: &HashMap<u64, f32>,
        keys: &[u64],
        tops: &[f32],
        duration: Duration,
        now: Instant,
    ) {
        if duration.is_zero() {
            self.rows.clear();
            return;
        }
        for (key, top) in keys.iter().zip(tops) {
            if let Some(was) = drawn.get(key) {
                self.start(*key, was - top, duration, now);
            }
        }
    }

    /// Row `key` starts `from` its place and slides there.
    pub fn start(&mut self, key: u64, from: f32, duration: Duration, now: Instant) {
        if duration.is_zero() || from.abs() < LEAST {
            return;
        }
        let slide = Animation::new(from)
            .duration(duration)
            .easing(ARRIVE)
            .go(0.0, now);
        self.rows.insert(key, slide);
    }

    /// Takes the frame at `now`: those that arrived stop. Whether any still
    /// slides, for the next frame.
    pub fn step(&mut self, now: Instant) -> bool {
        self.now = Some(now);
        self.rows.retain(|_, slide| slide.is_animating(now));
        !self.rows.is_empty()
    }

    /// Every row lands where it is.
    pub fn land(&mut self) {
        self.rows.clear();
    }

    #[cfg(test)]
    pub fn sliding(&self) -> bool {
        !self.rows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_moved_by_a_layout_slides_from_where_it_was_drawn() {
        let mut slides = Slides::default();
        let start = Instant::now();
        let drawn = HashMap::from([(1, 0.0), (2, 40.0)]);
        // Row 2 went above row 1; row 3 is new and shows at its place.
        slides.moved(
            &drawn,
            &[2, 1, 3],
            &[0.0, 40.0, 80.0],
            Duration::from_millis(180),
            start,
        );
        assert!(slides.step(start));
        assert_eq!(slides.offset(2), 40.0, "drawn where it was");
        assert_eq!(slides.offset(1), -40.0);
        assert_eq!(slides.offset(3), 0.0);
        assert!(!slides.step(start + Duration::from_millis(200)), "arrived");
        assert_eq!(slides.offset(2), 0.0);
    }

    #[test]
    fn still_rows_jump() {
        let mut slides = Slides::default();
        let drawn = HashMap::from([(1, 0.0)]);
        slides.moved(&drawn, &[1], &[40.0], Duration::ZERO, Instant::now());
        assert!(!slides.sliding());
    }
}
