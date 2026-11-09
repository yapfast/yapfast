//! How fast a Go Live stream sends: a sender-side controller that keeps the
//! uplink's queue short. Discord's client runs libwebrtc's delay-based
//! control on transport-wide feedback; Yapfast reads the round trip and the
//! loss from the receiver reports Discord's stream server sends about the
//! video, which a queue at the sender's router lengthens too.
//!
//! It starts below the cap, raises the target a little each report while
//! the round trip stays near the lowest seen, and lowers it by a share when
//! the round trip climbs well over that or packets are lost, then holds.
//! The cap is Discord's for the stream's height, or the server's estimate
//! (REMB) when that's lower.

use std::{
    collections::VecDeque,
    fmt,
    time::{Duration, Instant},
};

/// The lowest bit/s a stream is sent at, what Discord's client gives a
/// screen.
pub const FLOOR: u64 = 500_000;
/// The most a stream starts at, below the cap for 1080p, about what
/// Discord's client trials 1080p30 at for accounts without Nitro.
const START: u64 = 2_500_000;
/// A round trip this far over the lowest is a queue building.
const QUEUING: Duration = Duration::from_millis(60);
/// For this many reports in a row.
const QUEUING_REPORTS: u32 = 2;
/// A round trip within this of the lowest leaves room to send more.
const NEAR: Duration = Duration::from_millis(25);
/// Packets lost since the last report, out of 256, past which the target
/// drops: 10%, as libwebrtc's loss-based control.
const LOSSY: u8 = 26;
/// And past which it doesn't rise: 2%.
const SOME_LOSS: u8 = 5;
/// Each drop takes the target to this share, in hundredths.
const BACK_OFF: u64 = 85;
/// Each rise adds this share, in hundredths: about 17 s from the start to
/// 1080p's cap, with a report a second.
const RAMP: u64 = 8;
/// After a drop, the target doesn't rise for this long.
const HOLD: Duration = Duration::from_secs(3);
/// The lowest round trip is the lowest of those measured this long ago or
/// since, so a path that changes gets a new one.
const BASELINE: Duration = Duration::from_secs(60);

/// The bit/s a stream starts at under `cap`.
pub fn start(cap: u64) -> u64 {
    cap.clamp(FLOOR, START)
}

/// Why the target changed, for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The round trip stayed near the lowest.
    Ramp,
    /// The round trip climbed well over the lowest: a queue is building.
    Queuing,
    /// Receiver reports said packets were lost.
    Loss,
    /// The cap or Discord's estimate fell below the target.
    Cap,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ramp => "the round trip stayed short",
            Self::Queuing => "the round trip grew: the uplink is queuing",
            Self::Loss => "packets were lost",
            Self::Cap => "the cap or Discord's estimate is lower",
        })
    }
}

pub struct Controller {
    target: u64,
    /// Discord's cap for the stream's height.
    cap: u64,
    /// The server's estimate (REMB), once it sent one.
    estimate: Option<u64>,
    /// Round trips measured within [`BASELINE`], oldest first.
    trips: VecDeque<(Instant, Duration)>,
    /// Reports in a row whose round trip said a queue was building.
    queuing: u32,
    /// Until when the target doesn't rise.
    held: Option<Instant>,
}

impl Controller {
    pub fn new(cap: u64) -> Self {
        Self {
            target: start(cap),
            cap,
            estimate: None,
            trips: VecDeque::new(),
            queuing: 0,
            held: None,
        }
    }

    /// The bit/s to send at.
    pub fn target(&self) -> u64 {
        self.target
    }

    /// The lowest round trip measured lately.
    pub fn baseline(&self) -> Option<Duration> {
        self.trips.iter().map(|&(_, trip)| trip).min()
    }

    fn limit(&self) -> u64 {
        self.estimate
            .map_or(self.cap, |estimate| estimate.min(self.cap))
            .max(FLOOR)
    }

    /// Lowers the target to the limit when it's past it.
    fn clamp(&mut self) -> Option<Reason> {
        let limit = self.limit();
        (self.target > limit).then(|| {
            self.target = limit;
            Reason::Cap
        })
    }

    /// Takes a new cap, for a new frame size.
    pub fn set_cap(&mut self, cap: u64) -> Option<Reason> {
        self.cap = cap;
        self.clamp()
    }

    /// Takes the server's bandwidth estimate (REMB).
    pub fn set_estimate(&mut self, estimate: u64) -> Option<Reason> {
        self.estimate = Some(estimate);
        self.clamp()
    }

    /// Takes a receiver report about the video, received `now`: the round
    /// trip it measured, if it echoed a sender report this end still
    /// knows, and the packets lost since the last, out of 256.
    pub fn report(
        &mut self,
        now: Instant,
        trip: Option<Duration>,
        fraction_lost: u8,
    ) -> Option<Reason> {
        if let Some(trip) = trip {
            self.trips.push_back((now, trip));
            while self
                .trips
                .front()
                .is_some_and(|&(at, _)| now.duration_since(at) > BASELINE)
            {
                self.trips.pop_front();
            }
        }
        let baseline = self.baseline();
        let over = trip.zip(baseline).map(|(trip, baseline)| trip - baseline);
        if over.is_some_and(|over| over > QUEUING) {
            self.queuing += 1;
        } else if trip.is_some() {
            self.queuing = 0;
        }
        let drop = if fraction_lost > LOSSY {
            Some(Reason::Loss)
        } else if self.queuing >= QUEUING_REPORTS {
            Some(Reason::Queuing)
        } else {
            None
        };
        if let Some(reason) = drop {
            self.queuing = 0;
            self.held = Some(now + HOLD);
            let lower = (self.target * BACK_OFF / 100).max(FLOOR);
            return (lower < self.target).then(|| {
                self.target = lower;
                reason
            });
        }
        // Rising only on a round trip measured: without one, sending more
        // could fill the queue unseen.
        let held = self.held.is_some_and(|until| now < until);
        if held || fraction_lost > SOME_LOSS || !over.is_some_and(|over| over <= NEAR) {
            return None;
        }
        let higher = (self.target * (100 + RAMP) / 100).min(self.limit());
        (higher > self.target).then(|| {
            self.target = higher;
            Reason::Ramp
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAP: u64 = 9_000_000;

    fn ms(milliseconds: u64) -> Duration {
        Duration::from_millis(milliseconds)
    }

    /// Reports a second apart from `at`, each with `trip` and `lost`.
    fn reports(
        controller: &mut Controller,
        at: &mut Instant,
        count: usize,
        trip: Option<Duration>,
        lost: u8,
    ) -> Vec<Option<Reason>> {
        (0..count)
            .map(|_| {
                *at += Duration::from_secs(1);
                controller.report(*at, trip, lost)
            })
            .collect()
    }

    #[test]
    fn starts_below_the_cap() {
        assert_eq!(Controller::new(CAP).target(), 2_500_000);
        assert_eq!(Controller::new(3_500_000).target(), 2_500_000);
        assert_eq!(
            Controller::new(1_000_000).target(),
            1_000_000,
            "never past it"
        );
        assert_eq!(Controller::new(100_000).target(), FLOOR);
    }

    #[test]
    fn ramps_up_to_the_cap_while_the_round_trip_stays_short() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        let said = reports(&mut controller, &mut at, 3, Some(ms(40)), 0);
        assert!(
            said.iter().all(|&said| said == Some(Reason::Ramp)),
            "{said:?}"
        );
        assert_eq!(
            controller.target(),
            2_500_000 * 108 / 100 * 108 / 100 * 108 / 100
        );
        reports(&mut controller, &mut at, 30, Some(ms(50)), 0);
        assert_eq!(controller.target(), CAP, "within NEAR of the lowest");
        assert_eq!(controller.baseline(), Some(ms(40)));
    }

    #[test]
    fn does_not_ramp_without_a_round_trip() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        let said = reports(&mut controller, &mut at, 10, None, 0);
        assert!(said.iter().all(Option::is_none));
        assert_eq!(controller.target(), 2_500_000);
    }

    #[test]
    fn backs_off_when_the_round_trip_stays_up() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        reports(&mut controller, &mut at, 20, Some(ms(50)), 0);
        let before = controller.target();
        // Between NEAR and QUEUING: holds.
        assert_eq!(
            reports(&mut controller, &mut at, 2, Some(ms(90)), 0),
            [None, None]
        );
        assert_eq!(controller.target(), before);
        // One report over: may be a keyframe; two in a row is a queue.
        assert_eq!(
            reports(&mut controller, &mut at, 2, Some(ms(300)), 0),
            [None, Some(Reason::Queuing)]
        );
        assert_eq!(controller.target(), before * 85 / 100);
        // Held a while, even with the round trip short again.
        assert_eq!(
            reports(&mut controller, &mut at, 2, Some(ms(50)), 0),
            [None, None]
        );
        assert_eq!(
            reports(&mut controller, &mut at, 1, Some(ms(50)), 0),
            [Some(Reason::Ramp)]
        );
    }

    #[test]
    fn a_lone_slow_report_does_not_back_off() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        reports(&mut controller, &mut at, 2, Some(ms(50)), 0);
        let said = [
            reports(&mut controller, &mut at, 1, Some(ms(300)), 0),
            reports(&mut controller, &mut at, 1, Some(ms(50)), 0),
            reports(&mut controller, &mut at, 1, Some(ms(300)), 0),
        ];
        assert!(!said.concat().contains(&Some(Reason::Queuing)), "{said:?}");
    }

    #[test]
    fn backs_off_on_loss_and_holds_on_a_little() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        reports(&mut controller, &mut at, 5, Some(ms(50)), 0);
        let before = controller.target();
        assert_eq!(
            reports(&mut controller, &mut at, 1, Some(ms(50)), 10),
            [None],
            "4%: holds"
        );
        assert_eq!(controller.target(), before);
        assert_eq!(
            reports(&mut controller, &mut at, 1, Some(ms(50)), 40),
            [Some(Reason::Loss)]
        );
        assert_eq!(controller.target(), before * 85 / 100);
        assert_eq!(
            reports(&mut controller, &mut at, 1, None, 40),
            [Some(Reason::Loss)],
            "without a round trip too"
        );
    }

    #[test]
    fn never_goes_below_the_floor() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        let said = reports(&mut controller, &mut at, 40, None, 255);
        assert_eq!(controller.target(), FLOOR);
        assert_eq!(said.last(), Some(&None), "nothing left to drop");
    }

    #[test]
    fn follows_the_cap_and_discords_estimate() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        assert_eq!(controller.set_estimate(2_000_000), Some(Reason::Cap));
        assert_eq!(controller.target(), 2_000_000);
        reports(&mut controller, &mut at, 10, Some(ms(50)), 0);
        assert_eq!(controller.target(), 2_000_000, "never past the estimate");
        assert_eq!(controller.set_estimate(100_000), Some(Reason::Cap));
        assert_eq!(controller.target(), FLOOR, "nor below the floor");
        assert_eq!(controller.set_estimate(20_000_000), None);
        reports(&mut controller, &mut at, 40, Some(ms(50)), 0);
        assert_eq!(
            controller.target(),
            CAP,
            "an estimate past the cap leaves the cap"
        );
        assert_eq!(controller.set_cap(3_500_000), Some(Reason::Cap));
        assert_eq!(controller.target(), 3_500_000);
    }

    #[test]
    fn forgets_round_trips_from_long_ago() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        reports(&mut controller, &mut at, 1, Some(ms(20)), 0);
        reports(&mut controller, &mut at, 61, Some(ms(150)), 0);
        assert_eq!(controller.baseline(), Some(ms(150)), "the path changed");
    }
}
