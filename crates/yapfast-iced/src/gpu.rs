//! Showing a stream worker's picture where it lies, without a copy.
//!
//! A worker that draws on the card publishes the texture it drew in, shared
//! with this process, in the frames file's header. [`iced_wgpu::video`] asks
//! this module, per picture, whether it can bind that texture directly instead
//! of uploading the picture into the renderer's atlas. What it hands back is a
//! bind group over the worker's own memory, so nothing crosses the processor.
//!
//! A picture opens only on the adapter that made it, so the worker's LUID
//! decides before anything is opened, and a picture this process can't show is
//! refused once: the worker then sends pixels from its next frame on, which is
//! slower but never blank.
//!
//! Only Windows imports a picture here. Linux's DMA-BUF import is the same
//! shape over Vulkan's external memory, and is not written yet: on Linux the
//! client takes the pixels path it already had.

#[cfg(windows)]
use std::collections::HashMap;
#[cfg(windows)]
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use std::sync::OnceLock;

#[cfg(windows)]
use iced_wgpu::wgpu;

/// A picture the client took from a stream's frames file, kept from the take
/// to the frame the renderer draws, with the bind group it was opened into.
#[cfg(windows)]
pub struct Video {
    picture: yapfast_media::frames::Picture,
    /// The bind group over the picture's own texture, made on the renderer's
    /// device the first time the picture is drawn, and kept for the frames the
    /// same texture is shown again.
    bind_group: OnceLock<Arc<wgpu::BindGroup>>,
}

#[cfg(windows)]
static VIDEOS: Mutex<Option<HashMap<usize, Arc<Video>>>> = Mutex::new(None);

/// How many streams' pictures are registered, so a picture costs no lock while
/// no video is on.
#[cfg(windows)]
static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Registers `video` under `address`, the pointer its handle carries, which is
/// what the renderer's hook looks it up by, and returns the address to forget.
#[cfg(windows)]
pub fn register(address: usize, video: Video) -> usize {
    let mut videos = VIDEOS.lock().unwrap_or_else(|p| p.into_inner());
    let map = videos.get_or_insert_default();
    if map.insert(address, Arc::new(video)).is_none() {
        COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    address
}

/// Forgets the picture registered under `address`.
#[cfg(windows)]
pub fn forget(address: usize) {
    let mut videos = VIDEOS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(map) = videos.as_mut()
        && map.remove(&address).is_some()
    {
        COUNT.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Sets the renderer's hook, once, so a picture registered here is drawn from
/// the worker's own texture.
#[cfg(windows)]
pub fn install() {
    iced_wgpu::video::set_hook(import);
}

#[cfg(not(windows))]
pub fn install() {}

/// The renderer's hook: the bind group over `import.handle`'s picture, if the
/// handle is one of this module's and its texture is open on this device.
#[cfg(windows)]
fn import(import: &iced_wgpu::video::Import<'_>) -> Option<iced_wgpu::video::Imported> {
    if COUNT.load(std::sync::atomic::Ordering::Relaxed) == 0 {
        return None;
    }
    let address = match import.handle {
        iced_wgpu::core::image::Handle::Rgba { pixels, .. } => pixels.as_ptr() as usize,
        _ => return None,
    };
    let video = {
        let videos = VIDEOS.lock().unwrap_or_else(|p| p.into_inner());
        videos.as_ref()?.get(&address).cloned()?
    };
    if let Some(bind_group) = video.bind_group.get() {
        return Some(iced_wgpu::video::Imported {
            bind_group: Arc::clone(bind_group),
            size: video.picture.width.max(1),
        });
    }
    let bind_group = opened(import.device, import.layout, &video.picture)?;
    let _ = video.bind_group.set(Arc::clone(&bind_group));
    Some(iced_wgpu::video::Imported {
        bind_group,
        size: video.picture.width.max(1),
    })
}

/// Opens the picture's texture on `device` and binds it as the image pipeline's
/// `texture_2d_array`, one layer, so the shader samples it over 0..1.
#[cfg(windows)]
fn opened(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    picture: &yapfast_media::frames::Picture,
) -> Option<Arc<wgpu::BindGroup>> {
    let texture = unsafe { open(device, picture) }.ok()?;
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("a stream's shared picture"),
        format: None,
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
        aspect: wgpu::TextureAspect::All,
        base_mip_level: 0,
        mip_level_count: None,
        base_array_layer: 0,
        array_layer_count: Some(1),
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("a stream's shared picture"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    Some(Arc::new(bind_group))
}

#[cfg(windows)]
use std::ffi::c_void;
#[cfg(windows)]
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Graphics::{
            Direct3D12::{
                D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_TEXTURE2D, ID3D12Resource,
            },
            Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM},
        },
    },
    core::Interface,
};

/// Opens the texture a picture is in, on the window's device: the worker's own
/// memory, with no pixel copied anywhere.
///
/// # Safety
///
/// The handle must be a shared texture the worker duplicated into this process
/// and one this process hasn't opened or closed before: it is closed here, as
/// the resource it names is opened.
#[cfg(windows)]
unsafe fn open(
    device: &wgpu::Device,
    picture: &yapfast_media::frames::Picture,
) -> std::result::Result<wgpu::Texture, String> {
    let resource: ID3D12Resource = unsafe { open_shared(device, picture.texture)? };
    // SAFETY: the resource is open on the device for as long as it is held.
    let described: D3D12_RESOURCE_DESC = unsafe { resource.GetDesc() };
    let format = shared_format(&described, picture.width, picture.height)?;
    let size = wgpu::Extent3d {
        width: picture.width,
        height: picture.height,
        depth_or_array_layers: 1,
    };
    let hal_texture = unsafe {
        wgpu::hal::dx12::Device::texture_from_raw(
            resource,
            format,
            wgpu::TextureDimension::D2,
            size,
            1,
            1,
        )
    };
    let desc = wgpu::TextureDescriptor {
        label: Some("a stream's shared picture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    // The worker's Direct3D 11 device writes without transitions of its own,
    // so wgpu tracks no state for the texture and the first use is the first
    // transition.
    Ok(unsafe {
        device.create_texture_from_hal::<wgpu::hal::dx12::Api>(
            hal_texture,
            &desc,
            wgpu::wgt::TextureUses::UNINITIALIZED,
        )
    })
}

/// Opens the resource a shared handle names, on the device, and closes the
/// handle: `OpenSharedHandle` opens the object and leaves the handle in this
/// process's table, where nothing else would close it.
#[cfg(windows)]
unsafe fn open_shared<T: Interface>(
    device: &wgpu::Device,
    handle: u64,
) -> std::result::Result<T, String> {
    let hal = unsafe { device.as_hal::<wgpu::hal::dx12::Api>() }
        .ok_or("the window's device is not Direct3D 12")?;
    let mut opened: Option<T> = None;
    // SAFETY: the handle is one the worker duplicated into this process, and
    // the device is the one it was shared for.
    let status = unsafe {
        hal.raw_device()
            .OpenSharedHandle(HANDLE(handle as *mut c_void), &mut opened)
    };
    // SAFETY: the handle is this process's, and nothing has closed it yet.
    unsafe {
        let _ = CloseHandle(HANDLE(handle as *mut c_void));
    }
    status.map_err(|error| format!("could not open the shared picture: {error}"))?;
    opened.ok_or_else(|| "the shared picture handle opened nothing".to_owned())
}

/// The wgpu format a shared picture is imported with, as the resource describes
/// itself: Direct3D takes the size and format given to `texture_from_raw` on
/// trust, so a texture declared RGBA where the producer drew BGRA is a picture
/// read through the wrong channels.
#[cfg(windows)]
fn shared_format(
    described: &D3D12_RESOURCE_DESC,
    width: u32,
    height: u32,
) -> std::result::Result<wgpu::TextureFormat, String> {
    if described.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D {
        return Err("the shared resource is not a 2D texture".to_owned());
    }
    if described.DepthOrArraySize != 1 || described.MipLevels != 1 || described.SampleDesc.Count != 1
    {
        return Err("the shared texture has slices, mips or samples a picture hasn't".to_owned());
    }
    if described.Width != u64::from(width) || described.Height != height {
        return Err(format!(
            "the shared texture is {}x{}, and the picture says {width}x{height}",
            described.Width, described.Height
        ));
    }
    if described.Format == DXGI_FORMAT_B8G8R8A8_UNORM {
        Ok(wgpu::TextureFormat::Bgra8Unorm)
    } else if described.Format == DXGI_FORMAT_R8G8B8A8_UNORM {
        Ok(wgpu::TextureFormat::Rgba8Unorm)
    } else {
        Err(format!(
            "the shared texture is DXGI format {}, which this path can't show",
            described.Format.0
        ))
    }
}
