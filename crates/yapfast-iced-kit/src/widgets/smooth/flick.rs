//! Mouse wheel motion based on Slint's `Flickable` and direct touchpad scrolling.
//!
//! A wheel notch moves the list on Flutter's power curve. A notch while
//! one moves adds what was left of it. A wheel spun fast moves farther
//! for each notch. A touchpad event applies its pixel delta immediately.
//!
//! Distances here are how far the list moves its view, down or right
//! positive, from where it starts.

use std::time::{Duration, Instant};

use iced::{Point, Vector};

/// How far a wheel's notch scrolls (Slint's winit backend's).
pub const WHEEL_NOTCH: f32 = 60.0;
/// How long a notch's move takes.
pub const WHEEL_DURATION: Duration = Duration::from_millis(180);
/// Notches a second past which each notch scrolls farther, in proportion.
const ACCELERATION_START: f32 = 12.0;
/// The most a notch scales.
const MOST_ACCELERATION: f32 = 6.0;
/// How long a notch counts towards the wheel's speed: its weight falls by
/// e in this time.
const SPEED_MEMORY: Duration = Duration::from_millis(200);
/// How long after it scrolled a list keeps the wheel from what it holds,
/// while the pointer stays put.
const SCROLL_FILTER: Duration = Duration::from_millis(800);
/// How far, squared, the pointer may move and still count as put.
const FILTER_DISTANCE_SQUARED: f32 = 4.0;
/// How far a list is scrolled on one axis, and the farthest it goes
/// (none, or less, when what it holds fits).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Room {
    pub offset: f32,
    pub max: f32,
}

impl Room {
    fn end(self) -> f32 {
        self.max.max(0.0)
    }

    /// Moved by `by`, held to its ends.
    fn moved(self, by: f32) -> f32 {
        (self.offset + by).clamp(0.0, self.end()) - self.offset
    }
}

/// A list's room on both axes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rooms {
    pub x: Room,
    pub y: Room,
}

/// One axis's wheel motion on Flutter's power curve.
#[derive(Clone, Copy, Debug)]
struct Glide {
    start: Instant,
    distance: f32,
    traveled: f32,
    done: bool,
}

impl Glide {
    /// Covers `distance` in [`WHEEL_DURATION`], fast at first.
    fn wheel(start: Instant, distance: f32) -> Self {
        Self {
            start,
            distance,
            traveled: 0.0,
            done: false,
        }
    }

    fn sample(&self, elapsed: Duration) -> f32 {
        if elapsed >= WHEEL_DURATION {
            return self.distance;
        }
        let t = elapsed.as_secs_f32() / WHEEL_DURATION.as_secs_f32();
        // Android's deceleration rate, ln(0.78) / ln(0.9).
        let rate = 0.78f32.log10() / 0.9f32.log10();
        self.distance * (1.0 - (1.0 - t).powf(rate))
    }

    /// What's left of it at `now`.
    fn remaining(&self, now: Instant) -> f32 {
        self.distance - self.sample(now.saturating_duration_since(self.start))
    }

    /// The move from where it was to `now`, held to the end it goes
    /// towards, which ends it. Applied as a step, not a place, so what
    /// moves the list meanwhile (rows measured above) adds to it.
    fn step(&mut self, now: Instant, room: Room) -> f32 {
        if self.done {
            return 0.0;
        }
        let elapsed = now.saturating_duration_since(self.start);
        let position = self.sample(elapsed);
        let mut by = position - self.traveled;
        self.traveled = position;
        let to = room.offset + by;
        let (limit, past) = if self.distance >= 0.0 {
            (room.end(), to >= room.end())
        } else {
            (0.0, to <= 0.0)
        };
        if past {
            by = limit - room.offset;
        }
        self.done = past || elapsed >= WHEEL_DURATION;
        by
    }
}

/// What a wheel event did: whether the list took it, so it goes no
/// further, and how far it moves the list at once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome {
    pub taken: bool,
    pub moved: Vector,
}

impl Outcome {
    const IGNORED: Self = Self {
        taken: false,
        moved: Vector::ZERO,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollKind {
    Wheel,
    Touchpad,
}

/// A list's motion under the wheel and the touchpad (Slint's
/// `FlickableDataInner`), fed the wheel's events and the window's frames.
#[derive(Clone, Debug, Default)]
pub struct Flick {
    /// The moves running, across and down.
    glides: [Option<Glide>; 2],
    /// When it last scrolled, where the pointer was, and what moved it.
    last_scroll: Option<(Instant, Point, ScrollKind)>,
    /// When the wheel's last notch came, the recent notches, each weighing
    /// less with time, and the way they turned.
    notches: Option<(Instant, f32, Vector)>,
}

/// Whether `delta` goes along an axis the list can scroll.
fn allowed(delta: Vector, rooms: Rooms) -> bool {
    (delta.y != 0.0 && rooms.y.max > 0.0) || (delta.x != 0.0 && rooms.x.max > 0.0)
}

impl Flick {
    /// Whether it moves on its own, asking for frames.
    pub fn moving(&self) -> bool {
        self.glides.iter().any(Option::is_some)
    }

    /// Stops where it is: a press on it, or the list put somewhere.
    pub fn stop(&mut self) {
        self.glides = [None, None];
    }

    /// Keeps scrolling this list while the pointer stays where it scrolled
    /// recently, even if a smaller scrollable moves under the pointer.
    pub fn intercepts(
        &mut self,
        now: Instant,
        delta: Vector,
        at: Point,
        rooms: Rooms,
        kind: ScrollKind,
    ) -> bool {
        if !allowed(delta, rooms) {
            self.last_scroll = None;
            return false;
        }
        self.last_scroll.is_some_and(|(time, from, previous)| {
            let moved = at - from;
            kind == previous
                && now.saturating_duration_since(time) < SCROLL_FILTER
                && moved.x * moved.x + moved.y * moved.y < FILTER_DISTANCE_SQUARED
        })
    }

    /// Animates a mouse wheel notch, carrying over the distance still queued.
    pub fn wheel(&mut self, now: Instant, delta: Vector, at: Point, rooms: Rooms) -> Outcome {
        if !allowed(delta, rooms) {
            return Outcome::IGNORED;
        }
        let mut delta = self.accelerate(now, delta);
        let moved = self.frame(now, rooms);
        if let Some(glide) = &self.glides[0] {
            delta.x += glide.remaining(now);
        }
        if let Some(glide) = &self.glides[1] {
            delta.y += glide.remaining(now);
        }
        let glide = |distance: f32| (distance != 0.0).then(|| Glide::wheel(now, distance));
        self.glides = [glide(delta.x), glide(delta.y)];
        self.last_scroll = Some((now, at, ScrollKind::Wheel));
        Outcome { taken: true, moved }
    }

    /// Applies this pixel delta and cancels any queued wheel motion.
    pub fn touchpad(&mut self, now: Instant, delta: Vector, at: Point, rooms: Rooms) -> Outcome {
        if !allowed(delta, rooms) {
            return Outcome::IGNORED;
        }
        self.stop();
        self.notches = None;
        self.last_scroll = Some((now, at, ScrollKind::Touchpad));
        let moved = Vector::new(rooms.x.moved(delta.x), rooms.y.moved(delta.y));
        Outcome { taken: true, moved }
    }

    /// Lets the next touchpad gesture go to the list's contents first.
    pub fn end_touchpad(&mut self) {
        if let Some((_, _, ScrollKind::Touchpad)) = self.last_scroll {
            self.last_scroll = None;
        }
    }

    /// The move from the last frame to `now`'s; the moves that reach their
    /// time or an end stop.
    pub fn frame(&mut self, now: Instant, rooms: Rooms) -> Vector {
        let mut by = Vector::ZERO;
        for (index, room) in [rooms.x, rooms.y].into_iter().enumerate() {
            let Some(glide) = &mut self.glides[index] else {
                continue;
            };
            let step = glide.step(now, room);
            if glide.done {
                self.glides[index] = None;
            }
            if index == 0 {
                by.x = step;
            } else {
                by.y = step;
            }
        }
        by
    }

    /// Scales a line delta by how fast the wheel spins.
    fn accelerate(&mut self, now: Instant, delta: Vector) -> Vector {
        let notches = (delta.x * delta.x + delta.y * delta.y).sqrt() / WHEEL_NOTCH;
        if notches < 1.0 {
            self.notches = None;
            return delta;
        }
        let earlier = match self.notches {
            // A turn the other way starts afresh.
            Some((time, count, way)) if way.x * delta.x + way.y * delta.y > 0.0 => {
                let age = now.saturating_duration_since(time).as_secs_f32();
                count * (-age / SPEED_MEMORY.as_secs_f32()).exp()
            }
            _ => 0.0,
        };
        let count = earlier + notches;
        self.notches = Some((now, count, delta));
        let speed = count / SPEED_MEMORY.as_secs_f32();
        delta * (speed / ACCELERATION_START).clamp(1.0, MOST_ACCELERATION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Point = Point::new(100.0, 100.0);

    fn rooms(offset: f32) -> Rooms {
        Rooms {
            x: Room::default(),
            y: Room {
                offset,
                max: 10_000.0,
            },
        }
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    /// Runs frames every 16 ms from `from` to `to` after `start`, moving a
    /// list from `offset`; where it ends.
    fn run(flick: &mut Flick, start: Instant, from: u64, to: u64, mut offset: f32) -> f32 {
        let mut at = from;
        while at <= to {
            offset += flick.frame(start + ms(at), rooms(offset)).y;
            at += 16;
        }
        offset
    }

    fn notch(flick: &mut Flick, now: Instant, offset: f32) -> Outcome {
        flick.wheel(now, Vector::new(0.0, WHEEL_NOTCH), AT, rooms(offset))
    }

    #[test]
    fn a_notch_reaches_its_distance_after_its_time_eased() {
        let start = Instant::now();
        let mut flick = Flick::default();
        let outcome = notch(&mut flick, start, 0.0);
        assert!(outcome.taken);
        assert_eq!(outcome.moved, Vector::ZERO, "nothing moves at once");
        assert!(flick.moving());
        let early = flick.frame(start + ms(30), rooms(0.0)).y;
        assert!(early > 0.0 && early < WHEEL_NOTCH, "{early}");
        // Fast at first: past half the way in a third of the time.
        let third = early + flick.frame(start + ms(60), rooms(early)).y;
        assert!(third > WHEEL_NOTCH / 2.0, "{third}");
        let end = third + flick.frame(start + WHEEL_DURATION, rooms(third)).y;
        assert!((end - WHEEL_NOTCH).abs() < 0.001, "{end}");
        assert!(!flick.moving(), "it stops at its time");
        assert_eq!(flick.frame(start + ms(400), rooms(end)), Vector::ZERO);
    }

    #[test]
    fn a_second_notch_mid_move_adds_what_was_left() {
        let start = Instant::now();
        let mut flick = Flick::default();
        notch(&mut flick, start, 0.0);
        let offset = run(&mut flick, start, 16, 80, 0.0);
        assert!(offset > 0.0 && offset < WHEEL_NOTCH);
        // 90 ms on, too slow to speed up, while the first still moves.
        let outcome = notch(&mut flick, start + ms(90), offset);
        let offset = offset + outcome.moved.y;
        let end = run(&mut flick, start + ms(90), 16, 400, offset);
        assert!((end - 2.0 * WHEEL_NOTCH).abs() < 0.01, "{end}");
    }

    #[test]
    fn a_wheel_spun_fast_scrolls_farther_each_notch() {
        // Ten notches 250 ms apart, and ten 10 ms apart (100 a second),
        // with frames between.
        let spin = |gap: u64| {
            let start = Instant::now();
            let mut flick = Flick::default();
            let mut offset = 0.0;
            for index in 0..10u64 {
                let now = start + ms(index * gap);
                offset += notch(&mut flick, now, offset).moved.y;
                offset += flick.frame(now + ms(gap / 2), rooms(offset)).y;
            }
            run(&mut flick, start + ms(10 * gap), 0, 400, offset)
        };
        let slow = spin(250);
        let fast = spin(10);
        assert!((slow - 10.0 * WHEEL_NOTCH).abs() < 0.01, "slow: {slow}");
        assert!(fast > 1.8 * slow, "fast {fast} against slow {slow}");
        assert!(
            fast <= 10.0 * 6.0 * WHEEL_NOTCH,
            "at most six times a notch: {fast}"
        );
    }

    #[test]
    fn a_notch_stops_at_the_end() {
        let start = Instant::now();
        let mut flick = Flick::default();
        let room = |offset| Rooms {
            x: Room::default(),
            y: Room {
                offset,
                max: 1030.0,
            },
        };
        flick.wheel(start, Vector::new(0.0, 60.0), AT, room(1000.0));
        let mut offset = 1000.0;
        for frame in 1..20 {
            offset += flick.frame(start + ms(frame * 16), room(offset)).y;
        }
        assert_eq!(offset, 1030.0);
        assert!(!flick.moving());
    }

    #[test]
    fn a_stop_drops_the_move_and_the_next_notch_starts_afresh() {
        let start = Instant::now();
        let mut flick = Flick::default();
        notch(&mut flick, start, 0.0);
        let offset = run(&mut flick, start, 16, 32, 0.0);
        // The list was put somewhere (scroll_to): what the notch had left goes.
        flick.stop();
        assert!(!flick.moving());
        notch(&mut flick, start + ms(400), 500.0);
        let end = run(&mut flick, start + ms(400), 16, 400, 500.0);
        assert!(
            (end - 500.0 - WHEEL_NOTCH).abs() < 0.001,
            "{end} after {offset}"
        );
    }

    #[test]
    fn a_wheel_across_a_list_that_only_goes_down_is_not_its() {
        let start = Instant::now();
        let mut flick = Flick::default();
        let outcome = flick.wheel(start, Vector::new(60.0, 0.0), AT, rooms(0.0));
        assert!(!outcome.taken);
        assert!(!flick.moving());
        // While the pointer stays where it scrolled, the list keeps the wheel.
        notch(&mut flick, start, 0.0);
        let delta = Vector::new(0.0, 60.0);
        let kind = ScrollKind::Wheel;
        assert!(flick.intercepts(start + ms(500), delta, AT, rooms(0.0), kind));
        let moved = Point::new(AT.x + 3.0, AT.y);
        assert!(!flick.intercepts(start + ms(500), delta, moved, rooms(0.0), kind));
        assert!(!flick.intercepts(start + ms(900), delta, AT, rooms(0.0), kind));
    }

    #[test]
    fn a_touchpad_release_lets_the_next_gesture_reach_nested_content() {
        let now = Instant::now();
        let mut flick = Flick::default();
        let delta = Vector::new(0.0, 2.0);
        let kind = ScrollKind::Touchpad;

        flick.touchpad(now, delta, AT, rooms(0.0));
        assert!(flick.intercepts(now, delta, AT, rooms(2.0), kind));
        flick.end_touchpad();
        assert!(!flick.intercepts(now, delta, AT, rooms(2.0), kind));
    }
}
