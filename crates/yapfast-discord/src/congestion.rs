//! How fast a Go Live stream sends: a sender-side controller that keeps the
//! uplink's queue short. Discord's client runs libwebrtc's delay-based
//! control on transport-wide feedback; Yapfast reads the round trip and the
//! loss from the receiver reports Discord's stream server sends about the
//! video, which a queue at the sender's router lengthens too.
//!
//! It starts below the cap, raises the target a little each report while
//! the round trip stays near the lowest seen, and lowers it below what the
//! server received when the round trip climbs well over that or packets are
//! lost, then holds. It doesn't rise far past what the encoder sends, so a
//! calm picture can't take it to a rate the uplink never carried, and near
//! the rate it last backed off from it rises slowly. The cap is Discord's
//! for the stream's height, or the server's estimate (REMB) when that's
//! lower.

use std::{
    collections::VecDeque,
    fmt,
    time::{Duration, Instant},
};

use crate::rtp::Reception;

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
/// Each drop takes the target to this share, in hundredths, of what the
/// server received.
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
/// What was sent is measured over this long, in slices of [`SLICE`], so it
/// follows a picture that gets busy within a report or two.
const SENDING: Duration = Duration::from_millis(500);
const SLICE: Duration = Duration::from_millis(50);
/// What the server received is measured between reports at least this far
/// apart; Discord's come about two a second.
const RECEIVING: Duration = Duration::from_millis(400);
/// A rate received past this many times the cap is a report that doesn't
/// add up, as after the sequence numbers restart.
const UNLIKELY: u64 = 4;
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

/// What went out in [`SLICE`] from `at`.
struct Slice {
    at: Instant,
    bytes: u64,
    packets: u64,
}

/// Where a receiver report left the server's count of the video: when it
/// came, the highest sequence number received, extended, and the packets
/// lost so far.
#[derive(Clone, Copy)]
struct Received {
    at: Instant,
    highest: u32,
    lost: u32,
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
    /// The bit/s the server received when it last backed off, which is what
    /// the uplink carried then, until the target rises well past it.
    capacity: Option<u64>,
    /// What went out within [`SENDING`], oldest first, video and resends,
    /// and when the first packet did.
    slices: VecDeque<Slice>,
    first: Option<Instant>,
    /// The report the bit/s received is measured from, and that bit/s.
    counted: Option<Received>,
    received: Option<u64>,
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
            slices: VecDeque::new(),
            first: None,
            counted: None,
            received: None,
        }
    }

    /// Takes a packet of `bytes` sent `now`, video or a resend.
    pub fn sent(&mut self, now: Instant, bytes: usize) {
        self.forget_sent(now);
        self.first.get_or_insert(now);
        match self.slices.back_mut() {
            Some(slice) if now.duration_since(slice.at) < SLICE => {
                slice.bytes += bytes as u64;
                slice.packets += 1;
            }
            _ => self.slices.push_back(Slice {
                at: now,
                bytes: bytes as u64,
                packets: 1,
            }),
        }
    }

    fn forget_sent(&mut self, now: Instant) {
        while self
            .slices
            .front()
            .is_some_and(|slice| now.duration_since(slice.at) >= SENDING)
        {
            self.slices.pop_front();
        }
    }

    /// The bit/s sent over the last [`SENDING`], once the stream has sent
    /// for that long.
    pub fn sending(&self, now: Instant) -> Option<u64> {
        let first = self.first?;
        if now.duration_since(first) < SENDING {
            return None;
        }
        let bytes: u64 = self
            .slices
            .iter()
            .filter(|slice| now.duration_since(slice.at) < SENDING)
            .map(|slice| slice.bytes)
            .sum();
        Some(bytes * 8 * 1000 / SENDING.as_millis() as u64)
    }

    /// The bytes of the packets sent lately, on average.
    fn packet_size(&self) -> Option<u64> {
        let (bytes, packets) = self.slices.iter().fold((0, 0), |(bytes, packets), slice| {
            (bytes + slice.bytes, packets + slice.packets)
        });
        (packets > 0).then(|| bytes / packets)
    }

    /// The bit/s the server received between its last reports at least
    /// [`RECEIVING`] apart.
    pub fn received(&self) -> Option<u64> {
        self.received
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

    /// Counts the packets the server received since the report it counts
    /// from, as the highest sequence number grew less the packets lost
    /// meanwhile, at the size of those sent lately.
    fn count_received(&mut self, now: Instant, reception: &Reception) {
        let this = Received {
            at: now,
            highest: reception.highest,
            lost: reception.lost,
        };
        let Some(from) = self.counted else {
            self.counted = Some(this);
            return;
        };
        let elapsed = now.duration_since(from.at);
        if elapsed < RECEIVING {
            return;
        }
        self.counted = Some(this);
        // The count of packets lost is 24 bits, and signed: duplicates can
        // take it down.
        let lost = i64::from(((this.lost.wrapping_sub(from.lost) << 8) as i32) >> 8);
        let arrived = (i64::from(this.highest.wrapping_sub(from.highest)) - lost).max(0) as u64;
        self.received = self
            .packet_size()
            .map(|size| arrived * size * 8 * 1000 / elapsed.as_millis() as u64)
            .filter(|&received| received <= self.cap * UNLIKELY);
    }

    /// Takes a receiver report about the video, received `now`: the round
    /// trip it measured, if it echoed a sender report this end still
    /// knows, and what it says the server received and lost.
    pub fn report(
        &mut self,
        now: Instant,
        trip: Option<Duration>,
        reception: &Reception,
    ) -> Option<Reason> {
        self.forget_sent(now);
        self.count_received(now, reception);
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
        let fraction_lost = reception.fraction_lost;
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
            // Below what the uplink carried, which is what the server got
            // while the queue built, however much more went out: a picture
            // that turned busy at once sends far past it.
            let carried = self
                .received
                .or(self.sending(now))
                .map_or(self.target, |carried| carried.min(self.target));
            self.capacity = Some(carried);
            let lower = (carried * BACK_OFF / 100).max(FLOOR);
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
        let sending = self.sending(now)?;
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
    /// The bytes of each packet the tests send.
    const PACKET: u64 = 1200;

    fn ms(milliseconds: u64) -> Duration {
        Duration::from_millis(milliseconds)
    }

    /// A stream on an uplink: what it sends, what the server gets of it,
    /// and the reports the server sends back.
    struct Uplink {
        controller: Controller,
        at: Instant,
        /// Bytes owed to the next packet, so a rate that isn't a whole
        /// number of packets a step still adds up.
        owed: u64,
        /// The server's count: the highest sequence number and packets
        /// lost so far.
        highest: u32,
        lost: u32,
    }

    impl Uplink {
        fn new(cap: u64) -> Self {
            Self {
                controller: Controller::new(cap),
                at: Instant::now(),
                owed: 0,
                highest: 0,
                lost: 0,
            }
        }

        fn target(&self) -> u64 {
            self.controller.target()
        }

        /// Sends `bits` a second for `time`, in 10 ms steps, of which the
        /// server gets `carried` a second; the rest is still queued.
        fn send(&mut self, time: Duration, bits: u64, carried: u64) {
            for _ in 0..time.as_millis() / 10 {
                self.at += ms(10);
                self.owed += bits / 800;
                while self.owed >= PACKET {
                    self.owed -= PACKET;
                    self.controller.sent(self.at, PACKET as usize);
                }
            }
            let arrived = carried * time.as_millis() as u64 / 1000 / 8 / PACKET;
            self.highest = self.highest.wrapping_add(arrived as u32);
        }

        /// The report the server sends, with `trip` and `lost` packets out
        /// of 256 lost since the last.
        fn report(&mut self, trip: Option<Duration>, lost: u8) -> Option<Reason> {
            let reception = Reception {
                fraction_lost: lost,
                highest: self.highest,
                lost: self.lost,
                ..Reception::default()
            };
            self.controller.report(self.at, trip, &reception)
        }

        /// Seconds of sending `bits` a second, all carried, each followed
        /// by a report with `trip` and `lost`.
        fn seconds(
            &mut self,
            count: usize,
            bits: u64,
            trip: Option<Duration>,
            lost: u8,
        ) -> Vec<Option<Reason>> {
            (0..count)
                .map(|_| {
                    self.send(Duration::from_secs(1), bits, bits);
                    self.report(trip, lost)
                })
                .collect()
        }

        /// Seconds of sending at the target, as a busy picture does.
        fn busy(&mut self, count: usize, trip: Option<Duration>, lost: u8) -> Vec<Option<Reason>> {
            (0..count)
                .map(|_| {
                    let target = self.target();
                    self.seconds(1, target, trip, lost)[0]
                })
                .collect()
        }
    }

    /// Whether `rate` is within a hundredth of `wanted`, as packets of
    /// [`PACKET`] measure it.
    fn near(rate: u64, wanted: u64) -> bool {
        rate.abs_diff(wanted) <= wanted / 100
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
        let mut uplink = Uplink::new(CAP);
        let said = uplink.busy(3, Some(ms(40)), 0);
        assert!(
            said.iter().all(|&said| said == Some(Reason::Ramp)),
            "{said:?}"
        );
        assert_eq!(
            uplink.target(),
            2_500_000 * 108 / 100 * 108 / 100 * 108 / 100
        );
        uplink.busy(30, Some(ms(50)), 0);
        assert_eq!(uplink.target(), CAP, "within NEAR of the lowest");
        assert_eq!(uplink.controller.baseline(), Some(ms(40)));
    }

    #[test]
    fn does_not_ramp_without_a_round_trip() {
        let mut uplink = Uplink::new(CAP);
        let said = uplink.busy(10, None, 0);
        assert!(said.iter().all(Option::is_none));
        assert_eq!(uplink.target(), 2_500_000);
    }

    #[test]
    fn backs_off_when_the_round_trip_stays_up() {
        let mut uplink = Uplink::new(CAP);
        uplink.busy(20, Some(ms(50)), 0);
        let before = uplink.target();
        // Between NEAR and QUEUING: holds.
        assert_eq!(uplink.busy(2, Some(ms(90)), 0), [None, None]);
        assert_eq!(uplink.target(), before);
        // One report over: may be a keyframe; two in a row is a queue.
        assert_eq!(
            uplink.busy(2, Some(ms(300)), 0),
            [None, Some(Reason::Queuing)]
        );
        assert!(
            near(uplink.target(), before * 85 / 100),
            "{}",
            uplink.target()
        );
        // Held a while, even with the round trip short again.
        assert_eq!(uplink.busy(2, Some(ms(50)), 0), [None, None]);
        assert_eq!(uplink.busy(1, Some(ms(50)), 0), [Some(Reason::Ramp)]);
    }

    #[test]
    fn a_lone_slow_report_does_not_back_off() {
        let mut uplink = Uplink::new(CAP);
        uplink.busy(2, Some(ms(50)), 0);
        let said = [
            uplink.busy(1, Some(ms(300)), 0),
            uplink.busy(1, Some(ms(50)), 0),
            uplink.busy(1, Some(ms(300)), 0),
        ];
        assert!(!said.concat().contains(&Some(Reason::Queuing)), "{said:?}");
    }

    #[test]
    fn backs_off_on_loss_and_holds_on_a_little() {
        let mut uplink = Uplink::new(CAP);
        uplink.busy(5, Some(ms(50)), 0);
        let before = uplink.target();
        assert_eq!(uplink.busy(1, Some(ms(50)), 10), [None], "4%: holds");
        assert_eq!(uplink.target(), before);
        assert_eq!(uplink.busy(1, Some(ms(50)), 40), [Some(Reason::Loss)]);
        assert!(
            near(uplink.target(), before * 85 / 100),
            "{}",
            uplink.target()
        );
        assert_eq!(
            uplink.busy(1, None, 40),
            [Some(Reason::Loss)],
            "without a round trip too"
        );
    }

    #[test]
    fn never_goes_below_the_floor() {
        let mut uplink = Uplink::new(CAP);
        let said = uplink.busy(40, None, 255);
        assert_eq!(uplink.target(), FLOOR);
        assert_eq!(said.last(), Some(&None), "nothing left to drop");
    }

    #[test]
    fn follows_the_cap_and_discords_estimate() {
        let mut uplink = Uplink::new(CAP);
        assert_eq!(uplink.controller.set_estimate(2_000_000), Some(Reason::Cap));
        assert_eq!(uplink.target(), 2_000_000);
        uplink.busy(10, Some(ms(50)), 0);
        assert_eq!(uplink.target(), 2_000_000, "never past the estimate");
        assert_eq!(uplink.controller.set_estimate(100_000), Some(Reason::Cap));
        assert_eq!(uplink.target(), FLOOR, "nor below the floor");
        assert_eq!(uplink.controller.set_estimate(20_000_000), None);
        uplink.busy(40, Some(ms(50)), 0);
        assert_eq!(
            uplink.target(),
            CAP,
            "an estimate past the cap leaves the cap"
        );
        assert_eq!(uplink.controller.set_cap(3_500_000), Some(Reason::Cap));
        assert_eq!(uplink.target(), 3_500_000);
    }

    #[test]
    fn does_not_rise_far_past_what_is_sent() {
        let mut uplink = Uplink::new(CAP);
        // A calm picture: the encoder sends a fifth of the target.
        uplink.seconds(30, 500_000, Some(ms(40)), 0);
        let sending = uplink.controller.sending(uplink.at).unwrap();
        assert!(near(sending, 500_000), "{sending}");
        assert_eq!(uplink.target(), 2_500_000, "never lowered by it either");
        // Busy again, it rises as the encoder follows.
        uplink.busy(3, Some(ms(40)), 0);
        assert_eq!(
            uplink.target(),
            2_500_000 * 108 / 100 * 108 / 100 * 108 / 100
        );
    }

    #[test]
    fn does_not_rise_without_knowing_what_was_sent() {
        let mut uplink = Uplink::new(CAP);
        assert_eq!(uplink.report(Some(ms(40)), 0), None);
        assert_eq!(uplink.target(), 2_500_000);
    }

    #[test]
    fn follows_what_is_sent_within_half_a_second() {
        let mut uplink = Uplink::new(CAP);
        uplink.send(Duration::from_secs(3), 1_000_000, 1_000_000);
        uplink.send(ms(500), 6_000_000, 6_000_000);
        let sending = uplink.controller.sending(uplink.at).unwrap();
        assert!(near(sending, 6_000_000), "{sending}");
        // And a stream that stopped sending has sent nothing.
        uplink.send(ms(500), 0, 0);
        assert_eq!(uplink.controller.sending(uplink.at), Some(0));
    }

    #[test]
    fn backs_off_below_what_was_received() {
        let mut uplink = Uplink::new(CAP);
        uplink.busy(20, Some(ms(40)), 0);
        assert_eq!(uplink.target(), CAP);
        // The encoder sent 4 Mbit/s of the 9 it may; the uplink didn't carry it.
        uplink.seconds(1, 4_000_000, None, 40);
        assert!(
            near(uplink.target(), 4_000_000 * 85 / 100),
            "{}",
            uplink.target()
        );
    }

    /// A calm picture, then a busy one at once, on an uplink that carries
    /// less than when the target was reached: one back-off takes it below
    /// what the uplink carries, however far past it the jump sent.
    #[test]
    fn backs_off_to_what_the_uplink_carried_after_a_jump() {
        let mut uplink = Uplink::new(CAP);
        uplink.busy(20, Some(ms(40)), 0);
        assert_eq!(uplink.target(), CAP);
        uplink.seconds(20, 1_000_000, Some(ms(40)), 0);
        assert_eq!(uplink.target(), CAP, "a calm picture leaves the target");
        // The uplink now carries 5 Mbit/s; the picture turns busy and sends 9.
        let mut said = vec![];
        for trip in [200, 400] {
            uplink.send(ms(500), CAP, 5_000_000);
            said.push(uplink.report(Some(ms(trip)), 0));
        }
        assert_eq!(said, [None, Some(Reason::Queuing)]);
        let target = uplink.target();
        assert!(near(target, 5_000_000 * 85 / 100), "{target}");
        // It remembers what the uplink carried, and comes back to it slowly.
        uplink.busy(3, Some(ms(40)), 0);
        let after = uplink.target();
        uplink.busy(1, Some(ms(40)), 0);
        let step = uplink.target() - after;
        assert!(near(step, 50_000), "{step}");
    }

    #[test]
    fn rises_slowly_near_the_rate_it_backed_off_from() {
        let mut uplink = Uplink::new(CAP);
        uplink.busy(5, Some(ms(40)), 0);
        let capacity = uplink.target();
        uplink.busy(1, None, 40);
        assert!(near(uplink.target(), capacity * 85 / 100));
        // Held, then a hundredth of that rate a report.
        uplink.busy(3, Some(ms(40)), 0);
        let after = uplink.target();
        uplink.busy(1, Some(ms(40)), 0);
        let step = uplink.target() - after;
        assert!(near(step, capacity / 100), "{step} of {capacity}");
        // Well past it, the uplink carries more, and it rises faster again.
        let mut reached = None;
        for _ in 0..60 {
            let before = uplink.target();
            uplink.busy(1, Some(ms(40)), 0);
            if uplink.target() - before > capacity * 2 / 100 {
                reached = Some(before);
                break;
            }
        }
        let reached = reached.expect("rose faster again");
        assert!(reached > capacity * 115 / 100, "{reached} of {capacity}");
    }

    #[test]
    fn ignores_a_received_count_that_does_not_add_up() {
        let mut uplink = Uplink::new(CAP);
        uplink.busy(2, Some(ms(40)), 0);
        // The sequence numbers jumped, as when the server starts over.
        uplink.highest = uplink.highest.wrapping_add(1 << 30);
        uplink.busy(1, Some(ms(40)), 0);
        assert_eq!(uplink.controller.received(), None);
    }

    #[test]
    fn forgets_round_trips_from_long_ago() {
        let mut uplink = Uplink::new(CAP);
        uplink.busy(1, Some(ms(20)), 0);
        uplink.busy(61, Some(ms(150)), 0);
        assert_eq!(
            uplink.controller.baseline(),
            Some(ms(150)),
            "the path changed"
        );
    }
}
