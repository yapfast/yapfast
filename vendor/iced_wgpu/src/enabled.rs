//! Yapfast port: whether this renderer may start, as the app chooses
//! between it and iced_tiny_skia in one build.
//!
//! iced's fallback renderer tries wgpu, then tiny-skia, and reads the
//! choice only from `ICED_BACKEND`. Off until the app turns it on: the
//! app draws on the CPU unless its GPU setting says otherwise, and its
//! tests' headless renderer then is tiny-skia's, as the pictures are made
//! for it, without starting a GPU.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether the compositor and the headless renderer may start; they may
/// only once the app said so.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
