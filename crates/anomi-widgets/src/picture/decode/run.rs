//! One run round an animation's frames: a thread that decodes them in order
//! and hands each over when it's taken, and what the window holds of it.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use iced::widget::image;

use super::super::look::{self, Look};
use super::super::moving::Moving;

/// One run round an animation's frames, as the window sees it.
pub(super) struct Run {
    pub(super) hand: Arc<Hand>,
    /// The frame to come next.
    pub(super) next: usize,
    pub(super) shown: Option<(usize, image::Handle)>,
    pub(super) used: Instant,
}

/// Where a run's thread hands a frame over: one at a time, as a channel of
/// no room does, so it decodes one ahead and waits.
#[derive(Default)]
pub(super) struct Hand {
    state: Mutex<Handing>,
    taken: Condvar,
}

#[derive(Default)]
pub(super) struct Handing {
    pub(super) frame: Option<(usize, image::Handle)>,
    /// The frame asked for, which the thread passes over others to reach.
    pub(super) wanted: usize,
    /// The window let go of the run.
    pub(super) closed: bool,
    /// The thread ended: nothing took its frame, or the frames ran out.
    pub(super) ended: bool,
    /// The source gave only this many frames, fewer than its delays, as
    /// a file that won't open or a GIF corrupt part way: no run gives more.
    pub(super) gives: Option<usize>,
}

impl Hand {
    pub(super) fn lock(&self) -> MutexGuard<'_, Handing> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Hands `frame` over and waits for it to be taken; false once the run
    /// should end: let go of, or nothing took the frame within `idle`.
    fn give(&self, index: usize, frame: image::Handle, idle: Duration) -> bool {
        let mut state = self.lock();
        state.frame = Some((index, frame));
        let until = Instant::now() + idle;
        loop {
            if state.closed {
                state.frame = None;
                return false;
            }
            if state.frame.is_none() {
                return true;
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                // Not held twice while nobody looks.
                state.frame = None;
                return false;
            }
            state = self
                .taken
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

impl Run {
    pub(super) fn start(
        source: &Arc<dyn Moving>,
        look: Look,
        row: usize,
        idle: Duration,
        now: Instant,
    ) -> Self {
        let hand = Arc::new(Hand::default());
        hand.lock().wanted = row;
        let started = {
            let (source, hand) = (source.clone(), hand.clone());
            std::thread::Builder::new()
                .name("anomi-frames".into())
                .spawn(move || {
                    decode(source.as_ref(), look, &hand, idle);
                    hand.lock().ended = true;
                })
        };
        if started.is_err() {
            hand.lock().ended = true;
        }
        Self {
            hand,
            next: row,
            shown: None,
            used: now,
        }
    }

    pub(super) fn ended(&self) -> bool {
        self.hand.lock().ended
    }

    /// Asks for `row`, and takes the frame waiting if it comes before it or
    /// is it, then asks for the one after; one not decoded yet leaves the
    /// one before showing, and one after it waits there to be asked for.
    pub(super) fn advance(&mut self, row: usize, count: usize) {
        let mut state = self.hand.lock();
        state.wanted = row;
        let due = state
            .frame
            .as_ref()
            .is_some_and(|(index, _)| not_past(*index, row, count));
        if !due {
            return;
        }
        if let Some((index, frame)) = state.frame.take() {
            self.next = (index + 1) % count;
            self.shown = Some((index, frame));
            // The next one due comes next: a run that fell behind passes
            // over the frame it would have been late with, and catches up.
            state.wanted = (row + 1) % count;
            self.hand.taken.notify_all();
        }
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let mut state = self.hand.lock();
        state.closed = true;
        state.frame = None;
        self.hand.taken.notify_all();
    }
}

/// Hands over `source`'s frames as `look` draws them, in order and round
/// again, from the one wanted, until nothing takes them. A source that
/// runs out before its delays do is read once: the run says how many it
/// gave and ends.
fn decode(source: &dyn Moving, look: Look, hand: &Hand, idle: Duration) {
    let count = source.delays().len();
    let (width, height) = source.size();
    // The first time round it starts where it's wanted: the frames before
    // were the run's past, not the next round's.
    let mut first = true;
    loop {
        // A run that hands over nothing would never hand over anything.
        let mut gave = false;
        let mut read = 0;
        for (index, rgba) in source.frames().take(count).enumerate() {
            read = index + 1;
            let wanted = {
                let state = hand.lock();
                if state.closed {
                    return;
                }
                state.wanted
            };
            let behind = if first {
                index < wanted
            } else {
                index != wanted && not_past(index, wanted, count)
            };
            if behind {
                continue;
            }
            let pixels = look::make(&rgba, width, height, &look);
            // Not held twice while it waits to be taken.
            drop(rgba);
            let frame = image::Handle::from_rgba(look.width, look.height, pixels);
            if !hand.give(index, frame, idle) {
                return;
            }
            gave = true;
        }
        if read < count {
            hand.lock().gives = Some(read);
            return;
        }
        if !gave {
            return;
        }
        first = false;
    }
}

/// Whether frame `at` comes before `row` or is it, going round `count`
/// frames: less than half the way round from it.
pub(super) fn not_past(at: usize, row: usize, count: usize) -> bool {
    count > 0 && (row + count - at % count) % count < count.div_ceil(2)
}
