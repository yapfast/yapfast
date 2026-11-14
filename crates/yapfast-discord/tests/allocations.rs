//! Counts what sending a stream's frames asks the allocator for, from outside
//! the crate: this test binary installs a global allocator of its own, which a
//! test binary may do without moving anyone else's numbers.
//!
//! Two commits claimed savings on the way a frame goes out and neither said
//! how much. `5d8b902` claims that packetizing a frame asks for nothing once
//! the plan is the last frame's, and `3ffea45` that three buffers carry thirty
//! encoded frames with three in flight, which is `stream_connection::Pool`.
//! Both are measured here against the recordings in testing/fixtures, and the
//! numbers are in the assertions' messages, so a regression fails with what
//! changed rather than as a mystery.
//!
//! The encoder is the gap. Encoding a frame into the pool's buffer is
//! `yapfast-core`'s `Encoder::encode_i420`, and that crate depends on this one,
//! so an integration test here can't build an encoder at all. What the pool
//! measures is the pool, which is the whole of what `3ffea45` changed about
//! memory: the encoder used to allocate each frame's buffer itself and now
//! asks the pool for one, and what the pool costs on its own is what's left.

use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;

use yapfast_discord::rtp::{Codec, Packetizer, Plan};
use yapfast_discord::stream_connection::{Encoded, Pool};

struct Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        unsafe { System.alloc(layout) }
    }
    // The pool's buffers come from `vec![0; _]`, which asks for memory that is
    // already zeroed instead of asking and then clearing it. A counter that
    // passed `alloc_zeroed` through would count the pool as free.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size() as u64, Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(size as u64, Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// What a span of code asked for: how many times, and how many bytes. A
/// reallocation counts as the size it asks for, which is the memory it ends up
/// holding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counts {
    allocations: u64,
    bytes: u64,
}

impl fmt::Display for Counts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} allocations of {} bytes",
            self.allocations, self.bytes
        )
    }
}

/// Allocations the process asked for while `f` ran.
fn counted<T>(f: impl FnOnce() -> T) -> (T, Counts) {
    let (allocations, bytes) = (ALLOCATIONS.load(Relaxed), BYTES.load(Relaxed));
    let value = f();
    (
        value,
        Counts {
            allocations: ALLOCATIONS.load(Relaxed) - allocations,
            bytes: BYTES.load(Relaxed) - bytes,
        },
    )
}

/// A recording from testing/fixtures as the frames a stream sends: an access
/// unit for each of H264's and H265's, an IVF frame for VP8, and a temporal
/// unit for AV1. Each is a frame a real encoder could have made, which is what
/// the packetizer's cost depends on.
fn recording(codec: Codec) -> Vec<Vec<u8>> {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testing/fixtures");
    let file = std::fs::read(fixtures.join(match codec {
        Codec::Vp8 => "stream-vp8.ivf",
        Codec::H264 => "stream-h264.h264",
        Codec::H265 => "stream-h265.h265",
        Codec::Av1 => "stream-av1.obu",
    }))
    .expect("a recording under testing/fixtures");
    match codec {
        // IVF: a header saying where the frames start, then each frame after a
        // 12-byte header that gives its length.
        Codec::Vp8 => {
            let mut at = usize::from(u16::from_le_bytes([file[6], file[7]]));
            let mut frames = vec![];
            while at + 12 <= file.len() {
                let size = u32::from_le_bytes(file[at..at + 4].try_into().unwrap()) as usize;
                frames.push(file[at + 12..at + 12 + size].to_vec());
                at += 12 + size;
            }
            frames
        }
        // Annex B: a new access unit at each delimiter, which is not sent, and
        // each kept unit behind the four-byte start code Yapfast sends.
        Codec::H264 | Codec::H265 => {
            let delimiter = |unit: &[u8]| match codec {
                Codec::H264 => unit[0] & 0x1f == 9,
                _ => unit[0] >> 1 & 0x3f == 35,
            };
            let starts: Vec<usize> = (0..file.len() - 2)
                .filter(|&at| file[at..at + 3] == [0, 0, 1])
                .map(|at| at + 3)
                .collect();
            let mut frames: Vec<Vec<u8>> = vec![];
            for (index, &start) in starts.iter().enumerate() {
                let mut end = starts.get(index + 1).map_or(file.len(), |next| next - 3);
                if file[end - 1] == 0 {
                    end -= 1;
                }
                let unit = &file[start..end];
                if delimiter(unit) || frames.is_empty() {
                    frames.push(vec![]);
                }
                if !delimiter(unit) {
                    frames.last_mut().unwrap().extend_from_slice(&[0, 0, 0, 1]);
                    frames.last_mut().unwrap().extend_from_slice(unit);
                }
            }
            frames
        }
        // AV1's OBUs cut into temporal units at their delimiters.
        Codec::Av1 => {
            let mut frames: Vec<Vec<u8>> = vec![];
            let mut at = 0;
            while at < file.len() {
                // An OBU gives its size in one LEB128 byte or two.
                let (size, used) = match file[at + 1] {
                    low if low & 0x80 == 0 => (usize::from(low), 1),
                    low => (usize::from(low & 0x7f) | usize::from(file[at + 2]) << 7, 2),
                };
                let end = at + 1 + used + size;
                if file[at] >> 3 & 0x0f == 2 {
                    frames.push(vec![]);
                }
                frames.last_mut().unwrap().extend_from_slice(&file[at..end]);
                at = end;
            }
            frames
        }
    }
}

/// Frames packetized in the steady state: three seconds of 30 fps video, long
/// enough that anything a frame costs shows up as a number rather than hiding
/// in one that happens to come out the same.
const FRAMES: usize = 100;

/// Sends `count` frames from `from` on, one after another as
/// `stream_connection`'s sender does: into the plan the last frame used, and
/// the datagram buffer the last packet was written to. Says how many packets
/// went out.
fn send(
    packetizer: &mut Packetizer,
    plan: &mut Plan,
    out: &mut Vec<u8>,
    frames: &[Vec<u8>],
    from: usize,
    count: usize,
) -> usize {
    let mut packets = 0;
    for index in from..from + count {
        let frame = &frames[index % frames.len()];
        // A recording opens with a keyframe, so each pass over it does too.
        let keyframe = index % frames.len() == 0;
        packetizer.plan_into(plan, frame, index as u32 * 3000, keyframe);
        while packetizer.write(plan, frame, out).is_some() {
            packets += 1;
        }
    }
    black_box(out);
    packets
}

/// One recording's cost, as measured here and then asserted: what its first
/// frame asks for, when the plan has nothing to lend it and neither has the
/// datagram buffer.
struct Expected {
    codec: Codec,
    /// Allocations the recording's first frame asks for. The plan grows its
    /// vectors of packets, chunks and written bytes as it describes the frame,
    /// and the buffer the first packet is written into asks for room for a
    /// full datagram; none of it is asked for again. These are a 64-bit
    /// target's counts: the plan's vectors hold words, and a word is eight
    /// bytes.
    first: u64,
}

/// The four recordings, each 30 frames of what an encoder puts out.
const EXPECTED: [Expected; 4] = [
    Expected {
        codec: Codec::H264,
        first: 11,
    },
    Expected {
        codec: Codec::H265,
        first: 13,
    },
    Expected {
        codec: Codec::Vp8,
        first: 9,
    },
    Expected {
        codec: Codec::Av1,
        first: 7,
    },
];

/// Packetizer and pool in one test, not two: the counter counts this whole
/// process, and `cargo test` runs a binary's tests in threads of their own
/// beside the harness's own, whose allocations land in whoever is measuring at
/// the time. One test is one thread with nothing else running.
#[test]
fn sending_a_frame_asks_for_nothing_once_the_plan_and_buffers_are_warm() {
    packetizing();
    encoding();
}

fn packetizing() {
    for expected in EXPECTED {
        let frames = recording(expected.codec);
        let mut packetizer = Packetizer::new(1, expected.codec);
        let mut plan = Plan::default();
        let mut out = Vec::new();

        let (sent, first) = counted(|| send(&mut packetizer, &mut plan, &mut out, &frames, 0, 1));

        // One pass over the whole recording first, which grows the plan to the
        // largest frame it has. A frame bigger than every frame before it costs
        // a growth step once, and that is not the per-frame cost a regression
        // would show as: what a steady stream pays is what's measured next.
        send(
            &mut packetizer,
            &mut plan,
            &mut out,
            &frames,
            1,
            frames.len() - 1,
        );

        let (warm_packets, warm) =
            counted(|| send(&mut packetizer, &mut plan, &mut out, &frames, 1, FRAMES));

        // A frame nothing goes out for would make zero allocations mean
        // nothing, and no recording has one.
        assert!(
            warm_packets >= FRAMES,
            "{}: {warm_packets} packets in {FRAMES} frames",
            expected.codec.name()
        );
        assert_eq!(
            warm,
            Counts::default(),
            "{} asked for {} over {FRAMES} frames once the plan had settled, {:.1} a frame",
            expected.codec.name(),
            warm,
            warm.allocations as f64 / FRAMES as f64
        );
        assert_eq!(
            first.allocations,
            expected.first,
            "{}: the first frame asked for {}, the recording's frame of {} bytes",
            expected.codec.name(),
            first,
            frames[0].len()
        );

        // `packets` is the wrapper kept for the tests and the odd packet: a
        // plan of its own each call, and a vector of its own for every packet.
        // It costs the first frame's growth plus the vector the packets go in
        // and one buffer each, which is the `2N + 3` `5d8b902` replaced with
        // the plan.
        let (wrapped_packets, wrapped) = counted(|| packetizer.packets(&frames[0], 0, true));
        assert_eq!(
            wrapped_packets.len(),
            sent,
            "{}: packets() gave {} where the plan gave {sent}",
            expected.codec.name(),
            wrapped_packets.len()
        );
        assert_eq!(
            wrapped.allocations,
            first.allocations + 1 + sent as u64,
            "{}: a frame through packets() asked for {} for {sent} packets, a \
             frame through the plan for {}",
            expected.codec.name(),
            wrapped,
            first
        );
    }
}

/// One frame through the pool: the frame sent last is dropped, which returns
/// its buffer, and the next frame is encoded into the buffer the pool hands
/// out for it.
fn encode_into(pool: &Arc<Pool>, in_flight: &mut [Option<Encoded>], at: usize) {
    let slot = at % in_flight.len();
    in_flight[slot] = None;
    let mut encoded = pool.checkout();
    encoded.spare()[..4].copy_from_slice(&[0, 0, 0, 1]);
    encoded.finish(4);
    in_flight[slot] = Some(encoded);
}

fn encoding() {
    // A 1080p stream's frames at two bytes a pixel, which is the size
    // `stream_connection` gives a pool for one.
    const MAX: usize = 1920 * 1080 * 2;
    // As long as a recording, and longer than the queue behind the sender.
    const SENT: usize = 30;

    let pool = Arc::new(Pool::new(MAX));
    let capacity = pool.capacity();

    // Before the pool, a frame was encoded into a buffer it allocated itself,
    // which the caller then copied out of: thirty frames asked for thirty
    // buffers.
    let (_, each) = counted(|| {
        for _ in 0..SENT {
            let mut encoded = Encoded::from(vec![0; capacity]);
            encoded.spare()[..4].copy_from_slice(&[0, 0, 0, 1]);
            encoded.finish(4);
            black_box(&encoded);
        }
    });

    // Three in flight, as the sender keeps: one being encoded into, one queued
    // to send and one being sent.
    let mut in_flight: [Option<Encoded>; 3] = [None, None, None];
    let (_, cold) = counted(|| {
        for frame in 0..SENT {
            encode_into(&pool, &mut in_flight, frame);
        }
    });
    let (_, warm) = counted(|| {
        for frame in SENT..2 * SENT {
            encode_into(&pool, &mut in_flight, frame);
        }
    });

    assert_eq!(
        cold.allocations,
        pool.created() as u64 + 1,
        "the pool's first {SENT} frames asked for {}: {} buffers of {capacity} bytes and the \
         list they go back into, against {} for a buffer each",
        cold,
        pool.created(),
        each
    );
    assert_eq!(
        pool.created(),
        3,
        "three frames in flight over {SENT} made {} buffers, which is {:.0} a frame",
        pool.created(),
        pool.created() as f64 / SENT as f64
    );
    assert_eq!(
        warm.allocations,
        0,
        "the pool asked for {} over {SENT} more frames with three in flight, {:.1} a frame",
        warm,
        warm.allocations as f64 / SENT as f64
    );
    assert_eq!(
        each,
        Counts {
            allocations: SENT as u64,
            bytes: (SENT * capacity) as u64,
        },
        "a buffer of its own each frame would have asked for {}",
        each
    );
}
