//! Which renderer draws: tiny-skia on the CPU or wgpu on the GPU, both in
//! the one build, chosen once as Yapfast starts (`choose`), before any
//! picture is made, as each takes its pixels in its own format. The CPU's
//! unless chosen otherwise, as the tests draw.

use std::sync::atomic::{AtomicBool, Ordering};

static GPU: AtomicBool = AtomicBool::new(false);

/// Draws with the GPU, or on the CPU: only that renderer may start when
/// the window opens, so a GPU that can't start is the window's error,
/// not a window drawn on the CPU from pictures made for the GPU.
pub fn choose(gpu: bool) {
    GPU.store(gpu, Ordering::Relaxed);
    iced_wgpu::set_enabled(gpu);
    iced_tiny_skia::set_enabled(!gpu);
}

/// Whether the GPU draws: pictures are then straight RGBA, as wgpu uploads
/// them; on the CPU, premultiplied BGRA, as tiny-skia copies them.
pub fn gpu() -> bool {
    GPU.load(Ordering::Relaxed)
}
