//! How a list moves under the wheel and the touchpad: Slint's `Flickable`
//! (`items/flickable.rs` and its simulations in i-slint-core) with its
//! constants and Yapfast's changes to it, for the platforms Yapfast runs on
//! (no rubber band at the ends, no momentum carried into the next gesture).
//!
//! A wheel's notch moves the list its distance over a fixed time on
//! Flutter's power curve; a notch while one moves adds what was left of
//! it; a wheel spun fast moves farther for each notch. A touchpad moves
//! the list with the fingers, and once they lift it goes on at their speed
//! and slows to a stop as Android's `OverScroller` does.
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
/// What the fingers move first, which the list doesn't follow, so it
/// starts without a jump.
const DISTANCE_THRESHOLD: f32 = 8.0;
/// How long after it scrolled a list keeps the wheel from what it holds,
/// while the pointer stays put.
const SCROLL_FILTER: Duration = Duration::from_millis(800);
/// How long after it scrolled a list takes a wheel that moves it no more.
const SHORT_SCROLL_FILTER: Duration = Duration::from_millis(400);
/// How far, squared, the pointer may move and still count as put.
const FILTER_DISTANCE_SQUARED: f32 = 4.0;
/// The slowest the fingers may lift and the list go on, in px a second.
const SLOWEST_FLING: f32 = 50.0;
/// The fling's friction.
const FRICTION: f32 = 0.015;
/// A lift this long after the fingers' last move finds them stopped.
const STOPPED: Duration = Duration::from_millis(40);

/// Android's deceleration rate, ln 0.78 / ln 0.9: the power of the wheel's
/// curve and the fling's spline.
fn rate() -> f32 {
    0.78f32.log10() / 0.9f32.log10()
}

/// What the touchpad is doing as an event comes: its fingers went down,
/// move, or lifted. A wheel's notch moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Started,
    Moved,
    Ended,
}

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

/// One axis's move over time: a wheel's on its curve or a fling's on
/// Android's spline (Slint's `AndroidFlick`).
#[derive(Clone, Copy, Debug)]
struct Glide {
    start: Instant,
    duration: Duration,
    distance: f32,
    velocity: f32,
    traveled: f32,
    wheel: bool,
    done: bool,
}

impl Glide {
    /// Covers `distance` in [`WHEEL_DURATION`], fast at first.
    fn wheel(start: Instant, distance: f32) -> Self {
        Self {
            start,
            duration: WHEEL_DURATION,
            distance,
            velocity: distance * rate() / WHEEL_DURATION.as_secs_f32(),
            traveled: 0.0,
            wheel: true,
            done: false,
        }
    }

    /// Goes on at `velocity` and slows to a stop.
    fn fling(start: Instant, velocity: f32) -> Self {
        let (duration, distance) = spline::parameters(velocity, FRICTION);
        Self {
            start,
            duration,
            distance,
            velocity,
            traveled: 0.0,
            wheel: false,
            done: false,
        }
    }

    /// Where it is `elapsed` after it started, and how fast it goes.
    fn sample(&self, elapsed: Duration) -> (f32, f32) {
        if self.done || elapsed >= self.duration {
            return (self.distance, 0.0);
        }
        if elapsed.is_zero() {
            return (0.0, self.velocity);
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        if self.wheel {
            let rate = rate();
            (
                self.distance * (1.0 - (1.0 - t).powf(rate)),
                self.velocity * (1.0 - t).powf(rate - 1.0),
            )
        } else {
            let (position, slope) = spline::sample(t);
            (
                position * self.distance,
                slope * self.distance / self.duration.as_secs_f32(),
            )
        }
    }

    /// What's left of it at `now`.
    fn remaining(&self, now: Instant) -> f32 {
        self.distance - self.sample(now.saturating_duration_since(self.start)).0
    }

    /// The move from where it was to `now`, held to the end it goes
    /// towards, which ends it. Applied as a step, not a place, so what
    /// moves the list meanwhile (rows measured above) adds to it.
    fn step(&mut self, now: Instant, room: Room) -> f32 {
        if self.done {
            return 0.0;
        }
        let elapsed = now.saturating_duration_since(self.start);
        let (position, _) = self.sample(elapsed);
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
        self.done = past || elapsed >= self.duration;
        by
    }
}

/// The fingers' last moves, to tell how fast they went as they lifted
/// (Slint's `IOsVelocityTracker`, Flutter's iOS one): the last three
/// moves' speeds blended, mostly the oldest's.
#[derive(Clone, Copy, Debug, Default)]
struct Tracker {
    /// Oldest first.
    samples: [Option<(Instant, Vector)>; 4],
}

impl Tracker {
    const WEIGHTS: [f32; 3] = [0.6, 0.35, 0.05];

    fn push(&mut self, time: Instant, delta: Vector) {
        self.samples.rotate_left(1);
        self.samples[3] = Some((time, delta));
    }

    /// Their speed, in px a second, if they still moved at `release`.
    fn estimate(&self, release: Instant) -> Option<Vector> {
        let (last, _) = self.samples[3]?;
        if release.saturating_duration_since(last) > STOPPED {
            return None;
        }
        let segment = |at: usize| match (self.samples[at], self.samples[at - 1]) {
            (Some((time, delta)), Some((before, _))) => {
                let seconds = time.saturating_duration_since(before).as_secs_f32();
                if seconds > 0.0 {
                    delta * (1.0 / seconds)
                } else {
                    Vector::ZERO
                }
            }
            _ => Vector::ZERO,
        };
        let [oldest, middle, newest] = Self::WEIGHTS;
        Some(segment(1) * oldest + segment(2) * middle + segment(3) * newest)
    }
}

/// What a touchpad gesture over the list has done so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Capture {
    /// The fingers went down.
    Start,
    /// They moved the list.
    Move,
}

/// What a wheel event did: whether the list took it, so it goes no
/// further, and how far it moves the list at once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome {
    pub taken: bool,
    pub moved: Vector,
}

/// A list's motion under the wheel and the touchpad (Slint's
/// `FlickableDataInner`), fed the wheel's events and the window's frames.
#[derive(Clone, Debug, Default)]
pub struct Flick {
    /// The moves running, across and down.
    glides: [Option<Glide>; 2],
    capture: Option<Capture>,
    /// When it last scrolled and where the pointer was.
    last_scroll: Option<(Instant, Point)>,
    tracker: Tracker,
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

    /// Whether a wheel event goes to it before what it holds: while the
    /// fingers move it, and while the pointer stays where it last scrolled
    /// it, so a list scrolling past a smaller one under the pointer keeps
    /// the wheel.
    pub fn intercepts(
        &mut self,
        now: Instant,
        phase: Phase,
        delta: Vector,
        at: Point,
        rooms: Rooms,
    ) -> bool {
        match phase {
            Phase::Started => false,
            Phase::Ended => self.capture == Some(Capture::Move),
            Phase::Moved => {
                if self.capture == Some(Capture::Move) {
                    return true;
                }
                if !allowed(delta, rooms) {
                    self.last_scroll = None;
                    if self.capture == Some(Capture::Start) {
                        self.capture = None;
                    }
                    return false;
                }
                self.capture.is_none() && self.recently(now, SCROLL_FILTER, at)
            }
        }
    }

    /// A wheel event at `at`, `delta` the way it moves the view (Slint's
    /// `process_wheel_event`).
    pub fn wheel(
        &mut self,
        now: Instant,
        mut delta: Vector,
        phase: Phase,
        at: Point,
        rooms: Rooms,
    ) -> Outcome {
        let ignored = Outcome {
            taken: false,
            moved: Vector::ZERO,
        };
        if phase != Phase::Started && delta != Vector::ZERO && !allowed(delta, rooms) {
            // Not this list's way: it lets go of it.
            *self = Self {
                notches: self.notches,
                ..Self::default()
            };
            return ignored;
        }
        let wheel = self.capture.is_none() && phase == Phase::Moved;
        let mut moved = Vector::ZERO;
        let mut rooms = rooms;
        if wheel {
            delta = self.accelerate(now, delta);
            // The move running catches up with now, and what it left adds
            // to the next.
            moved = self.frame(now, rooms);
            rooms.x.offset += moved.x;
            rooms.y.offset += moved.y;
            if let Some(glide) = &self.glides[0] {
                delta.x += glide.remaining(now);
            }
            if let Some(glide) = &self.glides[1] {
                delta.y += glide.remaining(now);
            }
        }
        if phase != Phase::Ended {
            self.stop();
        }
        match phase {
            Phase::Started => {
                self.tracker = Tracker::default();
                self.capture = Some(Capture::Start);
                self.last_scroll = Some((now, at));
            }
            Phase::Moved if self.capture.is_some() => {
                if self.capture == Some(Capture::Start) {
                    let less = |d: f32| {
                        if d >= 0.0 {
                            (d - DISTANCE_THRESHOLD).max(0.0)
                        } else {
                            (d + DISTANCE_THRESHOLD).min(0.0)
                        }
                    };
                    delta = Vector::new(less(delta.x), less(delta.y));
                }
                self.tracker.push(now, delta);
                moved = Vector::new(rooms.x.moved(delta.x), rooms.y.moved(delta.y));
                self.last_scroll = Some((now, at));
                self.capture = Some(Capture::Move);
            }
            Phase::Moved => {
                let glide = |distance: f32| (distance != 0.0).then(|| Glide::wheel(now, distance));
                self.glides = [glide(delta.x), glide(delta.y)];
                self.last_scroll = Some((now, at));
                return Outcome { taken: true, moved };
            }
            Phase::Ended => {
                if self.capture == Some(Capture::Move) {
                    self.release(now);
                }
                self.capture = None;
                return Outcome {
                    taken: self.recently(now, SHORT_SCROLL_FILTER, at),
                    moved,
                };
            }
        }
        if moved != Vector::ZERO
            || (phase != Phase::Started && self.recently(now, SHORT_SCROLL_FILTER, at))
        {
            Outcome { taken: true, moved }
        } else {
            if phase != Phase::Started {
                self.last_scroll = None;
            }
            ignored
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

    /// The fingers lifted: the list goes on at their speed, if fast enough.
    fn release(&mut self, now: Instant) {
        let velocity = self.tracker.estimate(now).unwrap_or(Vector::ZERO);
        let fling = |speed: f32| (speed.abs() >= SLOWEST_FLING).then(|| Glide::fling(now, speed));
        self.glides = [fling(velocity.x), fling(velocity.y)];
    }

    /// Whether it scrolled within `timeout` with the pointer where it is.
    fn recently(&self, now: Instant, timeout: Duration, at: Point) -> bool {
        self.last_scroll.is_some_and(|(time, from)| {
            let moved = at - from;
            now.saturating_duration_since(time) < timeout
                && moved.x * moved.x + moved.y * moved.y < FILTER_DISTANCE_SQUARED
        })
    }

    /// Scales a wheel's `delta` by how fast the wheel spins. Only whole
    /// notches count: touchpads that send no phases scroll by less at a
    /// time, and keep their own pace.
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

/// Android's `OverScroller` fling spline (android-15.0.0_r1), as Slint's
/// `animations/simulations/android/spline.rs`.
mod spline {
    use std::time::Duration;

    const RATE: f32 = 2.358_201_7;
    const PHYSICAL_COEFFICIENT: f32 = 9.80665 * 39.37 * 160.0 * 0.84;

    /// Where the fling is at each hundredth of its time, of its distance.
    const POSITION: [f32; 101] = {
        let mut table = [0.0; 101];
        let mut i = 0;
        let mut low = 0.0;
        while i < 100 {
            let alpha = i as f32 / 100.0;
            let mut high = 1.0;
            loop {
                let x = (low + high) / 2.0;
                let coefficient = 3.0 * x * (1.0 - x);
                let tx = coefficient * ((1.0 - x) * 0.175 + x * 0.35) + x * x * x;
                let error = tx - alpha;
                if error > -0.00001 && error < 0.00001 {
                    table[i] = coefficient * ((1.0 - x) * 0.5 + x) + x * x * x;
                    break;
                }
                if tx > alpha {
                    high = x;
                } else {
                    low = x;
                }
            }
            i += 1;
        }
        table[100] = 1.0;
        table
    };

    /// How long a fling at `velocity` takes and how far it goes.
    pub fn parameters(velocity: f32, friction: f32) -> (Duration, f32) {
        if velocity == 0.0 || !velocity.is_finite() || !friction.is_finite() || friction <= 0.0 {
            return (Duration::ZERO, 0.0);
        }
        let deceleration =
            f64::from(0.35 * velocity.abs() / (friction * PHYSICAL_COEFFICIENT)).ln();
        let rate = f64::from(RATE);
        let milliseconds = (1000.0 * (deceleration / (rate - 1.0)).exp()) as u64;
        let distance =
            f64::from(friction * PHYSICAL_COEFFICIENT) * (rate / (rate - 1.0) * deceleration).exp();
        (
            Duration::from_millis(milliseconds),
            (distance * f64::from(velocity.signum())) as i32 as f32,
        )
    }

    /// Where it is at `t` of its time, of its distance, and how fast.
    pub fn sample(t: f32) -> (f32, f32) {
        let index = (100.0 * t) as usize;
        if index >= 100 {
            return (1.0, 0.0);
        }
        let lower = index as f32 / 100.0;
        let upper = (index + 1) as f32 / 100.0;
        let slope = (POSITION[index + 1] - POSITION[index]) / (upper - lower);
        (POSITION[index] + (t - lower) * slope, slope)
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
        flick.wheel(
            now,
            Vector::new(0.0, WHEEL_NOTCH),
            Phase::Moved,
            AT,
            rooms(offset),
        )
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
        flick.wheel(
            start,
            Vector::new(0.0, 60.0),
            Phase::Moved,
            AT,
            room(1000.0),
        );
        let mut offset = 1000.0;
        for frame in 1..20 {
            offset += flick.frame(start + ms(frame * 16), room(offset)).y;
        }
        assert_eq!(offset, 1030.0);
        assert!(!flick.moving());
    }

    #[test]
    fn fingers_move_the_list_then_its_fling_slows_and_stops() {
        let start = Instant::now();
        let mut flick = Flick::default();
        let begun = flick.wheel(start, Vector::new(0.0, 3.0), Phase::Started, AT, rooms(0.0));
        assert_eq!(begun.moved, Vector::ZERO, "the first move only begins it");
        let mut offset = 0.0;
        for frame in 1..=10u64 {
            let outcome = flick.wheel(
                start + ms(frame * 10),
                Vector::new(0.0, 20.0),
                Phase::Moved,
                AT,
                rooms(offset),
            );
            assert!(outcome.taken);
            offset += outcome.moved.y;
            assert!(
                !flick.moving(),
                "it follows the fingers, with no frames of its own"
            );
        }
        // 20 px each 10 ms, less the first 8 px.
        assert_eq!(offset, 192.0);
        let lift = start + ms(105);
        flick.wheel(lift, Vector::ZERO, Phase::Ended, AT, rooms(offset));
        assert!(flick.moving(), "2000 px a second goes on");
        let mut last = f32::MAX;
        let mut frame = 1;
        while flick.moving() {
            let step = flick.frame(lift + ms(frame * 16), rooms(offset)).y;
            assert!(
                step >= 0.0 && step <= last + 0.5,
                "slows down: {step} after {last}"
            );
            last = step;
            offset += step;
            frame += 1;
            assert!(frame < 500, "stops");
        }
        assert!(offset > 300.0, "{offset}");
        assert!(last < 2.0, "slow at the end: {last}");
    }

    #[test]
    fn fingers_that_stop_before_lifting_fling_nothing() {
        let start = Instant::now();
        let mut flick = Flick::default();
        flick.wheel(start, Vector::new(0.0, 3.0), Phase::Started, AT, rooms(0.0));
        for frame in 1..=5u64 {
            flick.wheel(
                start + ms(frame * 10),
                Vector::new(0.0, 20.0),
                Phase::Moved,
                AT,
                rooms(0.0),
            );
        }
        flick.wheel(start + ms(200), Vector::ZERO, Phase::Ended, AT, rooms(0.0));
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
        let outcome = flick.wheel(start, Vector::new(60.0, 0.0), Phase::Moved, AT, rooms(0.0));
        assert!(!outcome.taken);
        assert!(!flick.moving());
        // While the pointer stays where it scrolled, the list keeps the wheel.
        notch(&mut flick, start, 0.0);
        assert!(flick.intercepts(
            start + ms(500),
            Phase::Moved,
            Vector::new(0.0, 60.0),
            AT,
            rooms(0.0)
        ));
        let moved = Point::new(AT.x + 3.0, AT.y);
        assert!(!flick.intercepts(
            start + ms(500),
            Phase::Moved,
            Vector::new(0.0, 60.0),
            moved,
            rooms(0.0)
        ));
        assert!(!flick.intercepts(
            start + ms(900),
            Phase::Moved,
            Vector::new(0.0, 60.0),
            AT,
            rooms(0.0)
        ));
    }

    #[test]
    fn the_spline_runs_from_nothing_to_all_of_it() {
        assert!(spline::sample(0.0).0 < 0.001);
        assert_eq!(spline::sample(1.0), (1.0, 0.0));
        let (half, _) = spline::sample(0.5);
        assert!(half > 0.5 && half < 1.0, "{half}");
        let (duration, distance) = spline::parameters(2000.0, FRICTION);
        assert!(duration > ms(500) && duration < ms(5000), "{duration:?}");
        assert!(distance > 100.0, "{distance}");
        assert_eq!(spline::parameters(-2000.0, FRICTION).1, -distance);
    }
}
