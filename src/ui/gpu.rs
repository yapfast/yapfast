//! Showing a stream's picture without copying it.
//!
//! A worker that can draw a frame on the GPU publishes the texture it drew it
//! in, shared with this process, in the frames file's header. The UI opens that
//! texture on the device Skia draws the window with and hands Slint an `Image`
//! of it, so what shows is the worker's own memory: nothing crosses the CPU.
//! Copying one 720p picture costs 1.1-1.9 ms, 3-6 % of a core per stream at
//! 30 fps; showing it this way costs neither.
//!
//! Two things the spike in `.local/spike/gpu-present` found shape the code. A shared
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
//!
//! A slot is the worker's to draw into again only when this process says it has
//! finished with the picture that was there, one slot and generation at a time.
//! Taking a picture is not finishing with it: the window shows it until the
//! next one replaces it, and the GPU may still be sampling its texture after
//! that, so a lease is retired only once the window's own queue has drawn a
//! frame without it. That is what stops a worker drawing over a picture that is
//! on the screen, or one the renderer hasn't finished reading.

use std::sync::Arc;

use yapfast_core::frames::{Frame, Frames, Picture};

#[cfg(windows)]
use std::{
    cell::RefCell,
    collections::HashMap,
    collections::VecDeque,
    ffi::c_void,
    sync::Weak,
    time::{Duration, Instant},
};

#[cfg(windows)]
use anyhow::{Context as _, Result};
#[cfg(windows)]
use slint::wgpu_30::wgpu;
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
    // Every tick, whether or not this stream has a picture now: a stream the
    // UI let go of, or one it refused, must not keep its pictures, its fence or
    // its slots alive until some other stream draws one.
    #[cfg(windows)]
    PRESENT.with_borrow_mut(|present| {
        if let Some(present) = present.as_mut() {
            present.housekeeping();
        }
    });
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

/// Lets go of what the UI opened for a stream that has stopped: its textures,
/// the fence, and the slots the worker may have been holding for it. A stream
/// that is only hidden keeps them, since it shows its pictures again next tick.
///
/// `key` is the address of the frames the stream read, which is what the entry
/// is keyed by; the entry says itself whether that address is still its own, so
/// a key that outlived its frames removes nothing.
#[cfg(windows)]
pub fn forget(key: usize) {
    PRESENT.with_borrow_mut(|present| {
        let Some(present) = present.as_mut() else {
            return;
        };
        present.housekeeping();
        if present
            .streams
            .get(&key)
            .is_some_and(|stream| stream.frames.as_ptr() as usize == key)
        {
            present.streams.remove(&key);
        }
    });
}

/// Without Direct3D there is nothing opened for a stream to let go of.
#[cfg(not(windows))]
pub fn forget(_key: usize) {}

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

/// How many pictures one stream may hold a slot for at once: the one on the
/// screen, and the one before it, which the window's queue may still be drawing
/// when the next arrives. A third would leave the worker no free slot of its
/// three, so the picture is passed over instead.
#[cfg(windows)]
const LEASES: usize = 2;

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
    /// The number of the set of textures and fence the pictures are of, as the
    /// last one named it; zero until one does. A picture that names another set
    /// is the worker's textures rebuilt, and what was opened for the older set
    /// goes with it: a handle's value can't say that, since Windows hands a
    /// closed value out again.
    set: u64,
    /// The textures opened, by the set and the handle the worker published them
    /// under: it draws in one texture per slot, so each is opened once and
    /// shown for as long as it holds pictures of that set.
    textures: Vec<Opened>,
    /// The fence the stream's pictures are drawn behind, and the set and handle
    /// it was opened from: a worker that makes another set is followed.
    fence: Option<(u64, u64, ID3D12Fence)>,
    /// How many pictures in a row arrived before the GPU had drawn them.
    late: u32,
    /// The pictures this window has taken and not yet finished with, oldest
    /// first: the worker may draw into those slots again only as they leave
    /// this queue. The newest is the picture on the screen.
    leases: VecDeque<Lease>,
    /// Whether the UI has refused this stream's pictures: what was opened for
    /// it is let go with it, rather than waiting for another stream to draw a
    /// picture.
    refused: bool,
}

/// A picture the window has taken: the slot it is in, and which picture of
/// that slot it is. The worker keeps the slot for as long as this is unfinished
/// with.
#[cfg(windows)]
#[derive(Clone, Copy)]
struct Lease {
    slot: u32,
    generation: u64,
}

#[cfg(windows)]
struct Opened {
    /// The set the texture was opened for, and the handle it was opened from:
    /// the two together are what a picture names it by, since the handle was
    /// closed as soon as the resource was open and Windows gives a value out
    /// again once it is.
    set: u64,
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
        // A stream the UI let go of leaves its address to the next one, which
        // must not inherit what was opened for it.
        self.housekeeping();
        let device = &self.device;
        let streams = &mut self.streams;
        let key = Arc::as_ptr(frames) as usize;
        let stream = streams.entry(key).or_insert_with(|| Stream::new(frames));
        if !std::ptr::eq(stream.frames.as_ptr(), Arc::as_ptr(frames)) {
            *stream = Stream::new(frames);
        }
        // A picture of a set this entry hasn't opened is the worker's textures
        // rebuilt: the three textures and the fence opened for the older set are
        // let go, and the new one is opened from this picture on. The number is
        // what says so and the handles can't, because Windows hands a closed
        // handle's value out again.
        //
        // A set numbered before the one already open is a picture read from
        // before the worker rebuilt them: what was opened for that set was let
        // go when this one was, and its handle was closed as it was opened, so
        // it is passed over and the picture on the screen stays. Opening it
        // again would be opening a number that may be somebody else's by now.
        if picture.set < stream.set {
            return None;
        }
        if picture.set > stream.set {
            stream.reopen(picture.set);
        }
        // Slots the window's queue has finished with are the worker's again,
        // and this is where it hears it: a picture taken is on the screen until
        // the next one replaces it and that frame has been drawn.
        stream.retire_drawn(device, frames);
        if stream.leases.len() >= LEASES {
            // The queue is still drawing pictures this window has replaced: it
            // is asked for the rest of what it was given, briefly, rather than
            // passing over every picture from here on.
            wait_drawn(device);
            stream.retire_drawn(device, frames);
            if stream.leases.len() >= LEASES {
                // A queue that is behind by two pictures keeps the one on the
                // screen and drops this one: a torn picture is worse than a
                // slow one, and the worker has a slot of its own to draw the
                // next in.
                return None;
            }
        }
        let texture = match stream.texture(device, &picture) {
            Ok(texture) => texture,
            Err(error) => {
                stream.refused = true;
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
                stream.refused = true;
                refuse(
                    frames,
                    format!("{} pictures in a row were not drawn yet", stream.late),
                    "a stream publishes pictures its fence hasn't reached",
                );
                return None;
            }
            Err(error) => {
                stream.refused = true;
                refuse(frames, error, "could not open a stream's fence");
                return None;
            }
        }
        match slint::Image::try_from(texture) {
            Ok(image) => {
                // From here the worker knows which adapter to draw for, and
                // that its pictures are shown rather than copied.
                frames.accept_gpu(luid);
                stream.leases.push_back(Lease {
                    slot: picture.slot,
                    generation: picture.generation,
                });
                Some(image)
            }
            Err(error) => {
                stream.refused = true;
                refuse(
                    frames,
                    format!("{error:?}"),
                    "Slint can't show a stream's shared texture",
                );
                None
            }
        }
    }

    /// Lets go of what no stream is using any more: a stream the UI let go of,
    /// and one whose pictures it refused. A stopped stream keeps the UI's
    /// textures, its fence handle and the worker's slots for as long as a
    /// window keeps the entry, which nothing else would ever revisit.
    fn housekeeping(&mut self) {
        self.streams
            .retain(|_, stream| !stream.refused && stream.frames.strong_count() > 0);
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
    fn new(frames: &Arc<Frames>) -> Self {
        Self {
            frames: Arc::downgrade(frames),
            set: 0,
            textures: Vec::new(),
            fence: None,
            late: 0,
            leases: VecDeque::new(),
            refused: false,
        }
    }

    /// Starts on another set of the worker's textures: what was opened for the
    /// set before is let go, and the new one is opened from the picture that
    /// named it. A picture already on the screen is a lease like any other, so
    /// the slot it is in is still the worker's only once this window's queue has
    /// drawn a frame without it.
    fn reopen(&mut self, set: u64) {
        self.set = set;
        self.textures.clear();
        self.fence = None;
    }

    /// Retires the pictures the window's queue has drawn a frame without: the
    /// worker may draw into those slots again. The newest lease is the picture
    /// on the screen and is never retired here, and the ones before it are
    /// retired only once the queue is empty, since until then the GPU may still
    /// be sampling their textures.
    fn retire_drawn(&mut self, device: &wgpu::Device, frames: &Arc<Frames>) {
        while self.leases.len() > 1 && queue_empty(device) {
            let Some(lease) = self.leases.pop_front() else {
                break;
            };
            frames.retire(lease.slot, lease.generation);
        }
    }

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
            .find(|opened| (opened.set, opened.handle) == (picture.set, picture.texture))
        {
            return Ok(opened.texture.clone());
        }
        let texture = unsafe { open(device, picture) }?;
        self.textures.push(Opened {
            set: picture.set,
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
            .is_none_or(|(set, handle, _)| (*set, *handle) != (picture.set, picture.fence))
        {
            let fence = unsafe { open_fence(device, picture.fence) }?;
            self.fence = Some((picture.set, picture.fence, fence));
        }
        let Some((_, _, fence)) = &self.fence else {
            return Ok(false);
        };
        Ok(wait(event, fence, picture.fence_value))
    }
}

/// Opens the resource a stream's shared handle names, on the window's device, and
/// closes the handle.
///
/// `OpenSharedHandle` is not a transfer: it opens the object the handle names and
/// leaves the handle itself in this process's table, where nothing would ever
/// close it — the worker's set releases only its own references, which are what
/// keep the object alive over there. The object that comes back holds the
/// resource, so the handle has done its work the moment the open answers, and it
/// goes whether the open answered or not: a handle kept after it would outlive
/// the stream, and a handle opened twice is a handle whose value is nobody's.
#[cfg(windows)]
fn open_shared<T: windows::core::Interface>(
    device: &wgpu::Device,
    handle: u64,
    what: &str,
) -> std::result::Result<T, String> {
    let hal = unsafe { device.as_hal::<wgpu::hal::dx12::Api>() }
        .ok_or("the window's device is not Direct3D 12")?;
    let mut opened: Option<T> = None;
    shared_handles::opened();
    // SAFETY: the handle is one the worker duplicated into this process, and the
    // device is the one it was shared for.
    let status = unsafe {
        hal.raw_device()
            .OpenSharedHandle(HANDLE(handle as *mut c_void), &mut opened)
    };
    // SAFETY: the handle is this process's, and nothing has closed it yet.
    unsafe {
        let _ = CloseHandle(HANDLE(handle as *mut c_void));
    }
    shared_handles::closed();
    status.map_err(|error| format!("could not open the shared {what}: {error}"))?;
    opened.ok_or_else(|| format!("the shared {what} handle opened nothing"))
}

/// What the importer opened out of streams' shared sets, and what it has closed
/// again.
///
/// Windows answers no cheap count of a process's handles, so a test's only
/// evidence that an import closes what it opened is a count kept where the two
/// happen. Two stores per imported handle, which is once per set and not once
/// per picture.
#[cfg(windows)]
mod shared_handles {
    use std::sync::atomic::{AtomicU64, Ordering};

    static OPENED: AtomicU64 = AtomicU64::new(0);
    static CLOSED: AtomicU64 = AtomicU64::new(0);

    /// Says a stream's shared handle is about to be opened.
    pub fn opened() {
        OPENED.fetch_add(1, Ordering::Relaxed);
    }

    /// Says that handle is closed, which every open does, worked or not.
    pub fn closed() {
        CLOSED.fetch_add(1, Ordering::Relaxed);
    }

    /// How many handles were opened and how many closed so far, for a test.
    #[cfg(test)]
    pub fn counted() -> (u64, u64) {
        (
            OPENED.load(Ordering::Relaxed),
            CLOSED.load(Ordering::Relaxed),
        )
    }
}

/// Opens the texture a picture is in, on the window's device: what comes back
/// is the worker's own memory, with no pixel of it copied anywhere. Slint shows
/// it by asking Skia for a surface over the same Direct3D 12 resource, which is
/// the hop the spike proved.
///
/// The texture is imported as the resource says it is rather than as this side
/// hoped: the description is read from the resource itself, because Direct3D 12
/// takes the size and the format given to `texture_from_raw` on trust, and a
/// texture declared RGBA where the producer draws BGRA is a picture read through
/// the wrong channels.
///
/// Safety: the handle must be a shared texture the worker duplicated into this
/// process and this process hasn't opened or closed before: it is closed here,
/// as the resource it names is opened, and a handle opened twice is one whose
/// value may be somebody else's by the second time.
#[cfg(windows)]
unsafe fn open(
    device: &wgpu::Device,
    picture: &Picture,
) -> std::result::Result<wgpu::Texture, String> {
    let resource: ID3D12Resource = open_shared(device, picture.texture, "texture")?;
    // SAFETY: the resource is open on the window's device for as long as it is
    // held here, and reading its description is all this asks of it.
    let described = unsafe { resource.GetDesc() };
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

/// The wgpu format a shared picture is imported with, as the resource describes
/// itself, or why this path can't show it.
///
/// wgpu is told what the resource is and not what the producer meant to make:
/// the producer draws into `DXGI_FORMAT_B8G8R8A8_UNORM`, so the texture is
/// imported as `Bgra8Unorm`, and a picture declared RGBA where the pixels are
/// BGRA is a colour nobody drew.
///
/// Everything else Direct3D 12 would take on trust is checked here too, because
/// a raw resource is not validated: a texture of another size is read past its
/// end or short of it, one with more mips or samples than the picture is read
/// wrongly, and one that isn't 2D is not a picture at all.
#[cfg(windows)]
fn shared_format(
    described: &D3D12_RESOURCE_DESC,
    width: u32,
    height: u32,
) -> std::result::Result<wgpu::TextureFormat, String> {
    if described.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D {
        return Err(format!(
            "the shared resource is dimension {}, and a picture is a 2D texture",
            described.Dimension.0
        ));
    }
    if described.DepthOrArraySize != 1 {
        return Err(format!(
            "the shared texture has {} slices, not one",
            described.DepthOrArraySize
        ));
    }
    if described.MipLevels != 1 {
        return Err(format!(
            "the shared texture has {} mip levels, not one",
            described.MipLevels
        ));
    }
    if described.SampleDesc.Count != 1 {
        return Err(format!(
            "the shared texture has {} samples, not one",
            described.SampleDesc.Count
        ));
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

/// Opens the fence a stream's pictures are drawn behind, and closes the handle.
///
/// Safety: the handle must be a shared fence the worker duplicated into this
/// process, and one this process hasn't opened or closed before: it is closed
/// here.
#[cfg(windows)]
unsafe fn open_fence(
    device: &wgpu::Device,
    handle: u64,
) -> std::result::Result<ID3D12Fence, String> {
    open_shared(device, handle, "fence")
}

/// Waits for a picture to be drawn, and says whether it was.
///
/// The fence is the whole guarantee: a worker draws into a texture this process
/// holds, publishes it, and only its fence value says the GPU finished the
/// drawing. A picture a frame behind its fence is waited for here, and one that
/// doesn't reach its fence in time is passed over rather than shown torn.
///
/// The event is only a way to sleep until something may have happened: one
/// auto-reset event is reused by every stream, and a wait that timed out leaves
/// its notification registered, so a wake can be another picture's — or this
/// fence's, from a value before the one asked for. The fence's own value is
/// therefore read after every wake, and only that is an answer.
#[cfg(windows)]
fn wait(event: HANDLE, fence: &ID3D12Fence, value: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(u64::from(WAIT_MS));
    loop {
        match drawn(fence, value) {
            Some(drawn) => return drawn,
            None => {}
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            // The deadline is the original one, however many times a wake that
            // wasn't this picture's cut the wait short.
            return drawn(fence, value).unwrap_or(false);
        }
        unsafe {
            // An event already set would make this wait return at once, and a
            // notification left over from a wait that timed out is not this
            // picture's.
            let _ = ResetEvent(event);
            if fence.SetEventOnCompletion(value, event).is_err() {
                return false;
            }
            let millis = u32::try_from(left.as_millis()).unwrap_or(u32::MAX).max(1);
            let _ = WaitForSingleObject(event, millis);
        }
    }
}

/// Whether the fence has reached `value`, as the fence answers it: `None` while
/// it hasn't yet.
#[cfg(windows)]
fn drawn(fence: &ID3D12Fence, value: u64) -> Option<bool> {
    // SAFETY: the fence is open on the window's device for as long as this
    // process asks it anything.
    let reached = unsafe { fence.GetCompletedValue() };
    match reached {
        // Direct3D says a device that has been removed answers with the largest
        // value there is, which is not a picture drawn: the window shows its
        // pixels instead until another device is made.
        u64::MAX => Some(false),
        reached if reached >= value => Some(true),
        _ => None,
    }
}

/// Whether the window's queue has drawn everything it was given: until it has,
/// the GPU may still be sampling a texture the worker would draw into again.
#[cfg(windows)]
fn queue_empty(device: &wgpu::Device) -> bool {
    match device.poll(wgpu::PollType::Poll) {
        Ok(status) => status.is_queue_empty(),
        // A device that can't be asked about its queue is not one to hand a
        // slot back on: the picture before stays up.
        Err(_) => false,
    }
}

/// Asks the window's queue for the rest of what it was given, waiting a bounded
/// moment for it, so that a slot is never kept because a queue never empties.
#[cfg(windows)]
fn wait_drawn(device: &wgpu::Device) {
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_millis(u64::from(WAIT_MS))),
    });
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;
    use windows::Win32::Graphics::{
        Direct3D12::D3D12_RESOURCE_DIMENSION_TEXTURE1D,
        Dxgi::Common::{DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_SAMPLE_DESC},
    };
    use yapfast_core::frames::{Drawn, SharedTexture, Writer};

    /// A fence answers with the largest value there is once its device has been
    /// removed, which is not a picture drawn: showing pixels is the fallback,
    /// not a picture read from a device that is gone.
    #[test]
    fn a_removed_device_is_not_a_drawn_picture() {
        // What `drawn` asks of the fence, without a device to remove: the two
        // answers the fence gives, and the one that must not count as one.
        let answer = |reached: u64, value: u64| {
            if reached == u64::MAX {
                Some(false)
            } else if reached >= value {
                Some(true)
            } else {
                None
            }
        };
        assert_eq!(answer(u64::MAX, 7), Some(false), "device removed");
        assert_eq!(answer(7, 7), Some(true));
        assert_eq!(answer(8, 7), Some(true), "a value past the one asked for");
        assert_eq!(answer(6, 7), None, "still drawing it");
    }

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
            texture: SharedTexture { handle: 1, set: 1 },
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

    /// A description of a shared texture as `ID3D12Resource::GetDesc` answers for
    /// one of the producer's: `DXGI_FORMAT_B8G8R8A8_UNORM`, one mip, one sample,
    /// 640 by 360. Every test below changes one thing about it.
    fn described() -> D3D12_RESOURCE_DESC {
        D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: 640,
            Height: 360,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..D3D12_RESOURCE_DESC::default()
        }
    }

    /// A picture is imported as the resource says it is, not as this side used to
    /// assume: the producer draws BGRA, so wgpu is told BGRA, and RGBA is imported
    /// as RGBA because that is the other format this path could be handed.
    #[test]
    fn a_shared_picture_is_imported_in_the_format_its_resource_has() {
        assert_eq!(
            shared_format(&described(), 640, 360),
            Ok(wgpu::TextureFormat::Bgra8Unorm),
            "the producer draws BGRA"
        );
        let rgba = D3D12_RESOURCE_DESC {
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            ..described()
        };
        assert_eq!(
            shared_format(&rgba, 640, 360),
            Ok(wgpu::TextureFormat::Rgba8Unorm)
        );
    }

    /// Everything else the raw import takes on trust is checked, and the reason
    /// says what was wrong: a texture Direct3D 12 is told the wrong size, mips,
    /// samples or format of is read wrongly or past its end, and nothing here
    /// would say so.
    #[test]
    fn a_resource_that_is_not_the_picture_is_refused_in_words() {
        let refused = |described: D3D12_RESOURCE_DESC, width: u32, height: u32, said: &str| {
            let error = shared_format(&described, width, height)
                .expect_err("a resource that can't be shown");
            assert!(
                error.contains(said),
                "the reason \"{error}\" doesn't name {said:?}"
            );
        };
        refused(described(), 1280, 720, "640x360");
        refused(described(), 640, 720, "640x360");
        refused(
            D3D12_RESOURCE_DESC {
                MipLevels: 2,
                ..described()
            },
            640,
            360,
            "2 mip levels",
        );
        refused(
            D3D12_RESOURCE_DESC {
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 4,
                    Quality: 0,
                },
                ..described()
            },
            640,
            360,
            "4 samples",
        );
        refused(
            D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE1D,
                ..described()
            },
            640,
            360,
            "a picture is a 2D texture",
        );
        refused(
            D3D12_RESOURCE_DESC {
                DepthOrArraySize: 2,
                ..described()
            },
            640,
            360,
            "2 slices",
        );
        refused(
            D3D12_RESOURCE_DESC {
                // DXGI's ten-bit RGB, which no shader here reads.
                Format: DXGI_FORMAT_R10G10B10A2_UNORM,
                ..described()
            },
            640,
            360,
            "24",
        );
    }

    /// A real shared set, opened through this window's device: the picture is
    /// imported in the format the producer's own resource has, and the handles the
    /// import was given are closed as the resources they name are opened.
    ///
    /// Windows answers no cheap count of this process's handles, so the evidence
    /// is the count the importer keeps: what it opened and what it closed are the
    /// same number. The delta is taken here rather than the total, so a test that
    /// ran before it says nothing, and the two are only equal if this test opened
    /// nothing it left open.
    ///
    /// Ignored because it makes a Direct3D 12 device and an NVENC session.
    #[test]
    #[ignore = "makes a Direct3D 12 device and opens an NVENC session"]
    fn a_shared_set_is_imported_as_its_resource_and_leaves_no_handle_open() {
        use yapfast_core::{
            d3d11::{D3d11Encoder, Shared},
            yapfast_discord::rtp::Codec,
        };

        install().expect("the window's own device");
        let device =
            PRESENT.with_borrow(|present| present.as_ref().expect("just made").device.clone());
        let (width, height) = (64u32, 36u32);
        let mut encoder = D3d11Encoder::new(Codec::H264, 1920, 1080, 60, 8000)
            .expect("an NVENC encoder on this machine");
        let source = encoder.source(1).expect("a texture to draw on");
        // This process stands in for the UI's: DuplicateHandle answers it as
        // readily as another one, and a handle is a handle whoever holds it.
        let mut shared = Shared::new(&encoder, width, height, std::process::id())
            .expect("a picture shared with this process");
        let (first, _) = shared
            .draw(0, &mut encoder, source.texture())
            .expect("the first picture drawn");
        let (second, fence_value) = shared
            .draw(1, &mut encoder, source.texture())
            .expect("the second picture drawn");

        // The resource's own description, read here rather than taken from the
        // header: this is what the import has to agree with.
        let resource: ID3D12Resource =
            open_shared(&device, first.handle, "texture").expect("a shared texture opened");
        let described = unsafe { resource.GetDesc() };
        drop(resource);
        assert_eq!(
            described.Format, DXGI_FORMAT_B8G8R8A8_UNORM,
            "the producer no longer draws BGRA, and the import has to follow it"
        );

        let picture = Picture {
            luid: shared.luid(),
            fence: shared.fence(),
            texture: second.handle,
            set: second.set,
            fence_value,
            generation: 1,
            slot: 1,
            width,
            height,
        };
        let (before_opened, before_closed) = shared_handles::counted();
        let shown = unsafe { open(&device, &picture) }.expect("the picture imported");
        assert_eq!(
            shown.format(),
            shared_format(&described, width, height).expect("a texture this path shows"),
            "the imported texture is declared as the resource is"
        );
        assert_eq!(shown.format(), wgpu::TextureFormat::Bgra8Unorm);
        let fence = unsafe { open_fence(&device, picture.fence) }.expect("the fence opened");
        drop(fence);
        let (opened, closed) = shared_handles::counted();
        let (opened, closed) = (opened - before_opened, closed - before_closed);
        assert_eq!(opened, 2, "a texture and a fence");
        assert_eq!(
            closed, opened,
            "the importer opened a handle it didn't close"
        );

        drop(shown);
        // The set goes as a stopped stream's does: the handles it duplicated
        // here and nothing opened are closed in this process by the worker's side.
        drop(shared);
        eprintln!("two {width}x{height} pictures at {opened} handles opened, {closed} closed");
    }
}
