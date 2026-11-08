//! Where a watched stream's sound and picture sit on the streamer's own clock,
//! and the bound on when a decoded picture is shown.
//!
//! Two questions, both about the streamer's timeline rather than this
//! machine's. The first is how far the picture runs from the sound: a 90 kHz
//! count and a 48 kHz one share no zero and no rate, so they can only be
//! compared once the streamer's sender reports have placed each on wall-clock
//! time, and the answer is only worth anything while those reports are fresh.
//! The second is when a decoded picture goes to the window: the sound that is
//! already audible is the master, so a picture the sound has passed by is
//! dropped and one it hasn't reached yet waits, briefly, rather than jumping
//! ahead. Both are pure functions of the instants they are given, so a test
//! drives them without waiting.

use std::time::{Duration, Instant};

/// The picture's clock: RTP timestamps on a stream's video SSRC count 90 kHz
/// ticks, as the RTP profile says of a 90 kHz one.
pub const PICTURE_RATE: u32 = 90_000;
/// The sound's clock: RTP timestamps on its audio SSRC count 48 kHz samples,
/// the rate a stream's Opus runs at.
pub const SOUND_RATE: u32 = 48_000;
/// What the NTP time kept in a sender report spans: its middle 32 bits are
/// seconds in 16.16, so the seconds they count turn over every 18 hours.
const NTP_TURN: f64 = 65_536.0;
/// A mapping older than this says nothing about the stream now. A sender
/// report comes about once a second, so five missed ones is a streamer that
/// stopped reporting, not a clock to measure another against.
pub const STALE: Duration = Duration::from_secs(5);
/// A picture the audible sound has passed by more than this is dropped:
/// showing it would be showing the past, and the sound is the master.
pub const LATE: Duration = Duration::from_millis(80);
/// A picture the audible sound hasn't reached yet waits at most this long
/// before it is shown anyway: holding one longer would grow the delay rather
/// than remove it.
pub const EARLY: Duration = Duration::from_millis(80);

/// Which of a stream's two clocks a mapping is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clock {
    /// The sound's, counted in 48 kHz samples.
    Sound,
    /// The picture's, counted in 90 kHz ticks.
    Picture,
}

/// One of the streamer's clocks as its last sender report put it: the RTP
/// timestamp it read at an NTP instant, and when the report reached Yapfast,
/// which says how fresh the mapping is.
///
/// A sender report names no SSRC of its own, so a stream that changed SSRC is
/// seen the one way a clock change shows: its timestamps stop carrying the held
/// clock on, and [`Timeline::heard`] drops both mappings rather than measuring
/// across the break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    /// The middle 32 bits of the NTP time: seconds since 1900 in 16.16,
    /// keeping the low 16 bits of the seconds and the high 16 of the fraction,
    /// so the seconds turn over every 18 hours.
    pub ntp: u32,
    /// The RTP timestamp the clock carried at that instant.
    pub rtp: u32,
    /// When the report reached Yapfast.
    pub heard: Instant,
}

impl Mapping {
    /// Where `timestamp` on this clock sits on wall-clock time, in seconds on
    /// the middle 32 bits of NTP: the report's own instant plus the ticks
    /// between its timestamp and this one, at `rate` ticks a second. Read
    /// signed, so a timestamp on the far side of the clock's wrap is a small
    /// step rather than four billion ticks.
    pub fn wall_time(self, timestamp: u32, rate: u32) -> f64 {
        let ticks = timestamp.wrapping_sub(self.rtp) as i32;
        f64::from(self.ntp) / NTP_TURN + f64::from(ticks) / f64::from(rate)
    }
}

/// A stream's two clocks against each other, in milliseconds, each the
/// picture's position minus the sound's on one wall clock: positive is a
/// picture running ahead of the sound, negative one behind it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Skew {
    /// The newest sound and the newest picture that arrived: how far apart the
    /// streamer sent them and the link delivered them. It moves with the
    /// network, so it is not what lip sync is judged on.
    pub arrival_ms: Option<i64>,
    /// What is audible now against what was last presented: the gap a viewer
    /// actually sees and hears. Estimated on the presentation side, since the
    /// worker hands a picture over and the renderer's own completion is the
    /// window's to know.
    pub presentation_ms: Option<i64>,
}

/// The two mappings a stream's sound and picture are compared through, and
/// the SSRCs they came on.
///
/// Another SSRC is another stream, or this one restarted, and timestamps on
/// either side of that are not two readings of one clock: both mappings go
/// rather than a skew measured across the change. The same for a session or a
/// device that changed under the stream, which a caller says with [`reset`].
///
/// [`reset`]: Timeline::reset
#[derive(Clone, Copy, Debug, Default)]
pub struct Timeline {
    sound: Option<Mapping>,
    picture: Option<Mapping>,
}

impl Timeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets both clocks: a stop, a seek, a deafen or a clock discontinuity
    /// leaves nothing that maps a timestamp onto the streamer's time.
    pub fn reset(&mut self) {
        self.sound = None;
        self.picture = None;
    }

    /// Whether anything maps both clocks, so a skew can be read at all.
    pub fn mapped(&self) -> bool {
        self.sound.is_some() && self.picture.is_some()
    }

    /// Notes the streamer's last report on `clock`. A report on a different
    /// SSRC than the one held is another stream, and so is one whose timestamp
    /// doesn't carry the held clock on (see [`continuous`]): both mappings go
    /// first, since timestamps on either side of the change are not two
    /// readings of one clock.
    pub fn heard(&mut self, clock: Clock, mapping: Mapping) {
        let rate = match clock {
            Clock::Sound => SOUND_RATE,
            Clock::Picture => PICTURE_RATE,
        };
        let broken =
            |held: Option<Mapping>| held.is_some_and(|held| !continuous(held, mapping, rate));
        let changed = match clock {
            Clock::Sound => broken(self.sound),
            Clock::Picture => broken(self.picture),
        };
        if changed {
            self.reset();
        }
        match clock {
            Clock::Sound => self.sound = Some(mapping),
            Clock::Picture => self.picture = Some(mapping),
        }
    }

    /// Where the sound and the picture are against each other at `now`.
    /// `arrival` is the newest timestamp that came on each clock;
    /// `presentation` is the one that is audible and the one that was shown.
    /// Either field is `None` while a mapping is missing or older than
    /// [`STALE`], since which clock an unmapped timestamp ran on is a guess.
    pub fn skew(
        &self,
        now: Instant,
        arrival: (Option<u32>, Option<u32>),
        presentation: (Option<u32>, Option<u32>),
    ) -> Skew {
        let Some((sound, picture)) = self.fresh(now) else {
            return Skew::default();
        };
        Skew {
            arrival_ms: gap(sound, picture, arrival.0, arrival.1),
            presentation_ms: gap(sound, picture, presentation.0, presentation.1),
        }
    }

    /// Both mappings, if they are there and younger than [`STALE`].
    fn fresh(&self, now: Instant) -> Option<(Mapping, Mapping)> {
        let fresh = |mapping: Option<Mapping>| {
            mapping.filter(|mapping| now.saturating_duration_since(mapping.heard) < STALE)
        };
        Some((fresh(self.sound)?, fresh(self.picture)?))
    }

    /// Where each of the streamer's clocks reads now, as `now` on its own
    /// rate carried from its last report. It is where the streams *are* on
    /// the streamer's timeline this instant, before any device buffer: what
    /// arrived, not what a viewer has yet heard.
    fn carried(&self, now: Instant) -> Option<(u32, u32)> {
        let (sound, picture) = self.fresh(now)?;
        Some((
            carried(sound, now, SOUND_RATE),
            carried(picture, now, PICTURE_RATE),
        ))
    }

    /// How far apart the two streams are as they arrive, in milliseconds: the
    /// sound's clock and the picture's carried to `now`, before any device
    /// buffer is taken off. `None` while either mapping is missing or stale.
    pub fn arrival_ms(&self, now: Instant) -> Option<i64> {
        let (sound, picture) = self.fresh(now)?;
        let (sound_at, picture_at) = self.carried(now)?;
        gap(sound, picture, Some(sound_at), Some(picture_at))
    }

    /// How far apart the sound a viewer hears and the picture last shown are,
    /// in milliseconds: the picture's own stream time against where the sound
    /// audible now sits, both on the picture's timeline. `None` while either
    /// mapping is missing or stale, or while no picture has been shown.
    pub fn presentation_ms(
        &self,
        now: Instant,
        held: Duration,
        shown_ms: Option<u64>,
    ) -> Option<i64> {
        let shown = shown_ms?;
        let audible = self.audible_ms(now, held)?;
        Some(shown as i64 - audible as i64)
    }

    /// Where the sound a viewer hears now sits on the picture's own timeline,
    /// in milliseconds: the streamer's sound clock, carried from its last
    /// report to `now`, less what the device still holds unplayed. `None`
    /// while either mapping is missing or stale.
    ///
    /// It is an estimate. The report says where the clock read when it was
    /// sent, and `held` is measured at the device, but the moment a sample
    /// leaves the buffer is the device's to know, so the two are carried
    /// across the gap at the clock's own rate rather than observed.
    pub fn audible_ms(&self, now: Instant, held: Duration) -> Option<u64> {
        let (sound, picture) = self.fresh(now)?;
        // The sound's clock now, carried from the report by what has arrived
        // since, and back by what the device still holds.
        let since = now.saturating_duration_since(sound.heard).as_secs_f64();
        let playing = f64::from(sound.rtp) + since * f64::from(SOUND_RATE)
            - held.as_secs_f64() * f64::from(SOUND_RATE);
        // That instant on the wall clock, then that wall clock on the
        // picture's timeline, counted in ticks from its own report: the same
        // unit a picture's RTP timestamp is in, then turned into the
        // milliseconds a scheduler compares.
        let wall = sound.wall_time(playing as u32, SOUND_RATE);
        let ticks = f64::from(picture.rtp)
            + (wall - f64::from(picture.ntp) / NTP_TURN) * f64::from(PICTURE_RATE);
        let milliseconds = ticks * 1_000.0 / f64::from(PICTURE_RATE);
        Some(milliseconds.max(0.0).round() as u64)
    }
}

/// `picture_at` on the picture's clock against `sound_at` on the sound's, in
/// milliseconds, both placed on wall-clock time by their own report. `None`
/// while either timestamp is missing.
fn gap(
    sound: Mapping,
    picture: Mapping,
    sound_at: Option<u32>,
    picture_at: Option<u32>,
) -> Option<i64> {
    let (sound_at, picture_at) = (sound_at?, picture_at?);
    let sound = sound.wall_time(sound_at, SOUND_RATE);
    let picture = picture.wall_time(picture_at, PICTURE_RATE);
    Some(milliseconds(picture - sound))
}

/// Where `mapping`'s clock reads at `now`, as a timestamp on its own rate,
/// carried on from the report by the time since. Read as a wrapping add so a
/// clock carried past its wrap is a small step rather than a panic.
fn carried(mapping: Mapping, now: Instant, rate: u32) -> u32 {
    let ticks = now.saturating_duration_since(mapping.heard).as_secs_f64() * f64::from(rate);
    mapping.rtp.wrapping_add(ticks as u32)
}

/// Whether `next` carries `before`'s clock on without a break: its timestamp
/// is where `before`, carried to when `next` was heard, says it should be, to
/// within a second of that clock.
///
/// A report from another SSRC, a stream that restarted, or a clock that jumped
/// all read as a break — which a caller turns into a [`Timeline::reset`], since
/// timestamps on either side of one are not two readings of the same clock and
/// a skew measured across it would be fiction.
pub fn continuous(before: Mapping, next: Mapping, rate: u32) -> bool {
    let expected = carried(before, next.heard, rate);
    (next.rtp.wrapping_sub(expected) as i32).unsigned_abs() < rate
}

/// `seconds` in milliseconds, read as a step across the NTP turn rather than
/// around it. The time kept is the middle 32 bits, so its seconds turn over
/// every 18 hours: two reports seconds apart can sit either side of that turn.
/// Half the turn is far more than any real gap, so a difference wider than
/// that is the turn, not the clocks.
fn milliseconds(seconds: f64) -> i64 {
    let mut seconds = seconds;
    if seconds > NTP_TURN / 2.0 {
        seconds -= NTP_TURN;
    } else if seconds < -NTP_TURN / 2.0 {
        seconds += NTP_TURN;
    }
    (seconds * 1_000.0).round() as i64
}

/// What to do with a decoded picture, against the sound that is audible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    /// Now: it is due.
    Now,
    /// Wait this long and then show it: it came early and the sound is
    /// catching up. Never more than [`EARLY`].
    After(Duration),
    /// Never: the sound has passed it by more than [`LATE`].
    Drop,
}

/// One picture waiting for its turn, with the stream time it holds.
#[derive(Clone, Copy, Debug)]
struct Waiting {
    time_ms: u64,
    due: Instant,
}

/// Bounds when a decoded picture is handed to the window, against the sound
/// that is audible: the sound is the master and the picture follows it.
///
/// At most one picture waits besides the one on screen: a second offer
/// replaces the first, which is then dropped, so a stream that comes early
/// never queues pictures behind a clock that is behind it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Scheduler {
    waiting: Option<Waiting>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets the picture waiting: a stop, a seek, a deafen or a clock
    /// discontinuity means the time it was due no longer means anything.
    pub fn reset(&mut self) {
        self.waiting = None;
    }

    /// The stream time of the picture waiting, once one is.
    pub fn waiting(&self) -> Option<u64> {
        self.waiting.map(|waiting| waiting.time_ms)
    }

    /// Decides the picture whose stream time is `picture_ms` against
    /// `audible_ms`, the sound a viewer hears now on that same timeline, at
    /// `now` on this machine's.
    ///
    /// A picture more than [`LATE`] behind the sound is dropped; one ahead of
    /// it waits at most [`EARLY`]; anything else shows at once. Neither bound
    /// is a lock or a sleep, so this never blocks the caller, which is what
    /// keeps it off the UI and network threads.
    pub fn offer(&mut self, picture_ms: u64, audible_ms: u64, now: Instant) -> Show {
        let ahead = picture_ms as i64 - audible_ms as i64;
        if ahead < -(LATE.as_millis() as i64) {
            return Show::Drop;
        }
        let wait = Duration::from_millis(ahead.max(0) as u64).min(EARLY);
        if wait.is_zero() {
            self.waiting = None;
            return Show::Now;
        }
        // At most one waits: the newer offer is the one a viewer is closer to,
        // and the one it replaces is simply not shown.
        self.waiting = Some(Waiting {
            time_ms: picture_ms,
            due: now + wait,
        });
        Show::After(wait)
    }

    /// Whether the picture held back is due at `now`, handing over its stream
    /// time when it is.
    pub fn ready(&mut self, now: Instant) -> Option<u64> {
        match self.waiting {
            Some(waiting) if now >= waiting.due => {
                self.waiting = None;
                Some(waiting.time_ms)
            }
            _ => None,
        }
    }
}

/// A picture that reached the window, and how its time is known.
///
/// The worker hands a picture over through the frames file and the renderer's
/// own completion — the fence value that says the GPU drew it — is the
/// window's to observe, so from a worker's side the time a picture reached the
/// screen is the time it was submitted, and the record says so. A renderer
/// that reports completion would set `estimated` false rather than the worker
/// claiming a presentation it did not see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Presented {
    /// The picture's stream time, in milliseconds, as it went to the window.
    pub time_ms: u64,
    /// When it was handed over.
    pub at: Instant,
    /// Whether `at` is when the picture was submitted rather than when it was
    /// drawn: `true` when only submission is known.
    pub estimated: bool,
}

impl Presented {
    /// A picture `time_ms` given to the window at `at`, whose drawing the
    /// worker cannot see complete: the time is its submission, estimated.
    pub fn submitted(time_ms: u64, at: Instant) -> Self {
        Self {
            time_ms,
            at,
            estimated: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clock that read `rtp` at `seconds` of NTP time, heard `ago` before
    /// `now`; a test names both so nothing waits.
    fn mapping(now: Instant, ago: Duration, seconds: u32, rtp: u32) -> Mapping {
        Mapping {
            ntp: seconds << 16,
            rtp,
            heard: now - ago,
        }
    }

    /// A stream whose sound read 4_800_000 at 100 s and whose picture read
    /// 9_000_000 at that same instant, both reported just now.
    fn stream(now: Instant) -> Timeline {
        let mut timeline = Timeline::new();
        timeline.heard(Clock::Sound, mapping(now, Duration::ZERO, 100, 4_800_000));
        timeline.heard(Clock::Picture, mapping(now, Duration::ZERO, 100, 9_000_000));
        timeline
    }

    #[test]
    fn arrival_and_presentation_are_separate_fields() {
        let now = Instant::now();
        let timeline = stream(now);
        // The newest sound and picture that came are the many both reports
        // named, so nothing separates them on arrival.
        let arrival = (Some(4_800_000), Some(9_000_000));
        // The picture that was shown is a second of its clock past that, and
        // the sound audible now sits on its own report: the picture is a
        // second ahead, which is what a viewer would see and hear.
        let presentation = (Some(4_800_000), Some(9_000_000 + 90_000));
        let skew = timeline.skew(now, arrival, presentation);
        assert_eq!(skew.arrival_ms, Some(0), "arrival");
        assert_eq!(skew.presentation_ms, Some(1_000), "presentation");
    }

    #[test]
    fn the_presentation_skew_can_go_either_way() {
        let now = Instant::now();
        let timeline = stream(now);
        // Half a second of sound clock: the picture is behind, not ahead.
        let presentation = (Some(4_800_000 + 24_000), Some(9_000_000));
        let skew = timeline.skew(now, (None, None), presentation);
        assert_eq!(skew.arrival_ms, None, "no arrival timestamps");
        assert_eq!(skew.presentation_ms, Some(-500));
    }

    #[test]
    fn a_stream_that_restarted_is_another_stream() {
        let now = Instant::now();
        let mut timeline = stream(now);
        assert!(timeline.mapped());
        // The sound now comes on a clock that doesn't carry the held one on —
        // a restart, or another SSRC — so the picture's mapping, measured
        // against the old sound clock, no longer says anything.
        timeline.heard(Clock::Sound, mapping(now, Duration::ZERO, 140, 6_720_000));
        assert!(!timeline.mapped(), "both mappings go");
        let skew = timeline.skew(now, (Some(4_800_000), Some(9_000_000)), (None, None));
        assert_eq!(skew, Skew::default(), "nothing to compare");
    }

    #[test]
    fn the_same_clock_carried_on_keeps_both_mappings() {
        let now = Instant::now();
        let mut timeline = Timeline::new();
        timeline.heard(
            Clock::Sound,
            mapping(now, Duration::from_secs(1), 100, 4_800_000),
        );
        timeline.heard(
            Clock::Picture,
            mapping(now, Duration::from_secs(1), 100, 9_000_000),
        );
        // A second report on the sound's own SSRC, a second later, carrying a
        // second of its clock on: the same clock, so the picture's mapping
        // still holds.
        timeline.heard(Clock::Sound, mapping(now, Duration::ZERO, 101, 4_848_000));
        assert!(timeline.mapped());
    }

    #[test]
    fn a_clock_that_jumped_is_another_stream() {
        let now = Instant::now();
        let mut timeline = Timeline::new();
        timeline.heard(
            Clock::Sound,
            mapping(now, Duration::from_secs(1), 100, 4_800_000),
        );
        timeline.heard(Clock::Picture, mapping(now, Duration::ZERO, 100, 9_000_000));
        assert!(timeline.mapped());
        // The sound's clock leaps ten seconds on the same SSRC: a restart, not
        // a clock running fast, so nothing measured across it holds.
        timeline.heard(Clock::Sound, mapping(now, Duration::ZERO, 110, 5_280_000));
        assert!(!timeline.mapped(), "both mappings go");
    }

    #[test]
    fn a_mapping_older_than_five_seconds_says_nothing() {
        let now = Instant::now();
        let mut timeline = Timeline::new();
        timeline.heard(
            Clock::Sound,
            mapping(now, Duration::from_secs(4), 100, 4_800_000),
        );
        timeline.heard(
            Clock::Picture,
            mapping(now, Duration::from_secs(5), 100, 9_000_000),
        );
        // The picture's report is exactly five seconds old: past what a
        // mapping is good for.
        let skew = timeline.skew(
            now,
            (Some(4_800_000), Some(9_000_000)),
            (Some(4_800_000), Some(9_000_000)),
        );
        assert_eq!(skew, Skew::default(), "the picture's mapping is stale");
        // Four seconds old, both are still good.
        let mut timeline = stream(now);
        timeline.heard(
            Clock::Picture,
            mapping(now, Duration::from_secs(4), 100, 9_000_000),
        );
        let skew = timeline.skew(now, (Some(4_800_000), Some(9_000_000)), (None, None));
        assert_eq!(skew.arrival_ms, Some(0));
    }

    #[test]
    fn a_timestamp_past_the_wrap_is_a_small_step() {
        let now = Instant::now();
        let mut timeline = Timeline::new();
        // The sound's clock is a half second from turning over; the picture's
        // sits at zero. A quarter second either side of the wrap is a quarter
        // second, not four billion ticks.
        timeline.heard(
            Clock::Sound,
            mapping(now, Duration::ZERO, 100, u32::MAX - 23_999),
        );
        timeline.heard(Clock::Picture, mapping(now, Duration::ZERO, 100, 0));
        let sound = u32::MAX - 23_999;
        let skew = timeline.skew(
            now,
            (Some(sound.wrapping_add(48_000)), Some(90_000)),
            (None, None),
        );
        assert_eq!(skew.arrival_ms, Some(0), "both are a second on");
    }

    #[test]
    fn the_eighteen_hour_turn_is_not_a_skew() {
        let now = Instant::now();
        let mut timeline = Timeline::new();
        // The NTP time kept turns over at 65_536 s. A sound at the last second
        // of it and a picture a second past the start are two seconds apart,
        // not eighteen hours.
        timeline.heard(
            Clock::Sound,
            mapping(now, Duration::ZERO, 65_535, 4_800_000),
        );
        timeline.heard(Clock::Picture, mapping(now, Duration::ZERO, 0, 0));
        let skew = timeline.skew(now, (Some(4_800_000), Some(90_000)), (None, None));
        assert_eq!(skew.arrival_ms, Some(2_000));
    }

    #[test]
    fn a_missing_timestamp_leaves_that_field_unknown() {
        let now = Instant::now();
        let timeline = stream(now);
        let skew = timeline.skew(now, (Some(4_800_000), None), (Some(4_800_000), Some(0)));
        assert_eq!(skew.arrival_ms, None, "no picture arrived");
        assert_eq!(skew.presentation_ms, Some(-100_000));
    }

    #[test]
    fn the_audible_sound_is_carried_to_the_picture_timeline() {
        let now = Instant::now();
        // Both clocks read zero at 100 s, and the sound's report came a second
        // ago. With nothing held, the sound audible now is a second of its
        // clock past zero, which is a second on the picture's timeline too.
        let mut timeline = Timeline::new();
        timeline.heard(Clock::Sound, mapping(now, Duration::from_secs(1), 100, 0));
        timeline.heard(Clock::Picture, mapping(now, Duration::ZERO, 100, 0));
        let audible = timeline.audible_ms(now, Duration::ZERO).unwrap();
        assert!((audible as i64 - 1_000).abs() <= 1, "{audible}");
        // Half a second held at the device is half a second the viewer hasn't
        // heard yet.
        let held = timeline
            .audible_ms(now, Duration::from_millis(500))
            .unwrap();
        assert!((held as i64 - 500).abs() <= 1, "{held}");
    }

    #[test]
    fn the_audible_sound_is_unknown_without_a_fresh_mapping() {
        let now = Instant::now();
        let mut timeline = Timeline::new();
        assert_eq!(timeline.audible_ms(now, Duration::ZERO), None);
        timeline.heard(Clock::Sound, mapping(now, Duration::from_secs(6), 100, 0));
        assert_eq!(timeline.audible_ms(now, Duration::ZERO), None, "stale");
    }

    #[test]
    fn the_arrival_skew_is_carried_to_now_unbuffered() {
        let now = Instant::now();
        // Both clocks read zero at 100 s, and both reports came a second ago.
        // Carried to now they are still level, so nothing separates what
        // arrived. The device's buffer is not in this number.
        let mut timeline = Timeline::new();
        timeline.heard(Clock::Sound, mapping(now, Duration::from_secs(1), 100, 0));
        timeline.heard(Clock::Picture, mapping(now, Duration::from_secs(1), 100, 0));
        assert_eq!(timeline.arrival_ms(now), Some(0));
        // The picture's report came only half a second ago, so its clock is
        // carried half as far: it sits half a second behind the sound.
        let mut timeline = Timeline::new();
        timeline.heard(Clock::Sound, mapping(now, Duration::from_secs(1), 100, 0));
        timeline.heard(
            Clock::Picture,
            mapping(now, Duration::from_millis(500), 100, 0),
        );
        assert_eq!(timeline.arrival_ms(now), Some(-500));
    }

    #[test]
    fn the_arrival_skew_is_unknown_without_both_mappings() {
        let now = Instant::now();
        let mut timeline = Timeline::new();
        assert_eq!(timeline.arrival_ms(now), None);
        timeline.heard(Clock::Sound, mapping(now, Duration::ZERO, 100, 0));
        assert_eq!(timeline.arrival_ms(now), None, "no picture mapping");
        timeline.heard(Clock::Picture, mapping(now, Duration::from_secs(6), 100, 0));
        assert_eq!(timeline.arrival_ms(now), None, "stale");
    }

    #[test]
    fn the_presentation_skew_is_the_shown_picture_against_the_audible_sound() {
        let now = Instant::now();
        // Both clocks read zero at 100 s; the sound's report came a second
        // ago, so the sound audible now is at a second on the picture's
        // timeline. A picture shown at 1.5 s is half a second ahead of it.
        let mut timeline = Timeline::new();
        timeline.heard(Clock::Sound, mapping(now, Duration::from_secs(1), 100, 0));
        timeline.heard(Clock::Picture, mapping(now, Duration::ZERO, 100, 0));
        let presentation = timeline.presentation_ms(now, Duration::ZERO, Some(1_500));
        assert_eq!(presentation, Some(500));
        // Half a second still held at the device is half a second the sound
        // hasn't been heard yet: the picture is a whole second ahead.
        let presentation = timeline.presentation_ms(now, Duration::from_millis(500), Some(1_500));
        assert_eq!(presentation, Some(1_000));
        // No picture shown yet says nothing.
        assert_eq!(timeline.presentation_ms(now, Duration::ZERO, None), None);
    }

    #[test]
    fn a_picture_the_sound_has_passed_is_dropped() {
        let now = Instant::now();
        let mut scheduler = Scheduler::new();
        // 81 ms behind the sound is past what a viewer can be shown.
        assert_eq!(scheduler.offer(1_000, 1_081, now), Show::Drop);
        // Exactly 80 ms behind is the bound, and shows.
        assert_eq!(scheduler.offer(1_000, 1_080, now), Show::Now);
        assert_eq!(scheduler.waiting(), None);
    }

    #[test]
    fn an_early_picture_waits_at_most_eighty_ms() {
        let now = Instant::now();
        let mut scheduler = Scheduler::new();
        // 30 ms ahead of the sound waits 30 ms.
        assert_eq!(
            scheduler.offer(1_030, 1_000, now),
            Show::After(Duration::from_millis(30))
        );
        assert_eq!(scheduler.waiting(), Some(1_030));
        // A second ahead still waits only the 80 ms bound.
        assert_eq!(scheduler.offer(2_000, 1_000, now), Show::After(EARLY));
        assert_eq!(scheduler.waiting(), Some(2_000), "the newer one waits");
        // Due exactly 80 ms on.
        assert_eq!(
            scheduler.ready(now + EARLY - Duration::from_millis(1)),
            None
        );
        assert_eq!(scheduler.ready(now + EARLY), Some(2_000));
        assert_eq!(scheduler.waiting(), None, "handed over once");
        assert_eq!(scheduler.ready(now + EARLY), None, "and not again");
    }

    #[test]
    fn only_one_picture_waits_and_a_newer_one_replaces_it() {
        let now = Instant::now();
        let mut scheduler = Scheduler::new();
        assert_eq!(
            scheduler.offer(1_040, 1_000, now),
            Show::After(Duration::from_millis(40))
        );
        assert_eq!(scheduler.waiting(), Some(1_040));
        // A newer picture offers while the first waits: it takes its place,
        // and the one it replaced is never shown.
        assert_eq!(
            scheduler.offer(1_050, 1_000, now),
            Show::After(Duration::from_millis(50))
        );
        assert_eq!(scheduler.waiting(), Some(1_050));
    }

    #[test]
    fn a_picture_on_time_shows_at_once_and_clears_what_waited() {
        let now = Instant::now();
        let mut scheduler = Scheduler::new();
        let _ = scheduler.offer(1_040, 1_000, now);
        assert_eq!(scheduler.waiting(), Some(1_040));
        // The sound caught up: the picture is due, and nothing is left
        // waiting behind it.
        assert_eq!(scheduler.offer(1_000, 1_000, now), Show::Now);
        assert_eq!(scheduler.waiting(), None);
    }

    #[test]
    fn a_reset_forgets_the_picture_that_waited() {
        let now = Instant::now();
        let mut scheduler = Scheduler::new();
        let _ = scheduler.offer(1_040, 1_000, now);
        scheduler.reset();
        assert_eq!(scheduler.waiting(), None);
        assert_eq!(scheduler.ready(now + EARLY), None, "it is gone");
    }

    #[test]
    fn a_clock_carried_on_is_continuous_and_a_jump_is_not() {
        let now = Instant::now();
        // The sound's clock read zero a second ago; a report now that reads a
        // second of its own samples on carries it on.
        let before = mapping(now, Duration::from_secs(1), 100, 0);
        let on = mapping(now, Duration::ZERO, 100, 48_000);
        assert!(continuous(before, on, SOUND_RATE));
        // A report four seconds later, carrying four seconds of its clock on
        // from the first, is still the same clock even though nothing
        // reported in between.
        let early = mapping(now, Duration::from_secs(4), 100, 0);
        let later = mapping(now, Duration::ZERO, 100, 4 * 48_000);
        assert!(continuous(early, later, SOUND_RATE));
        // A report whose timestamp jumped ten seconds is a restart, not a
        // clock running fast.
        let jumped = mapping(now, Duration::ZERO, 100, 480_000);
        assert!(!continuous(before, jumped, SOUND_RATE));
        // And one whose clock turned over between the two reports is still
        // continuous: the first read a little before the wrap, the second a
        // little after it, half a second of samples on either side.
        let before_wrap = mapping(now, Duration::from_secs(1), 100, u32::MAX - 23_999);
        let after_wrap = mapping(now, Duration::ZERO, 100, 24_000);
        assert!(continuous(before_wrap, after_wrap, SOUND_RATE));
    }

    #[test]
    fn a_submitted_picture_is_marked_estimated() {
        let now = Instant::now();
        let presented = Presented::submitted(1_234, now);
        assert_eq!(presented.time_ms, 1_234);
        assert_eq!(presented.at, now);
        assert!(presented.estimated, "only submission is known");
    }
}
