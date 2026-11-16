//! The frame trace, off unless `YAPFAST_LOG=debug`: under the target
//! `yapfast::frames`, each frame the CPU renderer presents (waiting for a
//! buffer, the damage diff, the draw, the present, the area drawn and the
//! area the compositor is told changed, -1 for the whole window) and
//! each step a list's motion takes on a frame (`widgets::scroll_view`),
//! times in microseconds since the window began, so a frame's lines line
//! up.

use std::{sync::OnceLock, time::Instant};

/// The trace's log target.
pub const TARGET: &str = "yapfast::frames";

static EPOCH: OnceLock<Instant> = OnceLock::new();

/// Whether the trace is on.
pub fn on() -> bool {
    tracing::enabled!(target: TARGET, tracing::Level::DEBUG)
}

/// `at`, in microseconds since the window began.
pub fn micros(at: Instant) -> u64 {
    at.saturating_duration_since(*EPOCH.get_or_init(Instant::now))
        .as_micros() as u64
}

/// Starts the trace's clock, and with the trace on has the renderer tell
/// of each frame.
pub fn install() {
    let _ = EPOCH.get_or_init(Instant::now);
    // The GPU's frames go untold: wgpu has no such hook.
    if on() {
        iced_tiny_skia::timing::set_hook(presented);
    }
}

fn presented(frame: &iced_tiny_skia::timing::Frame) {
    tracing::debug!(
        target: TARGET,
        t = micros(frame.start),
        wait_us = frame.wait.as_micros() as u64,
        diff_us = frame.diff.as_micros() as u64,
        draw_us = frame.draw.as_micros() as u64,
        present_us = frame.present.as_micros() as u64,
        regions = frame.regions,
        kpx = (frame.area / 1000.0).round() as u32,
        shown_kpx = frame.shown.map_or(-1, |shown| (shown / 1000) as i64),
        "presented"
    );
}
