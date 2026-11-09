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
//! `gpu::install` makes on Windows is there, and a stream's texture opens on
//! this one and no other: a shared resource opens on the adapter that made it.
#![allow(
    unexpected_cfgs,
    reason = "`vulkan` is the build feature that carries Skia's Vulkan; until that build lands the cfg names a feature Cargo doesn't know yet, and afterwards it says exactly what this file needs to know"
)]

/// Whether this build carries Skia's Vulkan renderer at all.
///
/// A build without it can't draw on a Vulkan device whatever the machine has,
/// so the device isn't worth making there. Asking costs nothing.
pub fn carried() -> bool {
    cfg!(all(target_os = "linux", feature = "vulkan"))
}

/// The Vulkan device the window draws with, for Slint to take through
/// `WGPUConfiguration::Manual`.
///
/// `Manual` is what keeps Slint from making a device of its own: a stream's
/// texture can be shown only on the device the window draws with, and this
/// hands it that one. It changes nothing about when the window repaints, as
/// the Direct3D 12 device on Windows changes nothing either.
#[cfg(all(target_os = "linux", feature = "vulkan"))]
pub fn install() -> anyhow::Result<slint::wgpu_30::WGPUConfiguration> {
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
    Ok(slint::wgpu_30::WGPUConfiguration::Manual {
        instance,
        adapter,
        device,
        queue,
    })
}

/// A build without Skia's Vulkan has no device to hand Slint, and the window
/// draws with Skia on OpenGL as it always did.
#[cfg(not(all(target_os = "linux", feature = "vulkan")))]
pub fn install() -> anyhow::Result<slint::wgpu_30::WGPUConfiguration> {
    anyhow::bail!("this build draws with Skia on OpenGL")
}

/// Waits for one of wgpu's futures, both of which have done their work before
/// they are polled: a native adapter and its device come back ready, which is
/// what lets wgpu make its own in a single poll. So this polls, yielding a
/// turn, rather than carrying an executor for two calls.
#[cfg(all(target_os = "linux", feature = "vulkan"))]
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
