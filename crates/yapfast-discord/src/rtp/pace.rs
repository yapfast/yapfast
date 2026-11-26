//! How a run of packets goes out on the wire: a burst, then paced.

use super::*;

/// A run of the packets a viewer asked for again starts with this many of
/// them at once, then a batch a turn at the factor [`Pace::batch`] is given:
/// a NACK for a hundred packets must not overflow the path's buffers.
pub(crate) const BURST: usize = 8;
/// How long a batch past the first waits after the one before it. A batch is a
/// turn of the send loop, so sound, the gateway and viewers' feedback are
/// polled between them rather than after the whole run.
pub(crate) const STEP: Duration = Duration::from_millis(1);
/// The bytes a packet's datagram takes, its payload and headers, which a
/// batch's size is paid for in.
pub(crate) const PACE_PACKET: usize = 1_250;
/// The most packets a turn writes however long it waited, so the loop still
/// polls sound, the gateway and feedback: 64 a turn of Windows' 15.6 ms is
/// 41 Mbit/s.
pub(crate) const MAX_BATCH: usize = 64;

/// The pacing a run of the packets a viewer asked for again goes out at, as
/// the count of what a turn writes: the first [`BURST`] at once, then a batch
/// a turn.
pub(crate) struct Pace {
    /// How many of the run's packets are out, which decides the next batch's
    /// size while the run is in its burst.
    sent: usize,
    /// When the turn before this one wrote its batch, which this one is paid
    /// for over.
    last: Instant,
    /// The bits paid for and not yet sent, less than a packet's but for a
    /// turn the bound cut short: a 1 ms turn at most rates pays for a packet
    /// and a fraction, and the fraction is the next turn's.
    credit: u64,
}

impl Pace {
    /// A run whose first batch is due at once.
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            sent: 0,
            last: now,
            credit: 0,
        }
    }

    /// How many of the run's packets this turn writes: what's left of its
    /// first [`BURST`], then what `factor` hundredths of the target pays for
    /// in the time since the batch before, with what the turns before it paid
    /// for and didn't send. A turn that hasn't paid for a whole packet writes
    /// none and waits another [`STEP`].
    ///
    /// Paid for by the time the turn took, a coarse timer gets the rate a fine
    /// one does: Windows serves a 1 ms sleep in 15.6 ms, where a fixed batch
    /// would put a 20 Mbit stream out at under four. And with the fraction
    /// carried, a fine one gets the rate a coarse one does: a 1 ms turn of an
    /// 8.7 Mbit stream pays for two packets and a fifth, where rounding up to
    /// a floor of four sent it at 40 Mbit.
    pub(crate) fn batch(&mut self, target: u64, factor: u64, now: Instant) -> usize {
        if self.sent < BURST {
            return BURST - self.sent;
        }
        let waited = now.duration_since(self.last).as_micros() as u64;
        let packet = 8 * PACE_PACKET as u64;
        // Never more than the bound pays for: a turn that came late sends what
        // the bound lets it, and the rest of the wait isn't owed as a burst.
        let paid = target.saturating_mul(factor).saturating_mul(waited) / (100 * 1_000_000);
        self.credit = paid
            .saturating_add(self.credit)
            .min(MAX_BATCH as u64 * packet);
        let batch = self.credit / packet;
        self.credit -= batch * packet;
        batch as usize
    }

    /// Notes that one of the run's packets went out.
    pub(crate) fn went(&mut self) {
        self.sent += 1;
    }

    /// Notes that `batch`'s turn is over, `now` being when it was: the next
    /// turn is paid for over the time from here, so a turn that wrote nothing
    /// pays for its own wait and no more.
    pub(crate) fn wrote(&mut self, now: Instant) {
        self.last = now;
    }

    /// When the next batch may go: at once while the run is in its burst, and
    /// [`STEP`] after the batch before it otherwise.
    pub(crate) fn due(&self, now: Instant) -> Instant {
        if self.sent < BURST {
            now
        } else {
            self.last + STEP
        }
    }
}
