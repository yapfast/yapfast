//! A microphone: an AUHAL unit whose input callback renders what the device
//! heard into a lock-free queue, which [`Input::read`] empties on the
//! caller's thread.

use std::{
    collections::VecDeque,
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicPtr, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Result, ensure};
use objc2_audio_toolbox::{AudioUnitRender, AudioUnitRenderActionFlags};
use objc2_core_audio_types::{AudioBuffer, AudioBufferList, AudioTimeStamp};
use rtrb::{Consumer, Producer};

use super::{
    super::Format,
    devices::{self, OsStatus},
    resample::{self, Converter},
    ring::{self, POLL, Signals},
    unit::{Context, Direction, MAX_FRAMES, Unit},
    watch::{SilenceWatch, Watchdog},
};

/// How much a microphone's queue holds before the callback has to drop
/// what it hears.
const QUEUE: Duration = Duration::from_millis(500);

/// How much of the microphone's past a reader that fell behind keeps. What
/// is older is dropped, so what it reads is what's being said now and not
/// what was said while it was busy.
const BACKLOG_LIMIT: Duration = Duration::from_millis(300);

/// What a callback answers after a panic, which it never lets out.
const PANICKED: OsStatus = -1;

/// What the input callback owns.
struct Capture {
    /// The unit to render from, stored once it exists, before it starts.
    unit: AtomicPtr<c_void>,
    producer: Producer<u8>,
    signals: Arc<Signals>,
    /// Where one callback's frames are rendered, before they're queued.
    scratch: Box<[u8]>,
    channels: u32,
    frame_bytes: usize,
}

/// Buffers for [`Input::read_converted`], kept to allocate once.
#[derive(Default)]
struct Converting {
    raw: Vec<u8>,
    floats: Vec<f32>,
    converted: Vec<f32>,
    ready: VecDeque<u8>,
}

pub struct Input {
    // The unit goes first: it stops its callbacks before the context frees.
    _unit: Unit,
    _context: Context<Capture>,
    consumer: Consumer<u8>,
    signals: Arc<Signals>,
    /// What the caller asked for.
    format: Format,
    /// What the unit delivers: `format`, or the device's own rate in floats
    /// when `converter` takes it to `format`.
    unit_format: Format,
    converter: Option<Converter>,
    converting: Converting,
    watchdog: Watchdog,
    silence: SilenceWatch,
    reported_misses: u64,
    name: &'static str,
}

impl Input {
    pub fn open(
        microphone: Option<&str>,
        name: &'static str,
        format: Format,
        fragment: Duration,
    ) -> Result<Self> {
        ensure!(
            format.channels > 0 && format.rate > 0,
            "a stream needs a rate and channels"
        );
        let device = devices::resolve(microphone, false)?;
        let (unit_format, converter) = plan(format, devices::nominal_rate(device.id))?;
        let (producer, consumer) = ring::queue(unit_format.bytes(QUEUE));
        let signals = Arc::new(Signals::default());
        let context = Context::new(Capture {
            unit: AtomicPtr::new(std::ptr::null_mut()),
            producer,
            signals: Arc::clone(&signals),
            scratch: vec![0; MAX_FRAMES as usize * unit_format.frame_bytes()].into_boxed_slice(),
            channels: u32::from(unit_format.channels),
            frame_bytes: unit_format.frame_bytes(),
        });
        let mut unit = Unit::open(
            Direction::Capture,
            device.id,
            unit_format,
            Some(capture),
            context.as_ptr(),
        )?;
        context
            .shared()
            .unit
            .store(unit.raw().cast(), Ordering::Release);
        unit.start()?;
        tracing::info!(
            stream = name,
            device = device.name,
            rate = format.rate,
            device_rate = unit_format.rate,
            channels = format.channels,
            fragment_ms = fragment.as_millis() as u64,
            "microphone opened through Core Audio; macOS asks for permission on first use"
        );
        Ok(Self {
            _unit: unit,
            _context: context,
            consumer,
            signals,
            format,
            unit_format,
            converter,
            converting: Converting::default(),
            watchdog: Watchdog::start(Instant::now()),
            silence: SilenceWatch::new(format),
            reported_misses: 0,
            name,
        })
    }

    pub fn read(&mut self, bytes: &mut [u8]) -> Result<()> {
        self.drop_old_audio();
        if self.converter.is_some() {
            self.read_converted(bytes)?;
        } else {
            self.read_raw(bytes)?;
        }
        self.report_misses();
        if self.silence.observe(bytes) {
            tracing::error!(
                stream = self.name,
                "the microphone delivers only silence: macOS may be refusing it, see System Settings, Privacy & Security, Microphone"
            );
        }
        Ok(())
    }

    /// Fills `bytes` with what the unit delivered, waiting for it.
    fn read_raw(&mut self, bytes: &mut [u8]) -> Result<()> {
        let mut filled = 0;
        while filled < bytes.len() {
            let read = ring::pop(&mut self.consumer, &mut bytes[filled..]);
            filled += read;
            if read == 0 {
                self.wait()?;
            }
        }
        Ok(())
    }

    /// Fills `bytes` from the unit's own rate, converted to the caller's.
    fn read_converted(&mut self, bytes: &mut [u8]) -> Result<()> {
        let Some(mut converter) = self.converter.take() else {
            return self.read_raw(bytes);
        };
        let result = self.convert_until(&mut converter, bytes.len());
        self.converter = Some(converter);
        result?;
        for byte in bytes {
            *byte = self.converting.ready.pop_front().unwrap_or(0);
        }
        Ok(())
    }

    /// Converts chunks until `wanted` bytes of the caller's format are ready.
    fn convert_until(&mut self, converter: &mut Converter, wanted: usize) -> Result<()> {
        let chunk_bytes = converter.chunk_frames() * self.unit_format.frame_bytes();
        let mut raw = std::mem::take(&mut self.converting.raw);
        raw.resize(chunk_bytes, 0);
        let mut result = Ok(());
        while self.converting.ready.len() < wanted {
            result = self.read_raw(&mut raw).and_then(|()| {
                resample::floats_from_bytes(&raw, &mut self.converting.floats);
                self.converting.converted.clear();
                converter.convert(&self.converting.floats, &mut self.converting.converted)?;
                resample::push_samples(
                    &mut self.converting.ready,
                    &self.converting.converted,
                    self.format,
                );
                Ok(())
            });
            if result.is_err() {
                break;
            }
        }
        self.converting.raw = raw;
        result
    }

    /// Drops what the queue holds beyond [`BACKLOG_LIMIT`], oldest first.
    fn drop_old_audio(&mut self) {
        let extra = excess(
            self.consumer.slots(),
            self.unit_format.bytes(BACKLOG_LIMIT),
            self.unit_format.frame_bytes(),
        );
        if extra == 0 {
            return;
        }
        if let Ok(chunk) = self.consumer.read_chunk(extra) {
            chunk.commit_all();
        }
        tracing::debug!(
            stream = self.name,
            bytes = extra,
            "microphone audio dropped, the reader was late"
        );
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
                "microphone audio was lost to a full queue"
            );
            self.reported_misses = total;
        }
    }
}

/// What the unit should deliver, and the conversion that takes it to what
/// the caller asked for: none when the device runs at that rate already or
/// won't say what it runs at.
fn plan(wanted: Format, device_rate: f64) -> Result<(Format, Option<Converter>)> {
    let device_rate = device_rate.round() as u32;
    if device_rate == 0 || device_rate == wanted.rate {
        return Ok((wanted, None));
    }
    let delivered = Format {
        rate: device_rate,
        channels: wanted.channels,
        float: true,
    };
    let converter = Converter::new(device_rate, wanted.rate, wanted.channels)?;
    Ok((delivered, Some(converter)))
}

/// How many bytes of `queued` to drop to keep `limit`, in whole frames.
fn excess(queued: usize, limit: usize, frame_bytes: usize) -> usize {
    let extra = queued.saturating_sub(limit);
    extra - extra % frame_bytes.max(1)
}

/// The unit's callback: renders the frames the device heard and queues
/// them. It runs on a real-time thread and never allocates, locks or panics.
///
/// # Safety
///
/// `context` is the [`Capture`] this unit was made with, and the other
/// arguments are the unit's own, live for the call.
unsafe extern "C-unwind" fn capture(
    context: NonNull<c_void>,
    flags: NonNull<AudioUnitRenderActionFlags>,
    stamp: NonNull<AudioTimeStamp>,
    bus: u32,
    frames: u32,
    _data: *mut AudioBufferList,
) -> OsStatus {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: as the function's contract says; only this callback uses
        // the context while the unit runs.
        let capture = unsafe { context.cast::<Capture>().as_mut() };
        record(capture, flags, stamp, bus, frames)
    }))
    .unwrap_or(PANICKED)
}

fn record(
    capture: &mut Capture,
    flags: NonNull<AudioUnitRenderActionFlags>,
    stamp: NonNull<AudioTimeStamp>,
    bus: u32,
    frames: u32,
) -> OsStatus {
    let wanted = frames as usize * capture.frame_bytes;
    let unit = capture.unit.load(Ordering::Acquire);
    if wanted > capture.scratch.len() || unit.is_null() {
        capture.signals.miss();
        return 0;
    }
    let mut list = AudioBufferList {
        mNumberBuffers: 1,
        mBuffers: [AudioBuffer {
            mNumberChannels: capture.channels,
            mDataByteSize: wanted as u32,
            mData: capture.scratch.as_mut_ptr().cast(),
        }],
    };
    // SAFETY: the unit is the live one this callback belongs to, the list
    // points into `scratch`, which holds `wanted` bytes, and the stamp and
    // flags are the callback's own.
    let status = unsafe {
        AudioUnitRender(
            unit.cast(),
            flags.as_ptr(),
            stamp,
            bus,
            frames,
            NonNull::from(&mut list),
        )
    };
    if status != 0 {
        capture.signals.fail(status);
        return status;
    }
    capture.signals.beat();
    let rendered = (list.mBuffers[0].mDataByteSize as usize).min(wanted);
    deliver(capture, rendered);
    0
}

/// Queues the first `length` bytes of the scratch buffer.
fn deliver(capture: &mut Capture, length: usize) {
    let pushed = ring::push(&mut capture.producer, &capture.scratch[..length]);
    if pushed < length {
        capture.signals.miss();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONO: Format = Format {
        rate: 48_000,
        channels: 1,
        float: false,
    };

    #[test]
    fn a_device_at_the_wanted_rate_needs_no_conversion() {
        let (delivered, converter) = plan(MONO, 48_000.0).unwrap();
        assert_eq!(delivered, MONO);
        assert!(converter.is_none());
        assert!(plan(MONO, 0.0).unwrap().1.is_none());
    }

    #[test]
    fn a_device_at_another_rate_delivers_floats_to_convert() {
        let (delivered, converter) = plan(MONO, 44_100.0).unwrap();
        assert_eq!(
            delivered,
            Format {
                rate: 44_100,
                channels: 1,
                float: true
            }
        );
        assert!(converter.is_some());
    }

    #[test]
    fn a_reader_that_kept_up_loses_nothing() {
        assert_eq!(excess(1_000, 2_000, 2), 0);
        assert_eq!(excess(2_000, 2_000, 2), 0);
    }

    #[test]
    fn a_reader_that_fell_behind_drops_the_oldest_whole_frames() {
        assert_eq!(excess(2_101, 2_000, 4), 100);
        assert_eq!(excess(2_103, 2_000, 4), 100);
    }

    #[test]
    fn delivering_a_block_queues_it_and_a_full_queue_counts_a_miss() {
        let (producer, mut consumer) = ring::queue(4);
        let signals = Arc::new(Signals::default());
        let mut capture = Capture {
            unit: AtomicPtr::new(std::ptr::null_mut()),
            producer,
            signals: Arc::clone(&signals),
            scratch: vec![7; 6].into_boxed_slice(),
            channels: 1,
            frame_bytes: 2,
        };
        deliver(&mut capture, 4);
        assert_eq!(signals.missed.load(Ordering::Relaxed), 0);
        deliver(&mut capture, 2);
        assert_eq!(signals.missed.load(Ordering::Relaxed), 1);
        let mut out = [0; 8];
        assert_eq!(ring::pop(&mut consumer, &mut out), 4);
    }
}
