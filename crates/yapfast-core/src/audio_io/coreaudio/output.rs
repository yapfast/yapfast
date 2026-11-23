//! Speakers: an AUHAL unit whose render callback takes bytes from a lock-free
//! queue, which [`Output::write`] fills from the caller's thread.

use std::{
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
    sync::{Arc, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Result, ensure};
use objc2_audio_toolbox::AudioUnitRenderActionFlags;
use objc2_core_audio_types::{AudioBuffer, AudioBufferList, AudioTimeStamp};
use rtrb::{Consumer, Producer};

use super::{
    super::Format,
    devices::{self, OsStatus},
    ring::{self, POLL, Signals},
    unit::{Context, Direction, Unit},
    watch::Watchdog,
};

/// How much a speakers' queue holds when the caller names none: enough for
/// a sound written in one go to start at once without waiting on every
/// callback.
const DEFAULT_QUEUE: Duration = Duration::from_millis(500);

/// The least a queue holds. A callback asks for a device buffer, about 10 ms
/// to 20 ms, and a smaller queue would run dry between two of them.
const MIN_QUEUE: Duration = Duration::from_millis(40);

/// How long [`Output::flush`] waits for the callback to drop the queue.
const FLUSH_WITHIN: Duration = Duration::from_millis(100);

/// What a callback answers for a buffer it can't fill: `kAudio_ParamError`.
const BAD_BUFFER: OsStatus = -50;

/// What a callback answers after a panic, which it never lets out.
const PANICKED: OsStatus = -1;

/// The bytes a queue holds for `queue`, whole frames.
fn queue_bytes(format: Format, queue: Option<Duration>) -> usize {
    format.bytes(queue.unwrap_or(DEFAULT_QUEUE).max(MIN_QUEUE))
}

/// How long `bytes` of `format` take to play.
fn bytes_to_duration(bytes: usize, format: Format) -> Duration {
    let frames = bytes / format.frame_bytes().max(1);
    Duration::from_secs_f64(frames as f64 / f64::from(format.rate.max(1)))
}

/// What the render callback owns.
struct Render {
    consumer: Consumer<u8>,
    signals: Arc<Signals>,
}

pub struct Output {
    // The unit goes first: it stops its callbacks before the context frees.
    _unit: Unit,
    _context: Context<Render>,
    producer: Producer<u8>,
    signals: Arc<Signals>,
    format: Format,
    capacity: usize,
    watchdog: Watchdog,
    device_latency: Duration,
    reported_misses: u64,
    name: &'static str,
}

impl Output {
    pub fn open(
        speaker: Option<&str>,
        name: &'static str,
        format: Format,
        queue: Option<Duration>,
    ) -> Result<Self> {
        ensure!(
            format.channels > 0 && format.rate > 0,
            "a stream needs a rate and channels"
        );
        let device = devices::resolve(speaker, true)?;
        let capacity = queue_bytes(format, queue);
        let (producer, consumer) = ring::queue(capacity);
        let queue_capacity = producer.buffer().capacity();
        let signals = Arc::new(Signals::default());
        let context = Context::new(Render {
            consumer,
            signals: Arc::clone(&signals),
        });
        let mut unit = Unit::open(
            Direction::Playback,
            device.id,
            format,
            Some(render),
            context.as_ptr(),
        )?;
        unit.start()?;
        let device_latency = devices::latency(device.id, true);
        tracing::info!(
            stream = name,
            device = device.name,
            rate = format.rate,
            channels = format.channels,
            latency_ms = device_latency.as_millis() as u64,
            "speakers opened through Core Audio"
        );
        Ok(Self {
            _unit: unit,
            _context: context,
            producer,
            signals,
            format,
            capacity: queue_capacity,
            watchdog: Watchdog::start(Instant::now()),
            device_latency,
            reported_misses: 0,
            name,
        })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let whole = bytes.len() - bytes.len() % self.format.frame_bytes();
        if whole < bytes.len() {
            tracing::warn!(stream = self.name, "dropped a part of a frame");
        }
        let mut rest = &bytes[..whole];
        while !rest.is_empty() {
            let written = ring::push(&mut self.producer, rest);
            rest = &rest[written..];
            if written == 0 {
                self.wait()?;
            }
        }
        self.report_misses();
        Ok(())
    }

    pub fn drain(&mut self) -> Result<()> {
        while self.producer.slots() < self.capacity {
            self.wait()?;
        }
        // What the device itself still holds.
        thread::sleep(self.device_latency);
        Ok(())
    }

    pub fn flush(&mut self) {
        self.signals.flush.store(true, Ordering::Release);
        let until = Instant::now() + FLUSH_WITHIN;
        while self.signals.flush.load(Ordering::Acquire) && Instant::now() < until {
            thread::sleep(Duration::from_millis(1));
        }
    }

    pub fn latency(&self) -> Duration {
        let queued = self.capacity - self.producer.slots();
        bytes_to_duration(queued, self.format) + self.device_latency
    }

    /// Sleeps a moment, or says why waiting is no use: the device is gone.
    fn wait(&mut self) -> Result<()> {
        let beats = self.signals.beats.load(Ordering::Relaxed);
        let failure = self.signals.failure.load(Ordering::Relaxed);
        self.watchdog.check(beats, failure, Instant::now())?;
        thread::sleep(POLL);
        Ok(())
    }

    fn report_misses(&mut self) {
        let total = self.signals.missed.load(Ordering::Relaxed);
        if total > self.reported_misses {
            tracing::debug!(
                stream = self.name,
                count = total - self.reported_misses,
                "speakers ran out of samples"
            );
            self.reported_misses = total;
        }
    }
}

/// The unit's callback: plays what's queued, silence when nothing is. It
/// runs on a real-time thread and never allocates, locks or panics.
///
/// # Safety
///
/// `context` is the [`Render`] this unit was made with, and `data` is the
/// buffer list the unit gives, both live for the call.
unsafe extern "C-unwind" fn render(
    context: NonNull<c_void>,
    _flags: NonNull<AudioUnitRenderActionFlags>,
    _stamp: NonNull<AudioTimeStamp>,
    _bus: u32,
    _frames: u32,
    data: *mut AudioBufferList,
) -> OsStatus {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: as the function's contract says; only this callback uses
        // the context while the unit runs.
        let render = unsafe { context.cast::<Render>().as_mut() };
        // SAFETY: a non-null list is live for the call.
        match unsafe { data.as_mut() } {
            Some(list) if list.mNumberBuffers == 1 => {
                play(render, &mut list.mBuffers[0]);
                0
            }
            _ => BAD_BUFFER,
        }
    }))
    .unwrap_or(PANICKED)
}

fn play(render: &mut Render, buffer: &mut AudioBuffer) {
    render.signals.beat();
    if render.signals.flush.load(Ordering::Acquire) {
        ring::discard(&mut render.consumer);
        render.signals.flush.store(false, Ordering::Release);
    }
    if buffer.mData.is_null() {
        return;
    }
    // SAFETY: the unit gives a buffer of `mDataByteSize` bytes at `mData`.
    let out = unsafe {
        std::slice::from_raw_parts_mut(buffer.mData.cast::<u8>(), buffer.mDataByteSize as usize)
    };
    if fill(&mut render.consumer, out) {
        render.signals.miss();
    }
}

/// Fills `out` from the queue, with silence for what it lacks. Says whether
/// the queue ran short.
fn fill(consumer: &mut Consumer<u8>, out: &mut [u8]) -> bool {
    let got = ring::pop(consumer, out);
    out[got..].fill(0);
    got < out.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEREO: Format = Format {
        rate: 48_000,
        channels: 2,
        float: false,
    };

    #[test]
    fn a_queue_holds_what_was_asked_in_whole_frames() {
        assert_eq!(
            queue_bytes(STEREO, Some(Duration::from_millis(100))),
            4_800 * 4
        );
        assert_eq!(queue_bytes(STEREO, None), 24_000 * 4);
    }

    #[test]
    fn a_tiny_queue_is_raised_to_the_least() {
        assert_eq!(
            queue_bytes(STEREO, Some(Duration::from_millis(1))),
            1_920 * 4
        );
    }

    #[test]
    fn bytes_play_for_their_frames() {
        assert_eq!(
            bytes_to_duration(48_000 * 4, STEREO),
            Duration::from_secs(1)
        );
        assert_eq!(bytes_to_duration(0, STEREO), Duration::ZERO);
    }

    #[test]
    fn a_short_queue_is_played_then_silence() {
        let (mut producer, mut consumer) = ring::queue(16);
        ring::push(&mut producer, &[1, 2, 3]);
        let mut out = [9; 6];
        assert!(fill(&mut consumer, &mut out));
        assert_eq!(out, [1, 2, 3, 0, 0, 0]);
    }

    #[test]
    fn a_full_queue_fills_the_buffer_without_a_miss() {
        let (mut producer, mut consumer) = ring::queue(16);
        ring::push(&mut producer, &[1, 2, 3, 4, 5]);
        let mut out = [0; 4];
        assert!(!fill(&mut consumer, &mut out));
        assert_eq!(out, [1, 2, 3, 4]);
    }
}
