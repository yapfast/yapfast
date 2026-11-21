//! Yapfast port: drawing a video frame without a copy.
//!
//! A picture the client gets from a stream worker can already be a texture the
//! worker drew on the GPU and shared with this process (Windows) or a DMA-BUF
//! it exported (Linux). The image pipeline would upload such a picture into its
//! atlas like any other, which is the copy this avoids: the app sets a hook
//! here once, and the pipeline asks it, per picture, whether the picture is one
//! it can bind directly as its own texture. If it is, the pipeline samples that
//! texture over the frame's whole 0..1 UV instead of an atlas entry, and the
//! worker's memory is what shows.

use std::sync::{Arc, OnceLock};

/// A picture the pipeline asks the hook about.
pub struct Import<'a> {
    /// The renderer's device, which any bind group is made on.
    pub device: &'a wgpu::Device,
    /// The picture's handle, as the client made it. A video's handle names the
    /// frame the client took from the shared mapping.
    pub handle: &'a crate::core::image::Handle,
    /// The image pipeline's texture bind group layout: a `texture_2d_array` at
    /// binding 0, which a texture-only bind group must be made with.
    pub layout: &'a wgpu::BindGroupLayout,
}

/// What the hook hands back for a picture it can show without copying it.
pub struct Imported {
    /// A bind group over the picture's own texture view, one array layer, made
    /// with the pipeline's texture layout.
    pub bind_group: Arc<wgpu::BindGroup>,
    /// The size the shader maps 0..1 over; any non-zero value covers the whole
    /// texture, since the position is 0 and the scale is size over size.
    pub size: u32,
}

type Hook = fn(&Import<'_>) -> Option<Imported>;

static HOOK: OnceLock<Hook> = OnceLock::new();

/// Sets the one hook the image pipeline asks about every picture. Without one,
/// every picture is uploaded into the atlas as it always was.
pub fn set_hook(hook: Hook) {
    let _ = HOOK.set(hook);
}

/// Asks the hook about a picture, if one is set.
pub(crate) fn import(import: &Import<'_>) -> Option<Imported> {
    HOOK.get().and_then(|hook| hook(import))
}

type DeviceHook = fn(&wgpu::Device, &wgpu::Queue);

static DEVICE_HOOK: OnceLock<DeviceHook> = OnceLock::new();

/// Sets the one hook told of the renderer's device and queue once they are
/// made, so the app can import a worker's picture on the same device the
/// window draws with, and tell the worker which adapter it is.
pub fn set_device_hook(hook: DeviceHook) {
    let _ = DEVICE_HOOK.set(hook);
}

/// Tells the app the renderer's device and queue, when they are made.
pub(crate) fn device_made(device: &wgpu::Device, queue: &wgpu::Queue) {
    if let Some(hook) = DEVICE_HOOK.get() {
        hook(device, queue);
    }
}
