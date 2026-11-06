//! Showing a stream's picture without copying it.
//!
//! A worker that can draw a frame on the GPU publishes the texture it drew it
//! in, shared with this process, in the frames file's header. The UI opens that
//! texture on the device Skia draws the window with and hands Slint an `Image`
//! of it, so what shows is the worker's own memory: nothing crosses the CPU.
//! Copying one 720p picture costs 1.1-1.9 ms, 3-6 % of a core per stream at
//! 30 fps; showing it this way costs neither.
//!
//! Two things the spike in `spike/gpu-present` found shape the code. A shared
//! resource opens only on the adapter that made it and refuses with
//! `E_INVALIDARG` on any other, so the worker's adapter LUID decides before
//! anything is opened, and a mismatch is a decision rather than a failure to
//! retry. And a keyed mutex does not survive the hop to Direct3D 12 — the
//! D3D12 view of the resource has no `IDXGIKeyedMutex` — so the worker's fence,
//! not a mutex, is what says a picture is finished.
//!
//! A stream whose pictures can't be shown here says so in the frames header,
//! once, and goes on with pixels from its next frame: a stream that falls back
//! costs what it costs today, and a picture that can't be shown is a slow
//! stream rather than a blank tile.

use std::sync::Arc;

use yapfast_core::frames::{Frame, Frames, Picture};

#[cfg(windows)]
use std::{cell::RefCell, collections::HashMap, ffi::c_void, sync::Weak};

#[cfg(windows)]
use anyhow::{Context as _, Result};
#[cfg(windows)]
use slint::wgpu_30::wgpu;
#[cfg(windows)]
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        Graphics::Direct3D12::{ID3D12Fence, ID3D12Resource},
        System::Threading::{CreateEventW, ResetEvent, WaitForSingleObject},
    },
    core::PCWSTR,
};

/// The newest picture of a stream, as Slint shows it: the pixels the worker
/// wrote, copied out of the frames file, or the worker's own texture, shown
/// where the worker drew it.
///
/// Nothing comes back while nothing new arrived. A picture this process can't
/// show comes back as nothing too, with the worker asked for pixels from its
/// next frame on: the picture before it stays up for the moment that takes,
/// rather than a blank tile or a torn one.
pub fn taken(frames: &Arc<Frames>) -> Option<slint::Image> {
    let shown: Option<std::result::Result<slint::Image, Picture>> =
        frames.take_frame(|frame| match frame {
            Frame::Pixels {
                width,
                height,
                pixels,
            } => Ok(copied(width, height, pixels)),
            Frame::Gpu(picture) => Err(picture),
        });
    match shown? {
        Ok(image) => Some(image),
        Err(picture) => shared(frames, picture),
    }
}

/// A frame's pixels as Slint shows them, copied out of the frames file.
pub fn copied(width: u32, height: u32, pixels: &[u8]) -> slint::Image {
    slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(
        pixels, width, height,
    ))
}

/// A picture the worker drew, shown from its texture if this process can open
/// it, and asked for as pixels if it can't.
#[cfg(windows)]
fn shared(frames: &Arc<Frames>, picture: Picture) -> Option<slint::Image> {
    PRESENT.with_borrow_mut(|present| match present.as_mut() {
        Some(present) => present.show(frames, picture),
        // Slint draws with software, or with a device it made for itself: a
        // texture another device made can't be shown by either, and a stream
        // is asked for its pixels instead.
        None => {
            if frames.refuse_gpu() {
                tracing::warn!(
                    "the window has no Direct3D 12 device of its own; showing streams' pixels"
                );
            }
            None
        }
    })
}

#[cfg(not(windows))]
fn shared(frames: &Arc<Frames>, _picture: Picture) -> Option<slint::Image> {
    if frames.refuse_gpu() {
        tracing::warn!(
            "a stream draws on a shared texture, which Yapfast can open only on Windows; showing its pixels"
        );
    }
    None
}

#[cfg(windows)]
thread_local! {
    /// The window's own device, once it has one: a stream's shared texture is
    /// opened on it, and nothing else can show it anyway.
    static PRESENT: RefCell<Option<Present>> = const { RefCell::new(None) };
}

/// Makes the Direct3D 12 device the window draws with, which is also the only
/// device a stream's shared texture can be opened on, and hands it to Slint.
///
/// Slint takes it through `WGPUConfiguration::Manual`, so this needs no
/// rendering notifier and changes nothing about when the window repaints:
/// partial repainting was never active on the GPU surface, and the notifier
/// cost what the spike measured, which was nothing.
#[cfg(windows)]
pub fn install() -> Result<slint::wgpu_30::WGPUConfiguration> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        // The adapter the displays are on, which is the one a decoder picks:
        // a picture can only be opened on the adapter that drew it.
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .context("no Direct3D 12 adapter to draw Yapfast's window with")?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("yapfast window"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .context("could not make the window's Direct3D 12 device")?;
    let luid = adapter_luid(&device)?;
    let event = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
        .context("could not make the event a stream's fence is waited on")?;
    let info = adapter.get_info();
    tracing::info!(
        adapter = info.name,
        driver = info.driver,
        "the window draws on Yapfast's own Direct3D 12 device"
    );
    PRESENT.with_borrow_mut(|present| {
        *present = Some(Present {
            device: device.clone(),
            luid,
            event,
            streams: HashMap::new(),
        });
    });
    Ok(slint::wgpu_30::WGPUConfiguration::Manual {
        instance,
        adapter,
        device,
        queue,
    })
}

/// Without Direct3D there is no shared texture to open, and a stream that
/// publishes one is asked for pixels.
#[cfg(not(windows))]
pub fn install() -> Result<slint::wgpu_30::WGPUConfiguration> {
    anyhow::bail!("a shared picture needs Direct3D 12, which only Windows has")
}

/// The adapter's LUID, low half first, which the worker puts in the frames
/// header: a shared texture opens on the adapter that made it and on no other,
/// so this is what a worker checks before it draws a picture for this window.
#[cfg(windows)]
fn adapter_luid(device: &wgpu::Device) -> Result<u64> {
    let hal = unsafe { device.as_hal::<wgpu::hal::dx12::Api>() }
        .context("the window's device is not Direct3D 12")?;
    let luid = unsafe { hal.raw_device().GetAdapterLuid() };
    Ok(u64::from(luid.LowPart) | ((luid.HighPart as u32 as u64) << 32))
}

/// How long a picture may take to be drawn before it's passed over: half a
/// frame at 60 Hz, which a worker that signals its fence before it publishes
/// never spends, and short enough that the window doesn't feel it.
#[cfg(windows)]
const WAIT_MS: u32 = 8;

/// How many pictures in a row may arrive before the GPU has finished drawing
/// them before a stream gives up on the GPU path. One is a slow frame; a run of
/// them is a worker publishing before its own fence says the picture is done,
/// which no window should show.
#[cfg(windows)]
const LATE_LIMIT: u32 = 10;

/// How many textures one stream keeps open: its slots, and one more for a
/// worker that swaps a texture out as the picture changes size. A worker
/// cycling more than that isn't drawing into the triple buffer's slots.
#[cfg(windows)]
const KEPT: usize = 4;

/// The window's device, the adapter it runs on, and what was opened of each
/// stream's pictures.
#[cfg(windows)]
struct Present {
    /// The device Skia draws with: a shared texture is opened on it, and shown
    /// by it.
    device: wgpu::Device,
    /// The adapter it runs on, low half of the LUID first.
    luid: u64,
    /// One auto-reset event, reused: waiting for a picture's fence is a single
    /// call, and there is one window.
    event: HANDLE,
    /// One entry per stream, by the address of the frames it reads them from.
    streams: HashMap<usize, Stream>,
}

#[cfg(windows)]
impl Drop for Present {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.event);
        }
    }
}

/// What the UI opened of one stream's pictures.
#[cfg(windows)]
struct Stream {
    /// The stream this was made for, so a stream the UI let go leaves an
    /// address that a new one takes over without inheriting what was opened
    /// for the one before it.
    frames: Weak<Frames>,
    /// The textures opened, by the handle the worker published them under: it
    /// draws in one texture per slot, so each is opened once and shown for as
    /// long as it holds pictures.
    textures: Vec<Opened>,
    /// The fence the stream's pictures are drawn behind, and the handle it was
    /// opened from: a worker that makes another one is followed.
    fence: Option<(u64, ID3D12Fence)>,
    /// How many pictures in a row arrived before the GPU had drawn them.
    late: u32,
}

#[cfg(windows)]
struct Opened {
    handle: u64,
    texture: wgpu::Texture,
}

#[cfg(windows)]
impl Present {
    /// Shows a picture the worker drew, or refuses this stream's pictures and
    /// says why, once.
    fn show(&mut self, frames: &Arc<Frames>, picture: Picture) -> Option<slint::Image> {
        // The worker must draw on the adapter the window draws on: the
        // resource can't cross adapters, and Direct3D says so with
        // `E_INVALIDARG` at the open rather than trusting the caller.
        if picture.luid != self.luid {
            if frames.refuse_gpu() {
                tracing::warn!(
                    worker = format!("{:#x}", picture.luid),
                    window = format!("{:#x}", self.luid),
                    "a stream draws on another adapter; showing its pixels instead"
                );
            }
            return None;
        }
        let (event, luid) = (self.event, self.luid);
        let device = &self.device;
        let streams = &mut self.streams;
        // A stream the UI let go of leaves its address to the next one, which
        // must not inherit what was opened for it.
        streams.retain(|_, stream| stream.frames.strong_count() > 0);
        let key = Arc::as_ptr(frames) as usize;
        let stream = streams.entry(key).or_insert_with(|| Stream {
            frames: Arc::downgrade(frames),
            textures: Vec::new(),
            fence: None,
            late: 0,
        });
        if !std::ptr::eq(stream.frames.as_ptr(), Arc::as_ptr(frames)) {
            *stream = Stream {
                frames: Arc::downgrade(frames),
                textures: Vec::new(),
                fence: None,
                late: 0,
            };
        }
        let texture = match stream.texture(device, &picture) {
            Ok(texture) => texture,
            Err(error) => {
                refuse(frames, error, "could not open a stream's shared texture");
                return None;
            }
        };
        match stream.finished(device, event, &picture) {
            Ok(true) => stream.late = 0,
            Ok(false) => {
                stream.late += 1;
                if stream.late < LATE_LIMIT {
                    // The GPU is still drawing it, so the picture before it
                    // stays up: a torn one is worse than a slow one.
                    return None;
                }
                refuse(
                    frames,
                    format!("{} pictures in a row were not drawn yet", stream.late),
                    "a stream publishes pictures its fence hasn't reached",
                );
                return None;
            }
            Err(error) => {
                refuse(frames, error, "could not open a stream's fence");
                return None;
            }
        }
        match slint::Image::try_from(texture) {
            Ok(image) => {
                // From here the worker knows which adapter to draw for, and
                // that its pictures are shown rather than copied.
                frames.accept_gpu(luid);
                Some(image)
            }
            Err(error) => {
                refuse(
                    frames,
                    format!("{error:?}"),
                    "Slint can't show a stream's shared texture",
                );
                None
            }
        }
    }
}

/// Asks a stream for pixels from its next frame, and logs why the first time:
/// a stream decides once, because what it depends on — the pair of adapters,
/// the renderer — doesn't change while the stream lasts.
#[cfg(windows)]
fn refuse(frames: &Frames, reason: String, what: &str) {
    if frames.refuse_gpu() {
        tracing::warn!(reason = %reason, "{}; showing its pixels instead", what);
    }
}

#[cfg(windows)]
impl Stream {
    /// The texture a picture is in, opened on the window's device the first
    /// time it's seen and kept from then on.
    fn texture(
        &mut self,
        device: &wgpu::Device,
        picture: &Picture,
    ) -> std::result::Result<wgpu::Texture, String> {
        if let Some(opened) = self
            .textures
            .iter()
            .find(|opened| opened.handle == picture.texture)
        {
            return Ok(opened.texture.clone());
        }
        let texture = unsafe { open(device, picture) }?;
        self.textures.push(Opened {
            handle: picture.texture,
            texture: texture.clone(),
        });
        if self.textures.len() > KEPT {
            self.textures.remove(0);
        }
        Ok(texture)
    }

    /// Whether the picture is drawn, opening the fence the first time the
    /// worker names one.
    fn finished(
        &mut self,
        device: &wgpu::Device,
        event: HANDLE,
        picture: &Picture,
    ) -> std::result::Result<bool, String> {
        if self
            .fence
            .as_ref()
            .is_none_or(|(handle, _)| *handle != picture.fence)
        {
            self.fence = Some((picture.fence, unsafe { open_fence(device, picture.fence) }?));
        }
        let Some((_, fence)) = &self.fence else {
            return Ok(false);
        };
        Ok(wait(event, fence, picture.fence_value))
    }
}

/// Opens the texture a picture is in, on the window's device: what comes back
/// is the worker's own memory, with no pixel of it copied anywhere. Slint shows
/// it by asking Skia for a surface over the same Direct3D 12 resource, which is
/// the hop the spike proved.
///
/// Safety: the handle must be a shared texture the worker duplicated into this
/// process, and it must be `R8G8B8A8_UNORM`, one mip, one sample, and
/// `picture.width` by `picture.height` as the header says: Direct3D 12 takes
/// the size and the format on trust.
#[cfg(windows)]
unsafe fn open(
    device: &wgpu::Device,
    picture: &Picture,
) -> std::result::Result<wgpu::Texture, String> {
    let hal = unsafe { device.as_hal::<wgpu::hal::dx12::Api>() }
        .ok_or("the window's device is not Direct3D 12")?;
    let mut resource: Option<ID3D12Resource> = None;
    unsafe {
        hal.raw_device()
            .OpenSharedHandle(HANDLE(picture.texture as *mut c_void), &mut resource)
            .map_err(|error| format!("could not open the shared texture: {error}"))?;
    }
    let resource = resource.ok_or("the shared handle opened nothing")?;
    let size = wgpu::Extent3d {
        width: picture.width,
        height: picture.height,
        depth_or_array_layers: 1,
    };
    let hal_texture = unsafe {
        wgpu::hal::dx12::Device::texture_from_raw(
            resource,
            wgpu::TextureFormat::Rgba8Unorm,
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
        format: wgpu::TextureFormat::Rgba8Unorm,
        // What Slint asks of a texture it shows, and what the worker's
        // Direct3D 11 texture was made for: a shader resource and a render
        // target.
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    // `UNINITIALIZED` says wgpu tracks no state for the texture: the worker's
    // Direct3D 11 device writes it without transitions of its own, so the
    // first use is the first transition, and it starts from where Direct3D
    // leaves a resource it hasn't been told about.
    Ok(unsafe {
        device.create_texture_from_hal::<wgpu::hal::dx12::Api>(
            hal_texture,
            &desc,
            wgpu::wgt::TextureUses::UNINITIALIZED,
        )
    })
}

/// Opens the fence a stream's pictures are drawn behind.
///
/// Safety: the handle must be a shared fence the worker duplicated into this
/// process.
#[cfg(windows)]
unsafe fn open_fence(
    device: &wgpu::Device,
    handle: u64,
) -> std::result::Result<ID3D12Fence, String> {
    let hal = unsafe { device.as_hal::<wgpu::hal::dx12::Api>() }
        .ok_or("the window's device is not Direct3D 12")?;
    let mut fence: Option<ID3D12Fence> = None;
    unsafe {
        hal.raw_device()
            .OpenSharedHandle(HANDLE(handle as *mut c_void), &mut fence)
            .map_err(|error| format!("could not open the shared fence: {error}"))?;
    }
    fence.ok_or_else(|| "the shared fence handle opened nothing".to_string())
}

/// Waits for a picture to be drawn, and says whether it was.
///
/// The fence is the whole guarantee: a worker draws into a texture this process
/// holds, publishes it, and only its fence value says the GPU finished the
/// drawing. A picture a frame behind its fence is waited for here, and one that
/// doesn't reach its fence in time is passed over rather than shown torn.
#[cfg(windows)]
fn wait(event: HANDLE, fence: &ID3D12Fence, value: u64) -> bool {
    unsafe {
        // The usual case: the worker signalled before it published, and the
        // GPU got there while the UI was drawing the frame before this one.
        if fence.GetCompletedValue() >= value {
            return true;
        }
        // A wait that timed out leaves the event set behind it, and an event
        // already set would make this wait return at once.
        let _ = ResetEvent(event);
        if fence.SetEventOnCompletion(value, event).is_err() {
            return false;
        }
        WaitForSingleObject(event, WAIT_MS) == WAIT_OBJECT_0
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;
    use yapfast_core::frames::{Drawn, Writer};

    /// The window's own device is the one a stream's picture can be opened on,
    /// so a picture drawn for another adapter is refused before anything is
    /// opened, and the worker is asked for pixels from its next frame. That
    /// decision is what a wrong adapter costs at `OpenSharedHandle`, which the
    /// spike measured, without the call.
    ///
    /// Ignored because it makes a real Direct3D 12 device.
    #[test]
    #[ignore = "makes a Direct3D 12 device"]
    fn a_picture_from_another_adapter_is_refused_before_it_is_opened() {
        install().expect("the window's own device");
        let luid = PRESENT.with_borrow(|present| present.as_ref().expect("just made").luid);
        let frames = Arc::new(Frames::create(4, 2).unwrap());
        let path = frames.path().unwrap();
        let mut writer = Writer::open(&path).unwrap();

        let drawn = Drawn {
            luid: !luid,
            fence: 1,
            texture: 1,
            fence_value: 1,
            width: 4,
            height: 2,
        };
        let slot = writer.claim().expect("an empty slot");
        assert!(
            writer.publish_into(slot, &drawn),
            "the UI hasn't said what it can show"
        );
        let picture = frames
            .take_frame(|frame| match frame {
                Frame::Gpu(picture) => Some(picture),
                Frame::Pixels { .. } => None,
            })
            .flatten()
            .expect("the picture published");
        assert_eq!(picture.luid, !luid);
        assert!(
            shared(&frames, picture).is_none(),
            "a picture from another adapter can't be shown"
        );
        let slot = writer.claim().expect("the slot the reader left");
        assert!(
            !writer.publish_into(slot, &drawn),
            "the stream is asked for pixels from its next frame"
        );
    }
}
