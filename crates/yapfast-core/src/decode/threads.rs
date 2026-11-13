//! `decode.rs`'s threads.

use super::*;

/// The most threads one decoder asks for. A stream shown large comes off
/// four threads close to real time, while each of them is a reference picture
/// the codec keeps, so past four the work is heavier for little gain.
pub(crate) const MOST_THREADS: usize = 4;

/// One stream's share of a machine with `cores` that is decoding `streams` of
/// them at once: the cores divided among the streams, up to the four one
/// stream alone takes, and never less than the one thread a decoder can't do
/// without. A stream past the cores oversubscribes by that one thread rather
/// than waiting for another stream to finish.
pub fn share(cores: usize, streams: usize) -> usize {
    (cores / streams.max(1)).clamp(1, MOST_THREADS)
}

/// The decoder threads everything decoding in this process shares.
///
/// Each decoder asking the machine what it has, as if it were alone on it,
/// leaves the sum of them to chance: a watch that turns to another codec, a
/// second decoder beside the first, and two streams on a four-core machine
/// would each take four threads.
pub(crate) static THREADS: Budget = Budget::new();

/// The threads this process's decoders may use between them: what the Yapfast
/// that started this worker worked out for it, or what one decoder asks for
/// when a worker was started by hand.
pub(crate) static ALLOWED: AtomicUsize = AtomicUsize::new(MOST_THREADS);

/// Sets the threads this process's decoders may use between them, which the
/// Yapfast that started a stream worker works out from the machine's cores and
/// how many streams it's decoding at once. One thread is the floor, and four
/// the most one decoder asks for; anything else is ignored.
pub fn allow(threads: usize) {
    ALLOWED.store(threads.clamp(1, MOST_THREADS), Ordering::Relaxed);
}

/// The threads decoders in one process may use between them, handed out a
/// decoder at a time and given back when it goes.
pub struct Budget {
    pub(crate) claimed: AtomicUsize,
}

impl Budget {
    pub(crate) const fn new() -> Self {
        Self {
            claimed: AtomicUsize::new(0),
        }
    }

    /// Claims threads for one decoder out of `allowed`: up to four, as many
    /// as the budget has left, and at least one. A decoder on no threads
    /// decodes nothing, so a stream past the budget oversubscribes by the one
    /// thread it can't do without rather than waiting for another stream to
    /// finish.
    pub(crate) fn claim(&self, allowed: usize) -> Threads<'_> {
        let want = allowed.min(MOST_THREADS);
        let mut claimed = self.claimed.load(Ordering::Relaxed);
        loop {
            let threads = want.min(allowed.saturating_sub(claimed)).max(1);
            match self.claimed.compare_exchange_weak(
                claimed,
                claimed + threads,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    return Threads {
                        budget: self,
                        threads: threads as u32,
                    };
                }
                Err(moved) => claimed = moved,
            }
        }
    }

    /// How many threads this process's decoders hold now.
    pub fn claimed(&self) -> usize {
        self.claimed.load(Ordering::Relaxed)
    }
}

/// One decoder's share of a [`Budget`], given back when the decoder goes.
pub struct Threads<'a> {
    pub(crate) budget: &'a Budget,
    pub(crate) threads: u32,
}

impl Threads<'_> {
    pub fn count(&self) -> u32 {
        self.threads
    }
}

impl Drop for Threads<'_> {
    fn drop(&mut self) {
        self.budget
            .claimed
            .fetch_sub(self.threads as usize, Ordering::AcqRel);
    }
}
