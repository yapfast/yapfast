//! Yapfast port (patch 26): whether this renderer may start, as the app
//! chooses between it and iced_wgpu in one build.
//!
//! iced's fallback renderer tries wgpu, then tiny-skia, and reads the
//! choice only from `ICED_BACKEND`. The app chooses once, before the
//! window opens, and turns this renderer off when it draws with the GPU,
//! so a GPU that fails to start is an error it can answer rather than a
//! window drawn by the renderer its pictures weren't made for.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Whether the compositor and the headless renderer may start; they may
/// unless the app said otherwise.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
