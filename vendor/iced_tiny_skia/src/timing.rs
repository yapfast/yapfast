//! Yapfast port (patch 22): how long each frame took to draw and present,
//! for the app's frame trace.
//!
//! The app sets a hook once (`set_hook`), and only while its trace is on;
//! without one, `present` reads no clock.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// One frame's `present`.
pub struct Frame {
    /// When `present` began.
    pub start: Instant,
    /// Waiting for a buffer the compositor had let go of.
    pub wait: Duration,
    /// Telling what changed from the buffer's last frame.
    pub diff: Duration,
    /// Drawing the damaged regions.
    pub draw: Duration,
    /// Handing the buffer to the compositor.
    pub present: Duration,
    /// The damaged regions drawn, and their area in logical pixels.
    pub regions: usize,
    pub area: f32,
    /// What the compositor was told changed (patch 23), in the buffer's
    /// pixels; `None` when it was the whole window.
    pub shown: Option<u64>,
}

/// Told of each frame presented.
pub type Hook = fn(&Frame);

static HOOK: OnceLock<Hook> = OnceLock::new();

/// Sets the hook, once.
pub fn set_hook(hook: Hook) {
    let _ = HOOK.set(hook);
}

pub(crate) fn hook() -> Option<Hook> {
    HOOK.get().copied()
}
