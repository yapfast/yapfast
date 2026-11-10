//! How fast a Go Live stream sends: a sender-side controller that keeps the
//! uplink's queue short. Discord's client runs libwebrtc's delay-based
//! control on transport-wide feedback; Yapfast reads the round trip and the
//! loss from the receiver reports Discord's stream server sends about the
//! video, which a queue at the sender's router lengthens too.
//!
//! It starts below the cap, raises the target a little each report while
//! the round trip stays near the lowest seen, and lowers it below what was
//! sent when the round trip climbs well over that or packets are lost, then
//! holds. It doesn't rise far past what the encoder sends, so a calm picture
//! can't take it to a rate the uplink never carried, and near the rate it
//! last backed off from it rises slowly. The cap is Discord's for the
//! stream's height, or the server's estimate (REMB) when that's lower.

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
/// Near the rate it last backed off from, from this share of it in
/// hundredths, each rise adds a hundredth of that rate instead.
const NEAR_CAPACITY: u64 = 80;
/// Past this share of that rate, in hundredths, the uplink carries more than
/// it did, and the target rises by [`RAMP`] again.
const PAST_CAPACITY: u64 = 115;
/// The target doesn't rise past this share of what was sent, in hundredths.
const HEADROOM: u64 = 150;
/// What was sent is measured over this long.
const SENDING: Duration = Duration::from_secs(1);
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
    /// The bit/s sent when it last backed off, which the uplink didn't
    /// carry, until the target rises well past it.
    capacity: Option<u64>,
    /// Bytes sent since `counting`, and the bit/s sent over the last whole
    /// [`SENDING`].
    counted: u64,
    counting: Option<Instant>,
    sending: Option<u64>,
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
            capacity: None,
            counted: 0,
            counting: None,
            sending: None,
        }
    }

    /// Takes `bytes` sent `now`, video and resends alike.
    pub fn sent(&mut self, now: Instant, bytes: usize) {
        self.count(now);
        self.counting.get_or_insert(now);
        self.counted += bytes as u64;
    }

    /// Ends the second being counted once it's whole, so what was sent
    /// follows a stream that stopped sending too.
    fn count(&mut self, now: Instant) {
        let Some(since) = self.counting else {
            return;
        };
        let elapsed = now.duration_since(since);
        if elapsed >= SENDING {
            self.sending = Some(self.counted * 8 * 1000 / elapsed.as_millis() as u64);
            self.counted = 0;
            self.counting = Some(now);
        }
    }

    /// The bit/s sent over the last whole second, once there was one.
    pub fn sending(&self) -> Option<u64> {
        self.sending
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
        self.count(now);
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
            // Below what went out, which the uplink didn't carry, as the
            // encoder may send less than the target.
            let sent = self
                .sending
                .map_or(self.target, |sending| sending.min(self.target));
            self.capacity = Some(sent);
            let lower = (sent * BACK_OFF / 100).max(FLOOR);
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
        // Only a rate the encoder came near was tried on the uplink.
        let sending = self.sending?;
        if self
            .capacity
            .is_some_and(|capacity| self.target > capacity * PAST_CAPACITY / 100)
        {
            self.capacity = None;
        }
        let step = match self.capacity {
            Some(capacity) if self.target >= capacity * NEAR_CAPACITY / 100 => capacity / 100,
            _ => self.target * RAMP / 100,
        };
        let higher = (self.target + step)
            .min(self.limit())
            .min(sending * HEADROOM / 100);
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

    /// Reports a second apart from `at`, each with `trip` and `lost`, after
    /// a second of sending at the target, as a busy picture does.
    fn reports(
        controller: &mut Controller,
        at: &mut Instant,
        count: usize,
        trip: Option<Duration>,
        lost: u8,
    ) -> Vec<Option<Reason>> {
        (0..count)
            .map(|_| {
                let target = controller.target();
                sending(controller, at, target);
                controller.report(*at, trip, lost)
            })
            .collect()
    }

    /// A second from `at` of sending `bits` a second.
    fn sending(controller: &mut Controller, at: &mut Instant, bits: u64) {
        controller.sent(*at, bits.div_ceil(8) as usize);
        *at += Duration::from_secs(1);
        controller.sent(*at, 0);
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
    fn does_not_rise_far_past_what_is_sent() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        // A calm picture: the encoder sends a fifth of the target.
        for _ in 0..30 {
            sending(&mut controller, &mut at, 500_000);
            controller.report(at, Some(ms(40)), 0);
        }
        assert_eq!(controller.sending(), Some(500_000));
        assert_eq!(controller.target(), 2_500_000, "never lowered by it either");
        // Busy again, it rises as the encoder follows.
        reports(&mut controller, &mut at, 3, Some(ms(40)), 0);
        assert_eq!(
            controller.target(),
            2_500_000 * 108 / 100 * 108 / 100 * 108 / 100
        );
    }

    #[test]
    fn does_not_rise_without_knowing_what_was_sent() {
        let mut controller = Controller::new(CAP);
        let at = Instant::now();
        assert_eq!(controller.report(at, Some(ms(40)), 0), None);
        assert_eq!(controller.target(), 2_500_000);
    }

    #[test]
    fn backs_off_below_what_was_sent() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        reports(&mut controller, &mut at, 20, Some(ms(40)), 0);
        assert_eq!(controller.target(), CAP);
        // The encoder sent 4 Mbit/s of the 9 it may; the uplink didn't carry it.
        sending(&mut controller, &mut at, 4_000_000);
        assert_eq!(controller.report(at, None, 40), Some(Reason::Loss));
        assert_eq!(controller.target(), 4_000_000 * 85 / 100);
    }

    #[test]
    fn rises_slowly_near_the_rate_it_backed_off_from() {
        let mut controller = Controller::new(CAP);
        let mut at = Instant::now();
        reports(&mut controller, &mut at, 5, Some(ms(40)), 0);
        let capacity = controller.target();
        reports(&mut controller, &mut at, 1, None, 40);
        assert_eq!(controller.target(), capacity * 85 / 100);
        // Held, then a hundredth of that rate a report.
        reports(&mut controller, &mut at, 3, Some(ms(40)), 0);
        let after = controller.target();
        reports(&mut controller, &mut at, 1, Some(ms(40)), 0);
        assert_eq!(controller.target(), after + capacity / 100);
        // Well past it, the uplink carries more, and it rises faster again.
        let mut reached = None;
        for _ in 0..60 {
            let before = controller.target();
            reports(&mut controller, &mut at, 1, Some(ms(40)), 0);
            if controller.target() - before > capacity / 100 {
                reached = Some(before);
                break;
            }
        }
        let reached = reached.expect("rose faster again");
        assert!(reached > capacity * 115 / 100, "{reached} of {capacity}");
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
