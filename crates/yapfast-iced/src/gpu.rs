//! Showing a stream worker's picture where it lies, without a copy.
//!
//! A worker that draws on the card publishes the picture it drew in the frames
//! file's header: a texture it shared into this process on Windows, a DMA-BUF
//! on Linux. [`iced_wgpu::video`] asks this module, per picture, whether it can
//! bind that picture directly instead of uploading it into the renderer's
//! atlas. What it hands back is a bind group over the worker's own memory, so
//! nothing crosses the processor.
//!
//! The renderer makes its device when the window opens; [`video::set_device_hook`]
//! tells this module of it, so a picture is opened on the very device the window
//! draws with, and each stream's worker is told which device that is before it
//! draws. A picture that this build can't open is refused once, and the stream
//! sends pixels from its next frame: slower, but never blank.

use std::sync::{Arc, OnceLock};

use iced_wgpu::wgpu;

#[cfg(target_os = "linux")]
use yapfast_core::dmabuf;
#[cfg(any(windows, target_os = "linux"))]
use yapfast_core::frames::Frames;
#[cfg(target_os = "linux")]
mod linux;

/// The window's device, its queue, and the number the frames header calls its
/// adapter: what a picture is opened on, and what a worker compares with its own
/// device before it draws for this window.
static DEVICE: OnceLock<(wgpu::Device, wgpu::Queue, u64)> = OnceLock::new();

/// The number the frames header calls this window's adapter, if the renderer's
/// device is known.
pub fn number() -> Option<u64> {
    DEVICE.get().map(|(_, _, number)| *number)
}

/// The window's queue, for the work a picture's import submits.
pub(super) fn queue() -> Option<&'static wgpu::Queue> {
    DEVICE.get().map(|(_, queue, _)| queue)
}

/// Sets the hooks the renderer calls, once the window draws on the GPU.
pub fn install() {
    iced_wgpu::video::set_device_hook(device_made);
    #[cfg(any(windows, target_os = "linux"))]
    iced_wgpu::video::set_hook(import);
}

/// The renderer's device and queue, when it makes them: opens the door for a
/// picture, and names the adapter for the worker.
fn device_made(device: &wgpu::Device, queue: &wgpu::Queue) {
    if DEVICE.get().is_some() {
        return;
    }
    let number = adapter_number(device);
    drop(DEVICE.set((device.clone(), queue.clone(), number)));
    tracing::debug!(number, "the window's device takes streams' pictures");
}

/// The number the frames header carries for this window's adapter.
#[cfg(windows)]
fn adapter_number(device: &wgpu::Device) -> u64 {
    // SAFETY: the device is the window's and is alive here.
    let Some(hal) = (unsafe { device.as_hal::<wgpu::hal::dx12::Api>() }) else {
        return 0;
    };
    // SAFETY: the device is Direct3D 12, and the LUID is its adapter's.
    let luid = unsafe { hal.raw_device().GetAdapterLuid() };
    u64::from(luid.LowPart) | ((luid.HighPart as u32 as u64) << 32)
}

#[cfg(target_os = "linux")]
fn adapter_number(device: &wgpu::Device) -> u64 {
    // SAFETY: the device is the window's and is alive here.
    let Some(hal) = (unsafe { device.as_hal::<wgpu::hal::vulkan::Api>() }) else {
        return 0;
    };
    // SAFETY: the physical device is the one this device was made on, and
    // Vulkan answers its properties at any time.
    let properties = unsafe {
        hal.shared_instance()
            .raw_instance()
            .get_physical_device_properties(hal.raw_physical_device())
    };
    (u64::from(properties.vendor_id) << 32) | u64::from(properties.device_id)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn adapter_number(_device: &wgpu::Device) -> u64 {
    0
}

/// A picture the client took from a stream's frames file, kept from the take to
/// the frame the renderer draws, with the bind group it was opened into.
#[cfg(any(windows, target_os = "linux"))]
pub struct Video {
    source: Source,
    /// The stream this picture is from, and the slot it is in, which is the
    /// worker's again only once this process retires it.
    frames: Arc<Frames>,
    slot: u32,
    generation: u64,
    /// The bind group over the picture's own texture, made on the renderer's
    /// device the first time the picture is drawn.
    bind_group: OnceLock<Arc<wgpu::BindGroup>>,
}

#[cfg(any(windows, target_os = "linux"))]
enum Source {
    #[cfg(windows)]
    Shared(yapfast_core::frames::Picture),
    #[cfg(target_os = "linux")]
    Dmabuf(dmabuf::Picture),
}

#[cfg(any(windows, target_os = "linux"))]
impl Video {
    #[cfg(windows)]
    pub fn new(picture: yapfast_core::frames::Picture, frames: Arc<Frames>) -> Self {
        Self {
            source: Source::Shared(picture),
            slot: picture.slot,
            generation: picture.generation,
            frames,
            bind_group: OnceLock::new(),
        }
    }

    #[cfg(target_os = "linux")]
    pub fn dmabuf(picture: dmabuf::Picture, frames: Arc<Frames>) -> Self {
        Self {
            slot: picture.slot(),
            generation: picture.generation(),
            source: Source::Dmabuf(picture),
            frames,
            bind_group: OnceLock::new(),
        }
    }

    fn size(&self) -> u32 {
        match &self.source {
            #[cfg(windows)]
            Source::Shared(picture) => picture.width,
            #[cfg(target_os = "linux")]
            Source::Dmabuf(picture) => picture.width(),
        }
    }
}

#[cfg(any(windows, target_os = "linux"))]
static VIDEOS: std::sync::Mutex<Option<std::collections::HashMap<usize, Arc<Video>>>> =
    std::sync::Mutex::new(None);

#[cfg(any(windows, target_os = "linux"))]
static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Registers `video` under `address`, the pointer its handle carries, which is
/// what the renderer's hook looks it up by.
#[cfg(any(windows, target_os = "linux"))]
pub fn register(address: usize, video: Video) {
    let mut videos = VIDEOS.lock().unwrap_or_else(|p| p.into_inner());
    if videos
        .get_or_insert_default()
        .insert(address, Arc::new(video))
        .is_none()
    {
        COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Forgets the picture registered under `address`.
#[cfg(any(windows, target_os = "linux"))]
pub fn forget(address: usize) {
    let mut videos = VIDEOS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(map) = videos.as_mut()
        && let Some(video) = map.remove(&address)
    {
        COUNT.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        // The picture is no longer on screen, so the worker may draw into its
        // slot again.
        video.frames.retire(video.slot, video.generation);
    }
}

/// The renderer's hook: the bind group over `import.handle`'s picture, if the
/// handle is one of this module's and its texture is open on this device.
#[cfg(any(windows, target_os = "linux"))]
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
            size: video.size().max(1),
        });
    }
    let imported = match &video.source {
        #[cfg(windows)]
        Source::Shared(picture) => opened(import.device, import.layout, picture),
        #[cfg(target_os = "linux")]
        Source::Dmabuf(picture) => linux::import(import.device, import.layout, picture),
    };
    // A picture this build can't show is refused once, so the worker sends
    // pixels from its next frame, and its slot goes back now rather than being
    // held for a picture nothing shows. Until those pixels come, the tile is
    // blank rather than the sentinel's one byte drawn as a picture.
    let Some(bind_group) = imported else {
        if video.frames.refuse_gpu() {
            tracing::warn!("a stream's picture can't be shown here; showing its pixels");
        }
        video.frames.retire(video.slot, video.generation);
        return Some(iced_wgpu::video::Imported {
            bind_group: transparent(import.device, import.layout),
            size: 1,
        });
    };
    drop(video.bind_group.set(Arc::clone(&bind_group)));
    Some(iced_wgpu::video::Imported {
        bind_group,
        size: video.size().max(1),
    })
}

/// A bind group over a transparent 1x1 picture, made once: what a frame this
/// build can't import is drawn as, so the tile shows nothing that frame rather
/// than a sentinel's one byte read as pixels.
#[cfg(any(windows, target_os = "linux"))]
static CLEAR: OnceLock<Arc<wgpu::BindGroup>> = OnceLock::new();

#[cfg(any(windows, target_os = "linux"))]
fn transparent(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Arc<wgpu::BindGroup> {
    CLEAR
        .get_or_init(|| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("a stream's picture nothing shows"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            if let Some(queue) = queue() {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &[0, 0, 0, 0],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4),
                        rows_per_image: Some(1),
                    },
                    wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                );
            }
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                array_layer_count: Some(1),
                ..Default::default()
            });
            Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("a stream's picture nothing shows"),
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            }))
        })
        .clone()
}

// ---------------------------------------------------------------------------
// Windows: a texture the worker shared into this process, opened on the window's
// Direct3D 12 device.

#[cfg(windows)]
fn opened(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    picture: &yapfast_core::frames::Picture,
) -> Option<Arc<wgpu::BindGroup>> {
    // SAFETY: the picture is one this module registered, and the device is the
    // window's.
    let texture = unsafe { open(device, picture) }.ok()?;
    // The worker draws the picture on its own Direct3D 11 device and signals the
    // fence it shares before it publishes: wait for it here, so the shader never
    // samples memory the card is still writing.
    let _ = finished(device, picture);
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
use std::time::{Duration, Instant};
#[cfg(windows)]
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Graphics::{
            Direct3D12::{
                D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_TEXTURE2D, ID3D12Fence,
                ID3D12Resource,
            },
            Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM},
        },
        System::Threading::{CreateEventW, ResetEvent, WaitForSingleObject},
    },
    core::{Interface, PCWSTR},
};

/// Opens the texture a picture is in, on the window's device: the worker's own
/// memory, with no pixel copied anywhere.
///
/// # Safety
///
/// The handle must be a shared texture the worker duplicated into this process
/// and one this process hasn't opened or closed before: it is closed here.
#[cfg(windows)]
unsafe fn open(
    device: &wgpu::Device,
    picture: &yapfast_core::frames::Picture,
) -> std::result::Result<wgpu::Texture, String> {
    // SAFETY: the handle is one the worker duplicated into this process, and
    // the device is the one it was shared for.
    let resource: ID3D12Resource = unsafe { open_shared(device, picture.texture)? };
    // SAFETY: the resource is open on the device for as long as it is held.
    let described: D3D12_RESOURCE_DESC = unsafe { resource.GetDesc() };
    let format = shared_format(&described, picture.width, picture.height)?;
    let size = wgpu::Extent3d {
        width: picture.width,
        height: picture.height,
        depth_or_array_layers: 1,
    };
    // SAFETY: the resource is the worker's, open on this device, and the
    // description read above is what the raw texture is told.
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
    // SAFETY: `hal_texture` was made from this device's raw handle and matches `desc`.
    Ok(unsafe { device.create_texture_from_hal::<wgpu::hal::dx12::Api>(hal_texture, &desc) })
}

/// Opens the resource a shared handle names, on the device, and closes the
/// handle: `OpenSharedHandle` opens the object and leaves the handle in this
/// process's table, where nothing else would close it.
#[cfg(windows)]
unsafe fn open_shared<T: Interface>(
    device: &wgpu::Device,
    handle: u64,
) -> std::result::Result<T, String> {
    // SAFETY: the device is the window's and is alive here.
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
        drop(CloseHandle(HANDLE(handle as *mut c_void)));
    }
    status.map_err(|error| format!("could not open the shared picture: {error}"))?;
    opened.ok_or_else(|| "the shared picture handle opened nothing".to_owned())
}

/// The wgpu format a shared picture is imported with, as the resource describes
/// itself.
#[cfg(windows)]
fn shared_format(
    described: &D3D12_RESOURCE_DESC,
    width: u32,
    height: u32,
) -> std::result::Result<wgpu::TextureFormat, String> {
    if described.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D {
        return Err("the shared resource is not a 2D texture".to_owned());
    }
    if described.DepthOrArraySize != 1
        || described.MipLevels != 1
        || described.SampleDesc.Count != 1
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

// The fence a stream's pictures are drawn behind, opened once for the set they
// are shared in, and the event a wait sleeps on. The renderer's hook runs on the
// window's thread, where a picture is opened and the fence is waited on.
#[cfg(windows)]
thread_local! {
    static WAIT: std::cell::RefCell<Option<(u64, ID3D12Fence, HANDLE)>> =
        const { std::cell::RefCell::new(None) };
}

/// How long a picture may take to be drawn before it's passed over: half a frame
/// at 60 Hz, which a worker that signals its fence before it publishes never
/// spends.
#[cfg(windows)]
const WAIT_MS: u64 = 8;

/// Waits for the picture to be drawn, and says whether it was.
#[cfg(windows)]
fn finished(device: &wgpu::Device, picture: &yapfast_core::frames::Picture) -> bool {
    WAIT.with_borrow_mut(|slot| {
        if slot.as_ref().is_none_or(|(set, _, _)| *set != picture.set) {
            // SAFETY: the fence handle is one the worker duplicated into this
            // process, and the device is the one it was shared for.
            let Ok(fence) = (unsafe { open_shared::<ID3D12Fence>(device, picture.fence) }) else {
                return false;
            };
            // SAFETY: an auto-reset event with no name, owned here.
            let Ok(event) = (unsafe { CreateEventW(None, false, false, PCWSTR::null()) }) else {
                return false;
            };
            *slot = Some((picture.set, fence, event));
        }
        let Some((_, fence, event)) = slot.as_ref() else {
            return false;
        };
        wait(*event, fence, picture.fence_value)
    })
}

/// Waits until the fence reaches `value`, up to [`WAIT_MS`].
#[cfg(windows)]
fn wait(event: HANDLE, fence: &ID3D12Fence, value: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(WAIT_MS);
    loop {
        if let Some(drawn) = drawn(fence, value) {
            return drawn;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return drawn(fence, value).unwrap_or(false);
        }
        // SAFETY: the fence and the event are this process's, opened above.
        unsafe {
            // A notification left from a timed-out wait isn't this picture's.
            drop(ResetEvent(event));
            if fence.SetEventOnCompletion(value, event).is_err() {
                return false;
            }
            let millis = u32::try_from(left.as_millis()).unwrap_or(u32::MAX).max(1);
            let _ = WaitForSingleObject(event, millis);
        }
    }
}

/// Whether the fence has reached `value`, or `None` while it hasn't yet.
#[cfg(windows)]
fn drawn(fence: &ID3D12Fence, value: u64) -> Option<bool> {
    // SAFETY: the fence is open on the device for as long as it is held.
    let reached = unsafe { fence.GetCompletedValue() };
    match reached {
        // A removed device answers the largest value there is, which is not a
        // picture drawn.
        u64::MAX => Some(false),
        reached if reached >= value => Some(true),
        _ => None,
    }
}
