//! The buffers an encoded frame travels in, kept between frames so a stream
//! allocates nothing per frame.

use crate::lock;

use std::{
    ops::Deref,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

/// The buffers one encoder's frames are encoded into, reused as their frames
/// are sent. [`Pool::checkout`] hands one out and [`Encoded`]'s drop brings
/// it back; neither ever waits for one to come back.
/// The most buffers a [`Pool`] keeps: one being encoded into, one queued to
/// send, and one being sent. The queue is four frames deep, so a queue that
/// fills allocates a buffer per frame beyond these and frees it with the
/// frame: memory is a product goal, and a pool that kept pace with a stall
/// would hold it for a burst that may not come.
pub const BUFFERS: usize = 3;
/// Where a [`Pool`]'s buffers start: two bytes a pixel is room for a keyframe
/// of noise, but a stream's frames are a small fraction of that, and most
/// never need more. One that does grows the pool.
const START: usize = 512 * 1024;

pub struct Pool {
    /// The size buffers are handed out at now: a frame too big grows it.
    capacity: AtomicUsize,
    /// The most a frame of the encoder's size can need.
    max: usize,
    idle: Mutex<Vec<Vec<u8>>>,
    created: AtomicUsize,
    /// The bytes the pool's buffers hold now, idle and out on loan, and the
    /// most they have held: a stream's report shows both, so a pool that grew
    /// for one large frame says what it kept rather than how many buffers it
    /// made.
    live: AtomicUsize,
    peak: AtomicUsize,
}

/// The encoded bytes of one frame, in a buffer from its encoder's [`Pool`].
pub struct Encoded {
    buffer: Vec<u8>,
    len: usize,
    pool: Option<Arc<Pool>>,
}

impl Pool {
    /// A pool of buffers for an encoder's frames, which never need more than
    /// `max` bytes: enough for a keyframe of noise. They start at [`START`],
    /// or at `max` where that is already smaller.
    pub fn new(max: usize) -> Self {
        Self {
            capacity: AtomicUsize::new(max.min(START)),
            max,
            idle: Mutex::new(Vec::new()),
            created: AtomicUsize::new(0),
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }
    }

    /// A buffer of the pool's current size to encode a frame into: an idle
    /// one, or a new one when every buffer is still out.
    pub fn checkout(self: &Arc<Self>) -> Encoded {
        let capacity = self.capacity.load(Ordering::Relaxed);
        let idle = lock(&self.idle).pop();
        let buffer = match idle {
            Some(buffer) if buffer.capacity() == capacity => buffer,
            // A buffer of a size the pool has outgrown is no use to it: it is
            // let go, and what the pool holds goes down with it.
            stale => {
                if let Some(stale) = stale {
                    self.release(stale.capacity());
                }
                self.created.fetch_add(1, Ordering::Relaxed);
                self.hold(capacity);
                vec![0; capacity]
            }
        };
        Encoded {
            buffer,
            len: capacity,
            pool: Some(self.clone()),
        }
    }

    /// Doubles the size buffers are handed out at, up to the pool's max, for
    /// a frame too big for them: false when it is already as large as a
    /// frame of the encoder's size can need, so the caller gives up.
    pub fn grow(&self) -> bool {
        let capacity = self.capacity.load(Ordering::Relaxed);
        if capacity >= self.max {
            return false;
        }
        // The idle buffers of the size this leaves are ones the pool will never
        // hand out again: a checkout drops one it has outgrown, so they would sit
        // under the stack's new top for the rest of the stream, held for nothing.
        // Growing is the one moment they are all in one place to give back, so it
        // is where they go; a buffer still out is dropped by `Encoded`'s own size
        // check when it comes back.
        self.trim();
        self.capacity
            .store((capacity * 2).min(self.max), Ordering::Relaxed);
        true
    }

    /// The size of buffer it hands out now.
    pub fn capacity(&self) -> usize {
        self.capacity.load(Ordering::Relaxed)
    }

    /// The bytes the pool's buffers hold now, idle and out on loan, and the most
    /// they have held: a stream's report shows both, so a pool that grew for one
    /// large frame says what it kept rather than how many buffers it made.
    pub fn bytes(&self) -> (usize, usize) {
        (
            self.live.load(Ordering::Relaxed),
            self.peak.load(Ordering::Relaxed),
        )
    }

    /// Gives back the idle buffers now, for a machine short of memory. What is
    /// out on loan comes back to the pool's own check and stays or goes by the
    /// size it was made at, so nothing in flight is ever resized or freed.
    pub fn trim(&self) {
        let mut idle = lock(&self.idle);
        let bytes: usize = idle.iter().map(Vec::capacity).sum();
        idle.clear();
        drop(idle);
        self.release(bytes);
    }

    /// Counts a buffer the pool has just made, and the high-water mark with it.
    fn hold(&self, capacity: usize) {
        let live = self.live.fetch_add(capacity, Ordering::Relaxed) + capacity;
        self.peak.fetch_max(live, Ordering::Relaxed);
    }

    /// Counts a buffer the pool has let go of.
    fn release(&self, capacity: usize) {
        self.live.fetch_sub(capacity, Ordering::Relaxed);
    }

    /// How many buffers the pool has allocated so far, for tests and logs: a
    /// pool that keeps allocating is one the stream is outrunning.
    pub fn created(&self) -> usize {
        self.created.load(Ordering::Relaxed)
    }
}

impl Encoded {
    /// The whole buffer, for an encoder to write a frame into: [`Self::finish`]
    /// says how much of it the frame is.
    pub fn spare(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    /// The number of bytes of [`Self::spare`] the encoder wrote.
    pub fn finish(&mut self, len: usize) {
        debug_assert!(len <= self.buffer.len());
        self.len = len;
    }
}

impl Deref for Encoded {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buffer[..self.len]
    }
}

impl From<Vec<u8>> for Encoded {
    fn from(buffer: Vec<u8>) -> Self {
        let len = buffer.len();
        Self {
            buffer,
            len,
            pool: None,
        }
    }
}

impl Drop for Encoded {
    fn drop(&mut self) {
        let Some(pool) = self.pool.take() else {
            return;
        };
        let capacity = self.buffer.capacity();
        let buffer = std::mem::take(&mut self.buffer);
        // A buffer another size than the pool's is no use to it, and neither is
        // one past the few a stream works through: both are let go, and what the
        // pool holds goes down with them.
        if capacity == pool.capacity() {
            let mut idle = lock(&pool.idle);
            if idle.len() < BUFFERS {
                idle.push(buffer);
                return;
            }
        }
        pool.release(capacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock;

    /// Growing the pool gives back the idle buffers of the size it leaves.
    ///
    /// Three idle buffers are the most the pool keeps, one each for a frame
    /// being encoded, one queued and one being sent. A checkout discards an idle
    /// buffer the size it has outgrown, but only the one at the top of the stack:
    /// once a new-size buffer is back, serial checkouts cycle it and never reach
    /// the smaller ones underneath. Growing is where they are all in hand, so it
    /// is where they go, and the memory is counted here rather than the buffers.
    #[test]
    fn growing_the_pool_gives_back_the_buffers_it_outgrew() {
        const OLD: usize = 512 * 1024;
        let pool = Arc::new(Pool::new(4 * OLD));
        assert_eq!(pool.capacity(), OLD);
        // What the pool holds while it is idle: the bytes of every buffer it
        // could hand out, which is the number a stream's memory is about.
        let idle_bytes = || -> usize { lock(&pool.idle).iter().map(Vec::capacity).sum() };
        assert_eq!(idle_bytes(), 0, "nothing allocated yet");

        let held: Vec<Encoded> = (0..BUFFERS).map(|_| pool.checkout()).collect();
        drop(held);
        assert_eq!(idle_bytes(), BUFFERS * OLD, "three idle buffers");

        assert!(pool.grow());
        assert_eq!(pool.capacity(), 2 * OLD);
        // The three buffers of the size it left are gone rather than stranded
        // under the new top of the stack.
        assert_eq!(idle_bytes(), 0, "idle buffers of the size the pool outgrew");

        // One buffer checked out and returned, frame after frame: the pool hands
        // the same one back, and what it holds between frames is that one buffer
        // at the size it hands out now.
        let mut live = 0;
        for frame in 0..10 {
            let mut buffer = pool.checkout();
            live = buffer.spare().len();
            buffer.finish(4);
            assert_eq!(live, 2 * OLD, "frame {frame} was encoded into {live} bytes");
            drop(buffer);
            assert_eq!(
                idle_bytes(),
                2 * OLD,
                "frame {frame} left the idle pool holding {} bytes",
                idle_bytes()
            );
        }
        // Two buffers allocated in all: the three of the old size, freed by the
        // growth, and the one the frames after it go through.
        assert_eq!(pool.created(), BUFFERS + 1);
        assert!(live > OLD, "the pool grew rather than kept the old size");
    }
}
