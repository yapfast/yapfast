//! Skia on Vulkan: the window's own device on Linux.
//!
//! Skia on OpenGL draws the window but can't show a picture another process
//! drew on the GPU: nothing hands a texture from one device or process to
//! another without a copy. Skia on Vulkan through wgpu 30 can, and the
//! vendored renderer already wraps a `wgpu::Texture` as a Skia image
//! (`wgpu_30_surface/vulkan.rs`, `import_vulkan_texture`). This is the device
//! that wrap needs: the one Skia draws the window with.
//!
//! Two things have to hold, and either one missing keeps the OpenGL path,
//! which is what every Linux build drew with before:
//!
//! - the build carries Skia's Vulkan, which the build's `vulkan` feature adds.
//!   Rust-skia's prebuilt Skia has no Vulkan, so a build without it cannot draw
//!   on one however good the driver is;
//! - a Vulkan device opens here. A machine with no Vulkan driver, or one whose
//!   card offers none to this process, keeps what it had.
//!
//! A device that won't open is a fallback and never a startup failure: what it
//! costs is the picture path, and the window still draws.
//!
//! The device is the window's presentation device, as the Direct3D 12 device
//! `gpu::install` makes on Windows is there, and a stream's DMA-BUF picture is
//! imported on this one and no other: a picture is shown where it lies, so what
//! shows it is the device the window draws with. `gpu::window_device` is handed
//! that device here, once, and `gpu::taken` imports on it from then on.

/// Whether this build carries Skia's Vulkan renderer at all.
///
/// A build without it can't draw on a Vulkan device whatever the machine has,
/// so the device isn't worth making there. Asking costs nothing.
pub fn carried() -> bool {
    cfg!(all(target_os = "linux", feature = "skia-vulkan"))
}

/// Where NixOS keeps the machine's Vulkan loader. A build's own loader is what
/// its wrapper puts on the library path, and a binary run without that wrapper
/// has none at all; the machine's is here, beside the drivers the same build was
/// not made from.
#[cfg(all(target_os = "linux", feature = "skia-vulkan"))]
const HOST_LOADER: &std::ffi::CStr = c"/run/opengl-driver/lib/libvulkan.so.1";

/// The Vulkan device the window draws with, for Slint to take through
/// `WGPUConfiguration::Manual`.
///
/// `Manual` is what keeps Slint from making a device of its own: a stream's
/// texture can be shown only on the device the window draws with, and this
/// hands it that one. It changes nothing about when the window repaints, as
/// the Direct3D 12 device on Windows changes nothing either.
#[cfg(all(target_os = "linux", feature = "skia-vulkan"))]
pub fn install() -> anyhow::Result<slint::wgpu_30::WGPUConfiguration> {
    let first = draw();
    let error = match first {
        Ok(configuration) => return Ok(configuration),
        Err(error) => error,
    };
    // wgpu's own words for an instance whose loader it couldn't load, which is
    // the one failure loading the machine's loader can answer: a device it found
    // none of says so differently, and asking again changes nothing there.
    if !format!("{error:#}").contains("drivers/libraries could not be loaded") {
        return Err(error);
    }
    if !host_loader() {
        return Err(error);
    }
    tracing::info!(
        loader = HOST_LOADER.to_str().unwrap_or_default(),
        "no Vulkan loader on the library path; the window's device is asked for again"
    );
    draw()
}

/// Loads the machine's Vulkan loader by its full path, and says whether it did.
///
/// Loading it registers the library under its soname, `libvulkan.so.1`, which is
/// the name wgpu's own `dlopen` asks for: the second instance then finds the
/// loader the first one couldn't, without this process's library path being set
/// before it started.
#[cfg(all(target_os = "linux", feature = "skia-vulkan"))]
fn host_loader() -> bool {
    // SAFETY: a path this process names itself, loaded to take its symbols and
    // never unloaded: the instance that reads it needs it for the life of the
    // window, so nothing here holds a handle to let go of.
    unsafe { !libc::dlopen(HOST_LOADER.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL).is_null() }
}

/// Asks for the window's Vulkan device once: the instance, the adapter, and the
/// device and queue the renderer wraps.
#[cfg(all(target_os = "linux", feature = "skia-vulkan"))]
fn draw() -> anyhow::Result<slint::wgpu_30::WGPUConfiguration> {
    use anyhow::Context as _;
    use slint::wgpu_30::wgpu;

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        // The card the displays are on, as Windows asks for, and the one the
        // window's surface will be made against.
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .context("no Vulkan adapter to draw Yapfast's window with")?;
    // What the renderer's own device asks for, since Skia wraps this device:
    // every feature and limit the card offers, less the experimental ones wgpu
    // keeps behind an opt-in of its own.
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("yapfast window"),
        required_features: adapter.features() - wgpu::Features::all_experimental_mask(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .context("could not make the window's Vulkan device")?;
    let info = adapter.get_info();
    tracing::info!(
        adapter = info.name,
        driver = info.driver,
        backend = %info.backend,
        "the window draws on Yapfast's own Vulkan device"
    );
    // The same device imports a stream's DMA-BUF picture, so the window knows
    // which device to draw for and a picture is shown where it lies.
    crate::ui::gpu::window_device(device.clone(), queue.clone());
    Ok(slint::wgpu_30::WGPUConfiguration::Manual {
        instance,
        adapter,
        device,
        queue,
    })
}

/// A build without Skia's Vulkan has no device to hand Slint, and the window
/// draws with Skia on OpenGL as it always did.
#[cfg(not(all(target_os = "linux", feature = "skia-vulkan")))]
pub fn install() -> anyhow::Result<slint::wgpu_30::WGPUConfiguration> {
    anyhow::bail!("this build draws with Skia on OpenGL")
}

/// Waits for one of wgpu's futures, both of which have done their work before
/// they are polled: a native adapter and its device come back ready, which is
/// what lets wgpu make its own in a single poll. So this polls, yielding a
/// turn, rather than carrying an executor for two calls.
#[cfg(all(target_os = "linux", feature = "skia-vulkan"))]
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::{
        pin::pin,
        task::{Context, Poll, Waker},
    };
    let mut context = Context::from_waker(Waker::noop());
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
