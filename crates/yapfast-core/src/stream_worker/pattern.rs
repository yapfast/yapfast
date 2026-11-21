//! `stream_worker`'s pattern.

use super::*;

/// When frame `frame` of a loop begun at `started` is due at `fps`.
pub(crate) fn due_at(started: Instant, frame: u64, fps: u32) -> Option<Instant> {
    started.checked_add(Duration::from_secs(frame).checked_div(fps)?)
}

/// The frame a loop begun at `started` makes at `now`, and how long to wait for
/// it first.
///
/// On time that's the frame next in line; behind, it's the latest frame whose
/// deadline has passed, so a stall skips the frames it swallowed instead of
/// making all of them in a burst. Deadlines stay counted from `started`, so
/// elapsed time, and the timestamps made from it, run on through the stall.
pub(crate) fn pace(
    started: Instant,
    frame: u64,
    fps: u32,
    now: Instant,
) -> (u64, Option<Duration>) {
    let Some(due) = due_at(started, frame, fps) else {
        // No cadence to keep: a job's frame rate is clamped to 1–60, and zero
        // would divide by it. Make the frame asked for, at once.
        return (frame, None);
    };
    if let Some(wait) = due.checked_duration_since(now) {
        return (frame, Some(wait));
    }
    // The latest frame whose deadline is at or before `now`, counted in the
    // same whole nanoseconds `due_at` divides by.
    let elapsed = now.saturating_duration_since(started);
    let expired = elapsed.as_nanos() * u128::from(fps) / 1_000_000_000;
    (frame.max(u64::try_from(expired).unwrap_or(u64::MAX)), None)
}

/// Sleeps until the next frame is due and gives it, or returns `None` once told
/// to stop; the frame after it waits its turn, so a stall costs one late frame
/// rather than a burst of the frames it swallowed.
pub(crate) fn next_frame(
    started: Instant,
    frame: &mut u64,
    fps: u32,
    stop: &AtomicBool,
) -> Option<u64> {
    let (due, wait) = pace(started, *frame, fps, Instant::now());
    if let Some(wait) = wait {
        std::thread::sleep(wait);
    }
    if stop.load(Ordering::Relaxed) {
        return None;
    }
    *frame = due.saturating_add(1);
    Some(due)
}

pub(crate) fn test_pattern(
    mut sink: Sink,
    width: u32,
    height: u32,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    // Colour bars cost nothing to encode, so the pattern stays small unless a
    // measurement asks for a size: see `YAPFAST_STREAM_MAX_BITRATE`.
    let (width, height) = if std::env::var_os("YAPFAST_STREAM_MAX_BITRATE").is_some() {
        (width, height)
    } else {
        (width.min(640), height.min(360))
    };
    report(Report::Sharing { width, height });
    let started = Instant::now();
    let mut slot = 0u64;
    let mut rgba = vec![0; (width * height * 4) as usize];
    let mut picture = I420Buffer::new(width, height);
    let mut sampler = pressure::Sampler::new();
    while let Some(frame) = next_frame(started, &mut slot, sink.fps, stop) {
        let shift = (frame * 4) as u32;
        for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
            let (x, y) = (index as u32 % width, index as u32 / width);
            let bar = ((x + shift) * 8 / width) % 8;
            let level = 255 - (y * 128 / height) as u8;
            let on = |bit: u32| if bar & bit != 0 { level } else { 32 };
            pixel.copy_from_slice(&[on(4), on(2), on(1), 255]);
        }
        let (y_stride, u_stride, v_stride) = picture.strides();
        let (y, u, v) = picture.data_mut();
        // libyuv names formats by their order in a little-endian word: its
        // ABGR is R, G, B, A in memory.
        yuv_helper::abgr_to_i420(
            &rgba,
            width * 4,
            y,
            y_stride,
            u,
            u_stride,
            v,
            v_stride,
            width as i32,
            height as i32,
        );
        sink.frame(&picture)?;
        // Colour bars are all it shares.
        let _switch = sink.check(sampler.read(Instant::now()), report)?;
    }
    Ok(())
}
