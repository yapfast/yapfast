//! The queue between a Core Audio callback and the caller's blocking
//! `write` or `read`: one thread produces bytes, the other consumes them,
//! and neither waits for the other. A callback runs on the system's
//! real-time thread, so nothing here allocates or locks.

use std::{
    sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
    time::Duration,
};

use rtrb::{Consumer, Producer, RingBuffer};

/// How long a blocked caller sleeps between looks at the queue. A real-time
/// callback can't wake it without a system call, so it polls: at 2 ms that
/// is 500 cheap wakes a second, against a 10 ms microphone block or a
/// 20 ms voice frame.
pub const POLL: Duration = Duration::from_millis(2);

/// The most bytes one queue holds, a sanity bound on what a caller asks for.
const MAX_BYTES: usize = 1 << 26;

/// The two ends of a byte queue holding `capacity` bytes, at least one.
pub fn queue(capacity: usize) -> (Producer<u8>, Consumer<u8>) {
    RingBuffer::new(capacity.clamp(1, MAX_BYTES))
}

/// Appends as much of `bytes` as fits, and says how much that was.
pub fn push(producer: &mut Producer<u8>, bytes: &[u8]) -> usize {
    let count = bytes.len().min(producer.slots());
    let Ok(mut chunk) = producer.write_chunk(count) else {
        return 0;
    };
    let (first, second) = chunk.as_mut_slices();
    let (head, tail) = bytes[..count].split_at(first.len());
    first.copy_from_slice(head);
    second.copy_from_slice(tail);
    chunk.commit_all();
    count
}

/// Moves up to `out.len()` queued bytes into `out`, and says how many.
pub fn pop(consumer: &mut Consumer<u8>, out: &mut [u8]) -> usize {
    let count = out.len().min(consumer.slots());
    let Ok(chunk) = consumer.read_chunk(count) else {
        return 0;
    };
    let (first, second) = chunk.as_slices();
    let (head, tail) = out[..count].split_at_mut(first.len());
    head.copy_from_slice(first);
    tail.copy_from_slice(second);
    chunk.commit_all();
    count
}

/// Drops everything queued.
pub fn discard(consumer: &mut Consumer<u8>) {
    let count = consumer.slots();
    if let Ok(chunk) = consumer.read_chunk(count) {
        chunk.commit_all();
    }
}

/// What the callback and the caller's thread tell each other besides bytes.
#[derive(Default)]
pub struct Signals {
    /// Counts callbacks: a count that stands still means the device is gone.
    pub beats: AtomicU64,
    /// The caller asks the speakers' callback to drop what's queued; the
    /// callback clears it when done.
    pub flush: AtomicBool,
    /// Callbacks that found the speakers' queue empty, or a microphone's
    /// queue full: the caller logs them, a callback may not.
    pub missed: AtomicU64,
    /// The last status a render failed with, or 0 while none has.
    pub failure: AtomicI32,
}

impl Signals {
    pub fn beat(&self) {
        self.beats.fetch_add(1, Ordering::Relaxed);
    }

    pub fn miss(&self) {
        self.missed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn fail(&self, status: i32) {
        self.failure.store(status, Ordering::Relaxed);
        self.miss();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_goes_in_comes_out_in_order() {
        let (mut producer, mut consumer) = queue(8);
        assert_eq!(push(&mut producer, &[1, 2, 3]), 3);
        let mut out = [0; 8];
        assert_eq!(pop(&mut consumer, &mut out), 3);
        assert_eq!(&out[..3], &[1, 2, 3]);
    }

    #[test]
    fn a_full_queue_takes_only_what_fits() {
        let (mut producer, mut consumer) = queue(4);
        assert_eq!(push(&mut producer, &[1, 2, 3, 4, 5, 6]), 4);
        assert_eq!(push(&mut producer, &[7]), 0);
        let mut out = [0; 2];
        assert_eq!(pop(&mut consumer, &mut out), 2);
        assert_eq!(out, [1, 2]);
        assert_eq!(push(&mut producer, &[7, 8, 9]), 2);
    }

    #[test]
    fn bytes_survive_wrapping_around_the_end() {
        let (mut producer, mut consumer) = queue(5);
        let mut out = [0; 5];
        assert_eq!(push(&mut producer, &[1, 2, 3, 4]), 4);
        assert_eq!(pop(&mut consumer, &mut out[..3]), 3);
        assert_eq!(push(&mut producer, &[5, 6, 7, 8]), 4);
        assert_eq!(pop(&mut consumer, &mut out), 5);
        assert_eq!(out, [4, 5, 6, 7, 8]);
    }

    #[test]
    fn an_empty_queue_gives_nothing() {
        let (_producer, mut consumer) = queue(4);
        let mut out = [9; 4];
        assert_eq!(pop(&mut consumer, &mut out), 0);
        assert_eq!(out, [9; 4]);
    }

    #[test]
    fn discarding_empties_the_queue() {
        let (mut producer, mut consumer) = queue(4);
        push(&mut producer, &[1, 2, 3]);
        discard(&mut consumer);
        assert_eq!(producer.slots(), 4);
        assert_eq!(pop(&mut consumer, &mut [0; 4]), 0);
    }

    #[test]
    fn a_producer_and_a_consumer_thread_lose_nothing() {
        const TOTAL: usize = 100_000;
        let (mut producer, mut consumer) = queue(64);
        let writer = std::thread::spawn(move || {
            let mut sent = 0usize;
            while sent < TOTAL {
                let block: Vec<u8> = (sent..(sent + 7).min(TOTAL)).map(|n| n as u8).collect();
                sent += push(&mut producer, &block);
            }
        });
        let mut received = 0usize;
        let mut out = [0u8; 5];
        while received < TOTAL {
            let count = pop(&mut consumer, &mut out);
            for byte in &out[..count] {
                assert_eq!(*byte, received as u8);
                received += 1;
            }
        }
        writer.join().unwrap();
    }
}
