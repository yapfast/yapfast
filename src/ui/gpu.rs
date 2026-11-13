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
//! On Linux a picture is a DMA-BUF rather than a shared texture, so it arrives
//! as descriptors over the stream's socket (`yapfast_core::dmabuf`) instead of
//! as a handle in the frames header. Importing one is the window's Vulkan
//! device taking those descriptors as external memory: the `VkImage` is made
//! with `VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT` and the picture's
//! fourcc, modifier, plane offsets and strides, one `VkDeviceMemory` is
//! allocated per buffer the planes are in, and wgpu wraps the image as a
//! texture (`wgpu::hal::vulkan::Device::texture_from_raw`), which Skia shows.
//! The build that draws on Vulkan is the opt-in `skia-vulkan` one; every other
//! build imports nothing and a stream that sends pictures is asked for pixels.
//!
//! A DMA-BUF picture is not always in a format Skia can sample: the vendored
//! renderer imports a single-plane 8-bit RGBA (or BGRA) Vulkan image, and the
//! card's decoder exports NV12, two planes, one luma and one interleaved
//! chroma. A picture of that shape goes through a pass of its own first, which
//! samples the two planes and writes the tile in BGRA: the card's own memory is
//! read where it lies, and what Skia shows is a texture the size of the tile.
//! Nothing of either path crosses the processor.
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
    cell::RefCell, collections::HashMap, collections::VecDeque, ffi::c_void, sync::Weak,
    time::Instant,
};

#[cfg(windows)]
use anyhow::{Context as _, Result};
// The window's device and its queue are wgpu's on both platforms that show a
// picture where it lies: Direct3D 12 behind them on Windows, and Vulkan on
// Linux, where the queue is asked whether it has drawn everything in the same
// way.
#[cfg(any(windows, all(target_os = "linux", feature = "skia-vulkan")))]
use slint::wgpu_30::wgpu;
#[cfg(any(windows, all(target_os = "linux", feature = "skia-vulkan")))]
use std::time::Duration;
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
    // The same on Linux, for the pictures this window imported: a stream the UI
    // let go of must not keep them, and a stream whose pictures were refused
    // keeps none.
    #[cfg(all(target_os = "linux", feature = "skia-vulkan"))]
    present::housekeeping();
    let shown = frames.take_frame(Taken::of);
    match shown? {
        Taken::Pixels(image) => Some(image),
        Taken::Shared(picture) => shared(frames, picture),
        #[cfg(target_os = "linux")]
        Taken::Dmabuf(picture) => dmabuf(frames, picture),
    }
}

/// What a take of a stream's frames gave: the picture the window shows, or the
/// one it must be shown from, in the shape the platform hands it over in.
enum Taken {
    /// The worker's pixels, as Slint shows them.
    Pixels(slint::Image),
    /// A shared texture's handle, and the fence it is drawn behind.
    Shared(Picture),
    /// A DMA-BUF's descriptors, which are this process's from here.
    #[cfg(target_os = "linux")]
    Dmabuf(yapfast_core::dmabuf::Picture),
}

impl Taken {
    fn of(frame: Frame<'_>) -> Self {
        match frame {
            Frame::Pixels {
                width,
                height,
                pixels,
            } => Self::Pixels(copied(width, height, pixels)),
            Frame::Gpu(picture) => Self::Shared(picture),
            #[cfg(target_os = "linux")]
            Frame::Dmabuf(picture) => Self::Dmabuf(picture),
        }
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
///
/// A picture nothing shows is a slot the worker can have back at once: it is
/// not on the screen, so nothing samples its texture, and waiting for a
/// retirement that only a shown picture gets would leave the worker's slot
/// held — and, held long enough, end the card's picture path for the rest of
/// the stream.
#[cfg(windows)]
fn shared(frames: &Arc<Frames>, picture: Picture) -> Option<slint::Image> {
    let (slot, generation) = (picture.slot, picture.generation);
    let shown = PRESENT.with_borrow_mut(|present| match present.as_mut() {
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
    });
    if shown.is_none() {
        frames.retire(slot, generation);
    }
    shown
}

#[cfg(not(windows))]
fn shared(frames: &Arc<Frames>, picture: Picture) -> Option<slint::Image> {
    if frames.refuse_gpu() {
        tracing::warn!(
            "a stream draws on a shared texture, which Yapfast can open only on Windows; showing its pixels"
        );
    }
    // A picture this build can never show is one the worker's slot holds for
    // nothing.
    frames.retire(picture.slot, picture.generation);
    None
}

/// A picture the worker drew into a DMA-BUF, as the window shows it.
///
/// The picture is imported on the window's Vulkan device — the descriptors,
/// one for each of its planes, taken as external memory — and shown from the
/// texture that comes back. What can't be imported is refused once, with the
/// reason in the log, and the stream is asked for pixels from its next frame:
/// the picture before this one stays up rather than a blank tile, which is what
/// a stream that falls back costs.
///
/// Nothing shows a picture when the window draws without a Vulkan device, or in
/// a build that carries no importer: those are refusals, and a refusal retires
/// the slot at once — nothing samples a picture nothing showed.
#[cfg(all(target_os = "linux", feature = "skia-vulkan"))]
fn dmabuf(frames: &Arc<Frames>, picture: yapfast_core::dmabuf::Picture) -> Option<slint::Image> {
    present::show(frames, picture)
}

/// A build without Skia's Vulkan has no device to import a DMA-BUF on, so a
/// stream that publishes one is asked for pixels.
#[cfg(all(target_os = "linux", not(feature = "skia-vulkan")))]
fn dmabuf(frames: &Arc<Frames>, picture: yapfast_core::dmabuf::Picture) -> Option<slint::Image> {
    if frames.refuse_gpu() {
        tracing::warn!(
            planes = picture.plane_count(),
            fourcc = format!("{:08x}", picture.fourcc()),
            modifier = format!("{:x}", picture.modifier()),
            "this build draws without Vulkan, which is the device a DMA-BUF is imported on; showing the stream's pixels"
        );
    }
    // A picture nothing can show is one the worker's slot holds for nothing.
    frames.retire(picture.slot(), picture.generation());
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
/// `key` is what the stream's frames are known by — [`Frames::id`], not their
/// address, which the allocator hands out again to the next stream of the same
/// size.
#[cfg(windows)]
pub fn forget(key: u64) {
    PRESENT.with_borrow_mut(|present| {
        let Some(present) = present.as_mut() else {
            return;
        };
        present.housekeeping();
        present.streams.remove(&key);
    });
}

/// Without Direct3D and without a Vulkan device there is nothing opened for a
/// stream to let go of.
#[cfg(not(any(windows, all(target_os = "linux", feature = "skia-vulkan"))))]
pub fn forget(_key: u64) {}

/// Lets go of a stream's imported pictures: the textures and the memory behind
/// them, the slot each is still holding and the queue's own record of it. A
/// stream the UI stopped watching shows its pictures again next tick, so this is
/// for one that has ended, and for one whose pictures the UI refused.
#[cfg(all(target_os = "linux", feature = "skia-vulkan"))]
pub fn forget(key: u64) {
    present::forget(key);
}

/// Hands the window's Vulkan device to the importer, as Windows hands it the
/// Direct3D 12 device it made above: called by [`crate::ui::vulkan::install`],
/// the one place that device is made, so a picture is imported on the device
/// the window draws with and on no other.
#[cfg(all(target_os = "linux", feature = "skia-vulkan"))]
pub fn window_device(device: wgpu::Device, queue: wgpu::Queue) {
    present::install(device, queue);
}

/// Without Direct3D there is no shared texture to open, and a stream that
/// publishes one is asked for pixels.
#[cfg(not(windows))]
pub fn install() -> anyhow::Result<slint::wgpu_30::WGPUConfiguration> {
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
#[cfg(any(windows, all(target_os = "linux", feature = "skia-vulkan")))]
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
#[cfg(any(windows, all(target_os = "linux", feature = "skia-vulkan")))]
const LEASES: usize = 2;

/// How many pictures one set of the worker's shares, which is how many textures
/// one stream keeps open: `SHARED_PICTURES` in
/// `crates/yapfast-core/native/d3d11enc.c`, one per slot of the frames file.
///
/// Nothing opened for a picture is ever evicted to make room. The handle was
/// closed as the texture it named was opened, so a texture let go of here could
/// not be opened again: the next picture naming that handle would be refused.
/// A set that names more pictures than it shares is passed over instead, which
/// is a worker to show as pixels rather than one to make room for.
#[cfg(any(windows, test))]
const PICTURES: usize = 3;

/// What a stream's opened textures say about one more picture of the set they
/// are all of: a picture to open, or why it can't be. A set shares `PICTURES`
/// pictures and none of the ones open for it is ever let go of, since a handle
/// was closed as the texture it named was opened and could not be opened again.
/// A set naming one more is a picture to show as pixels, not one to make room
/// for by dropping a texture a slot may still be drawn into.
#[cfg(any(windows, test))]
fn opening(open: usize) -> std::result::Result<(), String> {
    if open < PICTURES {
        return Ok(());
    }
    Err(format!(
        "the worker named a {}th picture of a set that shares {PICTURES}",
        open + 1
    ))
}

/// The textures a stream keeps open are the pictures one set of the worker's
/// shares, and the worker's own header is what says how many that is. The two
/// numbers drifting apart is what let an eviction close a picture's handle, and
/// the picture naming it again could not be opened.
///
/// A Linux build can't open a shared texture, so this reads the number the
/// worker shares rather than holding one: the drift is the defect, and it is
/// the same number whether a device is there or not.
#[cfg(test)]
mod bound {
    use super::{PICTURES, opening};

    /// `SHARED_PICTURES` in the worker's own source, as the number it is.
    fn shared_pictures() -> usize {
        include_str!("../../crates/yapfast-core/native/d3d11enc.c")
            .lines()
            .find_map(|line| line.trim().strip_prefix("#define SHARED_PICTURES "))
            .and_then(|count| count.trim().parse().ok())
            .expect("SHARED_PICTURES in d3d11enc.c")
    }

    /// Every picture of a set is opened and kept, and one the set doesn't share
    /// is passed over rather than bought by letting go of a texture a slot may
    /// still be drawn into: the handle of one let go of here is already closed.
    #[test]
    fn a_set_is_held_whole_and_never_evicted_to_make_room() {
        assert_eq!(
            PICTURES,
            shared_pictures(),
            "a stream keeps open what the worker shares, and no fewer"
        );
        for open in 0..PICTURES {
            assert!(opening(open).is_ok(), "picture {} of a set", open + 1);
        }
        let refused = opening(PICTURES).expect_err("one picture more than a set shares");
        assert!(
            refused.contains(&format!("{}", PICTURES + 1)),
            "the refusal names the picture it is: {refused}"
        );
    }
}

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
    /// One entry per stream, by what its frames are known by: an address is
    /// handed out again as soon as the allocator makes the next one.
    streams: HashMap<u64, Stream>,
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
        // A stream the UI let go of has left an entry that nothing may inherit.
        self.housekeeping();
        let device = &self.device;
        let streams = &mut self.streams;
        let stream = streams
            .entry(frames.id())
            .or_insert_with(|| Stream::new(frames));
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
    /// window keeps the entry, which nothing else would ever revisit. And it
    /// gives the worker back the slots this window's queue has finished with,
    /// every tick rather than only when another picture replaces one: a worker
    /// whose slot the window still holds has no picture to hand it, so an answer
    /// that came only with a take would wait for the picture it is needed for.
    fn housekeeping(&mut self) {
        let Present {
            device, streams, ..
        } = self;
        streams.retain(|_, stream| !stream.refused && stream.frames.strong_count() > 0);
        for stream in streams.values_mut() {
            if let Some(frames) = stream.frames.upgrade() {
                stream.retire_drawn(device, &frames);
            }
        }
    }
}

/// Asks a stream for pixels from its next frame, and logs why the first time:
/// a stream decides once, because what it depends on — the pair of adapters,
/// the renderer — doesn't change while the stream lasts.
#[cfg(any(windows, all(target_os = "linux", feature = "skia-vulkan")))]
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
        opening(self.textures.len())?;
        let texture = unsafe { open(device, picture) }?;
        self.textures.push(Opened {
            set: picture.set,
            handle: picture.texture,
            texture: texture.clone(),
        });
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

#[cfg(windows)]
/// Opens the fence a stream's pictures are drawn behind, and closes the handle.
///
/// # Safety
///
/// The handle must be a shared fence the worker duplicated into this process,
/// and one this process hasn't opened or closed before: it is closed here.
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
        if let Some(drawn) = drawn(fence, value) {
            return drawn;
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
#[cfg(any(windows, all(target_os = "linux", feature = "skia-vulkan")))]
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
#[cfg(any(windows, all(target_os = "linux", feature = "skia-vulkan")))]
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
        let _ = writer.claim().expect("an empty slot");
        assert!(
            writer.publish_into(&drawn),
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
        let _ = writer.claim().expect("the slot the reader left");
        assert!(
            !writer.publish_into(&drawn),
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

/// How a DMA-BUF picture's descriptors become a texture: the shape the fourcc
/// names, the format that shape is imported as, and where each plane starts in
/// the memory it shares.
///
/// This is the part of the import that needs no card, so it is a function of the
/// picture's own numbers and is tested without one. What a driver takes on
/// trust is read here, once: an image declared a shape its memory isn't is a
/// picture shown through the wrong channels, and one whose planes are placed
/// wrongly reads past what the worker shared.
#[cfg(target_os = "linux")]
// Without the Vulkan feature nothing here imports a picture, but the reading
// of a descriptor and its tests are worth keeping in every Linux build: they
// are the part that needs no card.
#[cfg_attr(not(feature = "skia-vulkan"), allow(dead_code))]
mod plane {
    use std::os::fd::RawFd;

    use ash::vk;
    use slint::wgpu_30::wgpu;
    use yapfast_core::dmabuf::{Colour, MAX_PLANES, Picture, Plane};

    /// `DRM_FORMAT_MOD_LINEAR`: the memory is the picture's rows one after
    /// another, and nothing else about it needs saying. A picture the tests make
    /// is the one that names it; an imported one carries whatever the compositor
    /// said.
    #[cfg(test)]
    pub const MOD_LINEAR: u64 = 0;

    /// `DRM_FORMAT_MOD_INVALID`: the exporter didn't say how the memory is laid
    /// out. A tiling nobody named can't be imported as any of them, and a
    /// picture guessed at is a picture read wrongly, so it is refused.
    pub const MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

    /// `DRM_FORMAT_*`'s own packing of a fourcc into a number: the four
    /// characters, the first in the low byte.
    pub const fn fourcc(name: [u8; 4]) -> u32 {
        (name[0] as u32)
            | ((name[1] as u32) << 8)
            | ((name[2] as u32) << 16)
            | ((name[3] as u32) << 24)
    }

    /// What a picture's memory holds, as far as this importer can show it: the
    /// shapes a fourcc names that a texture can be made of here, and what that
    /// texture is to Vulkan and to wgpu.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Shape {
        /// Bytes R, G, B, A: `DRM_FORMAT_ABGR8888`.
        Rgba,
        /// Bytes B, G, R, A: `DRM_FORMAT_ARGB8888`, which is what a card's
        /// buffer of 32-bit pixels usually is.
        Bgra,
        /// A plane of luma and one of interleaved chroma: `DRM_FORMAT_NV12`,
        /// 8-bit 4:2:0, which is what the card's decoder exports for an 8-bit
        /// stream.
        Nv12,
        /// The same planes at 10 bits, in 16-bit samples with the 10 at their
        /// top: `DRM_FORMAT_P010`, which is what VA-API's decoder exports.
        P010,
        /// The same planes at 10 bits, in 16-bit samples with the 10 at their
        /// bottom: `DRM_FORMAT_P016`, which is what NVDEC decodes into.
        P016,
    }

    impl Shape {
        /// `DRM_FORMAT_ABGR8888`, whose bytes are R, G, B, A.
        pub const ABGR8888: u32 = fourcc(*b"AB24");
        /// `DRM_FORMAT_ARGB8888`, whose bytes are B, G, R, A.
        pub const ARGB8888: u32 = fourcc(*b"AR24");
        /// `DRM_FORMAT_NV12`.
        pub const NV12: u32 = fourcc(*b"NV12");
        /// `DRM_FORMAT_P010`, which is the fourcc VA-API's decoder exports.
        /// Named apart from the variant, which a `match` on the fourcc would
        /// otherwise read as one.
        pub const P010_FOURCC: u32 = fourcc(*b"P010");
        /// The fourcc NVDEC's P016 is carried as: 16-bit samples with the 10
        /// bits at their bottom. The kernel names the same four characters for
        /// its own 16-bit format, and the shims are the only ones that write
        /// this one, so the depth travels with it — [`Self::depth`].
        pub const P016_FOURCC: u32 = fourcc(*b"P016");

        /// The shape the fourcc names, or the reason this importer can't show
        /// a picture of it. Only the shapes Skia or the tile pass can read are
        /// here: a 4:2:2 or a planar picture would be a texture of a format this
        /// renderer can't sample, and an import of it is a picture shown wrongly
        /// rather than not at all.
        pub fn of(fourcc: u32) -> Result<Self, String> {
            match fourcc {
                Self::ABGR8888 => Ok(Self::Rgba),
                Self::ARGB8888 => Ok(Self::Bgra),
                Self::NV12 => Ok(Self::Nv12),
                Self::P010_FOURCC => Ok(Self::P010),
                Self::P016_FOURCC => Ok(Self::P016),
                other => Err(format!(
                    "the picture is DRM format {other:08x} ({}), which this importer can't show",
                    spelled(other)
                )),
            }
        }

        /// What the shape is called in a log line or a refusal.
        pub fn named(self) -> &'static str {
            match self {
                Self::Rgba => "RGBA",
                Self::Bgra => "BGRA",
                Self::Nv12 => "NV12",
                Self::P010 => "P010",
                Self::P016 => "P016",
            }
        }

        /// How many planes the shape has, which is how many descriptors a
        /// picture of it carries.
        pub fn planes(self) -> usize {
            match self {
                Self::Rgba | Self::Bgra => 1,
                Self::Nv12 | Self::P010 | Self::P016 => 2,
            }
        }

        /// Whether the shape is one the tile pass reads, rather than one Skia
        /// samples as it is: luma and chroma planes, 8 or 10 bits a sample.
        pub fn tiled(self) -> bool {
            self.planes() == 2
        }

        /// Whether chroma is shared by each two pixels across and down, which
        /// leaves a picture of an odd side with nowhere to put its last sample.
        pub fn subsampled(self) -> bool {
            self.tiled()
        }

        /// The fewest bytes one row of `plane` takes in a picture `width` pixels
        /// wide: four to a pixel, or — for the 4:2:0 shapes, whose chroma is two
        /// bytes a pair — one to a pixel at 8 bits and two at 10.
        pub fn row(self, width: u32, plane: usize) -> u32 {
            match (self, plane) {
                (Self::Rgba | Self::Bgra, _) => width.saturating_mul(4),
                (Self::Nv12, _) => width,
                (Self::P010 | Self::P016, _) => width.saturating_mul(2),
            }
        }

        /// What the tile pass is told the samples are, which is how it reads a
        /// 16-bit word as the 10 bits in it: the number the shader's `signal.w`
        /// holds.
        pub fn depth(self) -> u32 {
            match self {
                Self::Rgba | Self::Bgra | Self::Nv12 => 0,
                Self::P010 => 1,
                Self::P016 => 2,
            }
        }

        /// The Vulkan format the image is made as: the one whose memory layout
        /// is the fourcc's, and the multi-plane one the card's decoder writes.
        pub fn vk(self) -> vk::Format {
            match self {
                Self::Rgba => vk::Format::R8G8B8A8_UNORM,
                Self::Bgra => vk::Format::B8G8R8A8_UNORM,
                Self::Nv12 => vk::Format::G8_B8R8_2PLANE_420_UNORM,
                Self::P010 => vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
                Self::P016 => vk::Format::G16_B16R16_2PLANE_420_UNORM,
            }
        }

        /// The wgpu format the texture is declared as: the same memory, under
        /// the name wgpu has for it. wgpu names one 16-bit 4:2:0 format and not
        /// two, and its plane views are 16-bit unsigned samples either way —
        /// wider than both P010 and P016 hold — so the tile pass reads the depth
        /// out of the words itself, from [`Self::depth`].
        pub fn wgpu(self) -> wgpu::TextureFormat {
            match self {
                Self::Rgba => wgpu::TextureFormat::Rgba8Unorm,
                Self::Bgra => wgpu::TextureFormat::Bgra8Unorm,
                Self::Nv12 => wgpu::TextureFormat::NV12,
                Self::P010 | Self::P016 => wgpu::TextureFormat::P010,
            }
        }

        /// The format one plane of the picture is viewed as, and which plane it
        /// is: one sample a pixel for luma, two for chroma, at the shape's own
        /// depth.
        pub fn view(self, plane: usize) -> (wgpu::TextureFormat, wgpu::TextureAspect) {
            match (self, plane) {
                (Self::Nv12, 0) => (wgpu::TextureFormat::R8Unorm, wgpu::TextureAspect::Plane0),
                (Self::Nv12, _) => (wgpu::TextureFormat::Rg8Unorm, wgpu::TextureAspect::Plane1),
                (Self::P010 | Self::P016, 0) => {
                    (wgpu::TextureFormat::R16Unorm, wgpu::TextureAspect::Plane0)
                }
                (Self::P010 | Self::P016, _) => {
                    (wgpu::TextureFormat::Rg16Unorm, wgpu::TextureAspect::Plane1)
                }
                (Self::Rgba | Self::Bgra, 0) => (self.wgpu(), wgpu::TextureAspect::All),
                (Self::Rgba | Self::Bgra, _) => (self.wgpu(), wgpu::TextureAspect::All),
            }
        }

        /// What the window asks of the texture it wraps the picture in. A
        /// picture Skia samples itself has to be renderable too, which is what
        /// `slint::Image` asks of a texture it is given; the pictures the tile
        /// pass reads are only ever read, by the pass that writes the tile.
        pub fn uses(self) -> wgpu::TextureUsages {
            match self {
                Self::Rgba | Self::Bgra => {
                    wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT
                }
                Self::Nv12 | Self::P010 | Self::P016 => wgpu::TextureUsages::TEXTURE_BINDING,
            }
        }
    }

    /// The fourcc's own characters, for a log line: what an exporter wrote is
    /// more use than the number it wrote it as.
    fn spelled(fourcc: u32) -> String {
        let bytes = fourcc.to_le_bytes();
        bytes
            .iter()
            .map(|byte| match byte {
                0x20..=0x7e => char::from(*byte),
                other => char::from(*other).escape_debug().next().unwrap_or('?'),
            })
            .collect()
    }

    /// One plane, placed in the memory its descriptor names: where it starts,
    /// how far apart its rows are, and which of the picture's buffers it is in.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Placed {
        pub offset: u32,
        pub stride: u32,
        /// Which of [`Plan::memories`] holds this plane. One memory is imported
        /// and bound for each buffer, so planes that share a buffer share a
        /// memory, and a picture whose planes are in two buffers is a disjoint
        /// image to Vulkan.
        pub memory: usize,
    }

    /// What the importer is to make of one picture.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Plan {
        pub width: u32,
        pub height: u32,
        /// The DRM modifier the memory was laid out with, which is part of what
        /// an image is created as and not something an importer may choose.
        pub modifier: u64,
        pub shape: Shape,
        /// The colour signal the picture was coded with, which the tile pass
        /// reads to show a 10-bit picture in its own colour.
        pub colour: Colour,
        /// Each plane of the shape, in its order.
        pub planes: [Option<Placed>; MAX_PLANES],
        /// Each buffer the planes are in, as the descriptor the picture names
        /// it by, which is the worker's and not this process's to close.
        pub memories: [Option<RawFd>; MAX_PLANES],
    }

    impl Plan {
        /// Each plane, in the shape's order.
        pub fn planes(&self) -> impl Iterator<Item = Placed> + '_ {
            self.planes.iter().flatten().copied()
        }

        /// The descriptor of each buffer the planes are in.
        pub fn memories(&self) -> impl Iterator<Item = RawFd> + '_ {
            self.memories.iter().flatten().copied()
        }

        /// Whether the planes are not all in one buffer, which is an image
        /// Vulkan binds one plane at a time (`VK_IMAGE_CREATE_DISJOINT_BIT`).
        pub fn disjoint(&self) -> bool {
            self.memories().nth(1).is_some()
        }
    }

    /// A picture's numbers, as the importer reads them: what
    /// `yapfast_core::dmabuf::Picture` says in the shape this module works on,
    /// so that a test describes one without a stream to take it from.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Numbers {
        pub width: u32,
        pub height: u32,
        pub fourcc: u32,
        pub modifier: u64,
        /// The range, matrix and curve the samples are in, as the worker's shim
        /// read them out of the stream.
        pub colour: Colour,
        /// Each plane's place and the descriptor it is in, in the order the
        /// picture names them, and `None` past the last.
        pub planes: [Option<(Plane, RawFd)>; MAX_PLANES],
    }

    impl Numbers {
        /// What a picture that arrived says, as this module reads it.
        pub fn of(picture: &Picture) -> Self {
            let mut planes = [None; MAX_PLANES];
            for (index, plane) in picture.planes().enumerate() {
                planes[index] = Some(plane);
            }
            Self {
                width: picture.width(),
                height: picture.height(),
                fourcc: picture.fourcc(),
                modifier: picture.modifier(),
                colour: picture.colour(),
                planes,
            }
        }
    }

    /// What importing `numbers` takes, or why the picture can't be imported.
    pub fn layout(numbers: &Numbers) -> Result<Plan, String> {
        let Numbers {
            width,
            height,
            fourcc,
            modifier,
            colour,
            planes,
        } = *numbers;
        if width == 0 || height == 0 {
            return Err(format!("the picture is {width} by {height}"));
        }
        let shape = Shape::of(fourcc)?;
        // Chroma is one sample for each two pixels across and down, so a picture
        // of an odd side has nowhere to put its last one: an importer told it is
        // 4:2:0 would read the row below as chroma.
        if shape.subsampled() && (!width.is_multiple_of(2) || !height.is_multiple_of(2)) {
            return Err(format!(
                "a {} picture of {width} by {height} has half a chroma sample at its edge",
                shape.named()
            ));
        }
        if modifier == MOD_INVALID {
            return Err(
                "the picture was exported without saying how its memory is laid out".to_string(),
            );
        }
        let described: Vec<(Plane, RawFd)> = planes.iter().flatten().copied().collect();
        if described.len() != shape.planes() {
            return Err(format!(
                "a {} picture has {} plane{}, and this one came with {}",
                shape.named(),
                shape.planes(),
                if shape.planes() == 1 { "" } else { "s" },
                described.len()
            ));
        }
        let mut placed: [Option<Placed>; MAX_PLANES] = [None; MAX_PLANES];
        let mut memories: [Option<RawFd>; MAX_PLANES] = [None; MAX_PLANES];
        let mut buffers = 0;
        for (index, (plane, fd)) in described.iter().enumerate() {
            let least = shape.row(width, index);
            if plane.stride < least {
                return Err(format!(
                    "plane {index} of the {} picture has rows of {} bytes, and {width} pixels take at least {least}",
                    shape.named(),
                    plane.stride
                ));
            }
            // Planes that are in one buffer are imported into one memory: the
            // kernel's reference is taken once for the buffer, and Vulkan is
            // asked for one allocation, which two planes of one image can
            // share.
            let memory = match memories.iter().flatten().position(|held| held == fd) {
                Some(memory) => memory,
                None => {
                    let memory = buffers;
                    memories[memory] = Some(*fd);
                    buffers += 1;
                    memory
                }
            };
            placed[index] = Some(Placed {
                offset: plane.offset,
                stride: plane.stride,
                memory,
            });
        }
        Ok(Plan {
            width,
            height,
            modifier,
            shape,
            colour,
            planes: placed,
            memories,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use yapfast_core::dmabuf::{Matrix, Range, Transfer};

        fn plane(offset: u32, stride: u32) -> (Plane, RawFd) {
            (Plane { offset, stride }, 7)
        }

        /// A two-plane NV12 picture of `width` by `height`, both planes in the
        /// one buffer, as the card's decoder exports it.
        fn nv12(width: u32, height: u32) -> Numbers {
            let mut planes = [None; MAX_PLANES];
            planes[0] = Some((
                Plane {
                    offset: 0,
                    stride: width,
                },
                7,
            ));
            planes[1] = Some((
                Plane {
                    offset: width * height,
                    stride: width,
                },
                7,
            ));
            Numbers {
                width,
                height,
                fourcc: Shape::NV12,
                modifier: MOD_LINEAR,
                colour: Colour::SDR,
                planes,
            }
        }

        /// The same picture at 10 bits: 16-bit samples, so its rows are twice as
        /// long and its chroma starts where two luma rows' bytes end.
        fn ten_bit(width: u32, height: u32, fourcc: u32) -> Numbers {
            let mut planes = [None; MAX_PLANES];
            planes[0] = Some((
                Plane {
                    offset: 0,
                    stride: width * 2,
                },
                7,
            ));
            planes[1] = Some((
                Plane {
                    offset: width * 2 * height,
                    stride: width * 2,
                },
                7,
            ));
            Numbers {
                width,
                height,
                fourcc,
                modifier: MOD_LINEAR,
                colour: Colour {
                    range: Range::Limited,
                    matrix: Matrix::Bt2020Ncl,
                    transfer: Transfer::Pq,
                },
                planes,
            }
        }

        /// The fourcc decides what the memory holds, and the shape decides the
        /// Vulkan format the image is made as and the wgpu format the texture
        /// is declared as: the producer's own bytes, under each API's name for
        /// them, and no swizzle anywhere.
        #[test]
        fn a_fourcc_is_imported_as_its_format() {
            assert_eq!(Shape::of(Shape::ARGB8888), Ok(Shape::Bgra));
            assert_eq!(Shape::of(Shape::ABGR8888), Ok(Shape::Rgba));
            assert_eq!(Shape::of(Shape::NV12), Ok(Shape::Nv12));
            assert_eq!(Shape::of(Shape::P010_FOURCC), Ok(Shape::P010));
            assert_eq!(Shape::of(Shape::P016_FOURCC), Ok(Shape::P016));
            assert_eq!(Shape::Bgra.vk(), vk::Format::B8G8R8A8_UNORM);
            assert_eq!(Shape::Bgra.wgpu(), wgpu::TextureFormat::Bgra8Unorm);
            assert_eq!(Shape::Rgba.vk(), vk::Format::R8G8B8A8_UNORM);
            assert_eq!(Shape::Rgba.wgpu(), wgpu::TextureFormat::Rgba8Unorm);
            assert_eq!(Shape::Nv12.vk(), vk::Format::G8_B8R8_2PLANE_420_UNORM);
            assert_eq!(Shape::Nv12.wgpu(), wgpu::TextureFormat::NV12);
            assert_eq!(
                Shape::P010.vk(),
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
                "10 bits at the top of each 16-bit sample"
            );
            assert_eq!(
                Shape::P016.vk(),
                vk::Format::G16_B16R16_2PLANE_420_UNORM,
                "16-bit samples, which is what NVDEC decodes into"
            );
            assert_eq!(
                Shape::P010.wgpu(),
                wgpu::TextureFormat::P010,
                "wgpu's one 16-bit 4:2:0 format, whose plane views are R16 and Rg16"
            );
            assert_eq!(Shape::P016.wgpu(), wgpu::TextureFormat::P010);
            assert_eq!(
                Shape::P010.view(0),
                (wgpu::TextureFormat::R16Unorm, wgpu::TextureAspect::Plane0)
            );
            assert_eq!(
                Shape::P010.view(1),
                (wgpu::TextureFormat::Rg16Unorm, wgpu::TextureAspect::Plane1)
            );
            assert_eq!(
                Shape::Bgra.uses(),
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
                "a picture Skia samples itself is renderable too"
            );
            assert_eq!(
                Shape::Nv12.uses(),
                wgpu::TextureUsages::TEXTURE_BINDING,
                "NV12 is only read, by the pass that writes the tile"
            );
            assert_eq!(Shape::P010.uses(), wgpu::TextureUsages::TEXTURE_BINDING);
            // The depth the tile pass is told, which is how it reads a 16-bit
            // word: P010 keeps its 10 bits at the top and P016 at the bottom.
            assert_eq!(
                (
                    Shape::Nv12.depth(),
                    Shape::P010.depth(),
                    Shape::P016.depth()
                ),
                (0, 1, 2)
            );
        }

        /// A picture of a shape this importer can't show says so in words that
        /// name it, and a picture of a shape it can show has the number of
        /// planes that shape has.
        #[test]
        fn a_shape_is_read_from_its_fourcc_or_refused_in_words() {
            let mut numbers = nv12(4, 2);
            numbers.fourcc = fourcc(*b"XR24");
            let refused = layout(&numbers).expect_err("a format nothing here samples");
            assert!(refused.contains("XR24"), "the reason names it: {refused}");
            assert!(refused.contains("2") || refused.contains("plane"));

            // 4:2:2 in 16-bit samples is a picture the tile pass has no layout
            // for: its chroma is not shared by two rows.
            numbers.fourcc = fourcc(*b"NV16");
            assert!(layout(&numbers).is_err());

            // One plane and an NV12 fourcc is a picture whose memory was read
            // wrongly by whatever exported it.
            let mut half = nv12(4, 2);
            half.planes[1] = None;
            let refused = layout(&half).expect_err("a plane that isn't there");
            assert!(refused.contains("2 planes"), "{refused}");
        }

        /// Both planes of the card's NV12 are in one buffer, so they are one
        /// memory at the planes' own offsets and row pitches: two allocations
        /// for two views of one buffer would be the driver told the picture is
        /// something it isn't.
        #[test]
        fn the_planes_of_one_buffer_are_one_memory_at_their_own_places() {
            let plan = layout(&nv12(4, 2)).expect("a picture the card exports");
            assert_eq!(plan.shape, Shape::Nv12);
            assert!(!plan.disjoint(), "one buffer, so one memory");
            let planes: Vec<_> = plan.planes().collect();
            assert_eq!(planes.len(), 2);
            assert_eq!(
                planes[0],
                Placed {
                    offset: 0,
                    stride: 4,
                    memory: 0
                }
            );
            assert_eq!(
                planes[1],
                Placed {
                    offset: 8,
                    stride: 4,
                    memory: 0
                }
            );
            assert_eq!(plan.memories().collect::<Vec<_>>(), vec![7]);
            assert_eq!(plan.modifier, MOD_LINEAR, "linear, as it said");
        }

        /// Planes in different buffers are two memories, which is an image
        /// Vulkan binds one plane at a time.
        #[test]
        fn planes_in_different_buffers_are_a_disjoint_image() {
            let mut numbers = nv12(4, 2);
            numbers.planes[1] = Some((
                Plane {
                    offset: 0,
                    stride: 4,
                },
                9,
            ));
            let plan = layout(&numbers).expect("two buffers");
            assert!(plan.disjoint());
            assert_eq!(plan.memories().collect::<Vec<_>>(), vec![7, 9]);
            let planes: Vec<_> = plan.planes().collect();
            assert_eq!((planes[0].memory, planes[1].memory), (0, 1));
        }

        /// A 10-bit picture is two planes of one buffer like an 8-bit one, at
        /// rows of twice the bytes and with the colour signal it was coded with
        /// carried into the plan the tile pass reads.
        #[test]
        fn a_ten_bit_picture_is_two_wide_rows_of_one_memory() {
            for (fourcc, shape) in [
                (Shape::P010_FOURCC, Shape::P010),
                (Shape::P016_FOURCC, Shape::P016),
            ] {
                let numbers = ten_bit(4, 2, fourcc);
                let plan = layout(&numbers).expect("a 10-bit picture");
                assert_eq!(plan.shape, shape);
                assert!(!plan.disjoint(), "one buffer, so one memory");
                let planes: Vec<_> = plan.planes().collect();
                assert_eq!(planes.len(), 2);
                assert_eq!(
                    planes[0],
                    Placed {
                        offset: 0,
                        stride: 8,
                        memory: 0
                    },
                    "16 bits a sample"
                );
                assert_eq!(
                    planes[1],
                    Placed {
                        offset: 16,
                        stride: 8,
                        memory: 0
                    }
                );
                assert_eq!(
                    plan.colour,
                    Colour {
                        range: Range::Limited,
                        matrix: Matrix::Bt2020Ncl,
                        transfer: Transfer::Pq,
                    },
                    "the matrix, range and curve the stream declared"
                );

                // A 10-bit row is two bytes a pixel, so an 8-bit stride is a
                // picture read at half its width.
                let mut narrow = numbers;
                narrow.planes[0] = Some((
                    Plane {
                        offset: 0,
                        stride: 4,
                    },
                    7,
                ));
                let refused = layout(&narrow).expect_err("a row too narrow for 10 bits");
                assert!(refused.contains("at least 8"), "{refused}");
            }
        }

        /// A picture whose memory was laid out by a driver that didn't say how,
        /// or whose rows are closer together than its pixels, is refused rather
        /// than imported as something it isn't.
        #[test]
        fn a_layout_that_cannot_be_imported_is_refused_in_words() {
            let mut numbers = nv12(4, 2);
            numbers.modifier = MOD_INVALID;
            let refused = layout(&numbers).expect_err("no modifier");
            assert!(refused.contains("laid out"), "{refused}");

            let mut numbers = nv12(4, 2);
            numbers.planes[0] = Some((
                Plane {
                    offset: 0,
                    stride: 3,
                },
                7,
            ));
            let refused = layout(&numbers).expect_err("too narrow a row");
            assert!(refused.contains("3 bytes"), "{refused}");

            let mut numbers = nv12(4, 2);
            numbers.planes[0] = Some((
                Plane {
                    offset: 0,
                    stride: 4,
                },
                7,
            ));
            numbers.width = 3;
            let refused = layout(&numbers).expect_err("half a chroma sample");
            assert!(refused.contains("chroma"), "{refused}");

            let mut empty = nv12(4, 2);
            empty.height = 0;
            assert!(layout(&empty).is_err());
        }

        /// A 32-bit picture is one plane, one memory, at its own stride: what a
        /// capture exports when the picture is already colour.
        #[test]
        fn a_colour_picture_is_one_plane_of_four_bytes_a_pixel() {
            let mut planes = [None; MAX_PLANES];
            planes[0] = Some(plane(64, 32));
            let numbers = Numbers {
                width: 8,
                height: 2,
                fourcc: Shape::ARGB8888,
                modifier: MOD_LINEAR,
                colour: Colour::SDR,
                planes,
            };
            let plan = layout(&numbers).expect("a colour picture");
            assert_eq!(plan.shape, Shape::Bgra);
            assert_eq!(plan.memories().collect::<Vec<_>>(), vec![7]);
            assert_eq!(
                plan.planes().next(),
                Some(Placed {
                    offset: 64,
                    stride: 32,
                    memory: 0
                })
            );

            let mut narrow = numbers;
            narrow.planes[0] = Some(plane(0, 31));
            let refused = layout(&narrow).expect_err("four bytes a pixel");
            assert!(refused.contains("at least 32"), "{refused}");
        }
    }
}

/// The pictures one window has taken and not finished with, oldest first.
///
/// A picture is on the screen until the next replaces it, and the GPU may still
/// be sampling it after that, so everything but the newest is let go of only
/// once the window's queue has drawn a frame without it. Letting go is what
/// destroys the texture it was imported into and frees the memory behind it, so
/// it happens before the slot is the worker's again.
#[cfg(target_os = "linux")]
// Nothing outside the Vulkan importer leases a picture, but the rules here are
// worth testing in every Linux build.
#[cfg_attr(not(feature = "skia-vulkan"), allow(dead_code))]
mod leases {
    use std::collections::VecDeque;

    /// The pictures of one stream, oldest first.
    #[derive(Debug)]
    pub struct Leases<T> {
        queue: VecDeque<T>,
    }

    impl<T> Leases<T> {
        pub fn new() -> Self {
            Self {
                queue: VecDeque::new(),
            }
        }

        /// How many pictures are held, the one on the screen included.
        pub fn len(&self) -> usize {
            self.queue.len()
        }

        /// Takes a picture the window is showing now.
        pub fn push(&mut self, held: T) {
            self.queue.push_back(held);
        }

        /// The oldest picture, when it is safe to let go of: everything but the
        /// one on the screen. `None` while that is all there is.
        pub fn oldest(&mut self) -> Option<T> {
            if self.queue.len() <= 1 {
                return None;
            }
            self.queue.pop_front()
        }

        /// Lets go of every picture held, which a stream that has ended does.
        pub fn clear(&mut self) {
            self.queue.clear();
        }
    }

    impl<T> Default for Leases<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    #[cfg(test)]
    mod tests {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        use super::Leases;

        /// A picture that counts the times it was let go of. In the window it
        /// is the imported picture, whose drop destroys its texture and closes
        /// the descriptors behind it; what matters here is only *when* it is
        /// let go of, which a descriptor number can't say: tests run in one
        /// process and its numbers are handed out again as soon as one is free.
        struct Picture(Arc<AtomicUsize>);

        impl Drop for Picture {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        /// The picture on the screen is never let go of, and the one before it
        /// is — which is what closes what it holds, and why the worker's slot is
        /// the worker's again only then.
        #[test]
        fn a_picture_is_let_go_of_once_the_next_is_on_the_screen() {
            let let_go = Arc::new(AtomicUsize::new(0));
            let mut held = Leases::new();
            assert!(held.oldest().is_none(), "nothing held");
            held.push(Picture(let_go.clone()));
            assert_eq!(held.len(), 1);
            assert!(
                held.oldest().is_none(),
                "the picture on the screen stays, alone as it is"
            );
            assert_eq!(let_go.load(Ordering::Relaxed), 0, "and it is still held");

            held.push(Picture(let_go.clone()));
            assert_eq!(held.len(), 2);
            drop(held.oldest());
            assert_eq!(
                let_go.load(Ordering::Relaxed),
                1,
                "letting go of the one before it closed what it held"
            );
            assert!(
                held.oldest().is_none(),
                "the picture on the screen is never let go of here"
            );
            drop(held);
            assert_eq!(
                let_go.load(Ordering::Relaxed),
                2,
                "a stream's last picture goes with it"
            );
        }

        /// A stream that has ended lets go of every picture it still holds.
        #[test]
        fn a_stream_that_ended_lets_go_of_what_it_held() {
            let let_go = Arc::new(AtomicUsize::new(0));
            let mut held = Leases::new();
            held.push(Picture(let_go.clone()));
            held.push(Picture(let_go.clone()));
            held.clear();
            assert_eq!(held.len(), 0);
            assert_eq!(let_go.load(Ordering::Relaxed), 2);
        }
    }
}

/// The window's own Vulkan device, and what each stream's DMA-BUF pictures
/// became on it: the Linux half of what the Direct3D 12 code above does with
/// shared textures.
#[cfg(all(target_os = "linux", feature = "skia-vulkan"))]
mod present {
    use std::{
        cell::RefCell,
        collections::HashMap,
        os::fd::{FromRawFd, OwnedFd, RawFd},
        ptr,
        sync::{Arc, Weak},
    };

    use ash::{khr, vk};
    use slint::wgpu_30::wgpu;
    use yapfast_core::{
        dmabuf::{self, MAX_PLANES},
        frames::Frames,
    };

    use super::{LEASES, leases::Leases, plane, queue_empty, refuse, wait_drawn};

    thread_local! {
        /// The device Slint draws the window with, once it draws on Vulkan: a
        /// picture is imported there and shown from there, and no other device
        /// can show it.
        static PRESENT: RefCell<Option<Present>> = const { RefCell::new(None) };
    }

    /// Hands the window's device to the importer. Called by `vulkan::install`,
    /// the one place that device is made.
    pub fn install(device: wgpu::Device, queue: wgpu::Queue) {
        let number = device_number(&device);
        PRESENT.with_borrow_mut(|present| {
            *present = Some(Present {
                device,
                queue,
                number,
                streams: HashMap::new(),
                pass: None,
            });
        });
    }

    /// The number the window's device is known by in the frames header, where
    /// the shared texture path carries the adapter's LUID: the card's vendor and
    /// device ids, which every process that opens this card reads the same way.
    /// It names the model rather than one card of that model, which is enough
    /// for the worker's own comparison — what decides whether a picture can be
    /// shown is the import itself, and a picture that can't be is refused.
    fn device_number(device: &wgpu::Device) -> u64 {
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

    /// Shows a picture the worker drew into a DMA-BUF, or refuses this stream's
    /// pictures and says why, once.
    pub fn show(frames: &Arc<Frames>, picture: dmabuf::Picture) -> Option<slint::Image> {
        let (slot, generation) = (picture.slot(), picture.generation());
        let shown = PRESENT.with_borrow_mut(|present| match present.as_mut() {
            Some(present) => present.show(frames, picture),
            None => {
                if frames.refuse_gpu() {
                    tracing::warn!(
                        "the window has no Vulkan device of its own; showing streams' pixels"
                    );
                }
                None
            }
        });
        if shown.is_none() {
            // Nothing shows this picture, so nothing samples what it is in: the
            // worker has its slot back at once, rather than a stream left
            // holding every slot it publishes into.
            frames.retire(slot, generation);
        }
        shown
    }

    /// Lets go of what no stream is using any more: a stream the UI let go of,
    /// and one whose pictures it refused. A stopped stream keeps the imported
    /// pictures it holds, and the worker's slots with them, for as long as a
    /// window keeps the entry — which nothing else would ever revisit.
    pub fn housekeeping() {
        PRESENT.with_borrow_mut(|present| {
            let Some(present) = present.as_mut() else {
                return;
            };
            present.housekeeping();
        });
    }

    /// Lets go of a stream that has ended: the pictures it still holds are
    /// dropped, which closes what is behind them, and the slots they are in are
    /// the worker's again.
    pub fn forget(key: u64) {
        PRESENT.with_borrow_mut(|present| {
            let Some(present) = present.as_mut() else {
                return;
            };
            present.housekeeping();
            if let Some(mut stream) = present.streams.remove(&key) {
                stream.leases.clear();
            }
        });
    }

    /// The window's device, what the header calls it, and what was imported of
    /// each stream's pictures.
    struct Present {
        device: wgpu::Device,
        queue: wgpu::Queue,
        /// What the header calls the adapter the window draws on; see
        /// [`device_number`].
        number: u64,
        streams: HashMap<u64, Stream>,
        /// The pass that turns the card's NV12 into what Skia samples, made the
        /// first time a picture of that shape arrives: a pipeline is a shader
        /// compile, and one is enough for the whole window.
        pass: Option<Pass>,
    }

    /// What the UI imported of one stream's pictures.
    struct Stream {
        /// The stream this was made for, so that a stream the UI let go leaves
        /// an id a new one takes over without inheriting what was imported for
        /// the one before it.
        frames: Weak<Frames>,
        /// The pictures taken and not finished with, oldest first: the worker
        /// may draw into those slots again only as they leave this queue, and
        /// the newest is the picture on the screen.
        leases: Leases<Lease>,
        /// Whether the UI has refused this stream's pictures: what was imported
        /// for it is let go with it, rather than waiting for another stream to
        /// draw a picture.
        refused: bool,
    }

    /// A picture the window has taken: the slot it is in, which picture of that
    /// slot it is, and what was imported for it. The worker keeps the slot for
    /// as long as this is unfinished with.
    struct Lease {
        slot: u32,
        generation: u64,
        shown: Shown,
    }

    /// A picture that was imported, and what Skia shows of it.
    struct Shown {
        /// The picture's own texture, over the memory the worker shared.
        picture: wgpu::Texture,
        /// The tile the pass wrote, where the picture's shape is not one Skia
        /// samples: shown instead of the picture, and reading it, so it lives
        /// exactly as long as the picture does.
        tile: Option<wgpu::Texture>,
    }

    impl Shown {
        /// What Slint is shown of the picture: the tile where a pass was needed
        /// to read its planes, and the picture itself where Skia samples it.
        fn image(&self) -> Result<slint::Image, String> {
            let texture = self.tile.as_ref().unwrap_or(&self.picture).clone();
            slint::Image::try_from(texture)
                .map_err(|error| format!("Slint can't show a stream's DMA-BUF picture: {error:?}"))
        }
    }

    impl Present {
        /// Shows a picture the worker drew, or refuses this stream's pictures
        /// and says why, once.
        fn show(&mut self, frames: &Arc<Frames>, picture: dmabuf::Picture) -> Option<slint::Image> {
            // A stream the UI let go of has left an entry that nothing may
            // inherit.
            self.housekeeping();
            let Present {
                device,
                queue,
                number,
                streams,
                pass,
            } = self;
            let stream = streams
                .entry(frames.id())
                .or_insert_with(|| Stream::new(frames));
            // Slots the window's queue has finished with are the worker's
            // again, and this is where it hears it: a picture taken is on the
            // screen until the next one replaces it, and the queue has to have
            // drawn a frame without it.
            stream.retire_drawn(device, frames);
            if stream.leases.len() >= LEASES {
                // The queue is still drawing pictures this window replaced: ask
                // it for the rest of what it was given, briefly, rather than
                // passing over every picture from here on.
                wait_drawn(device);
                stream.retire_drawn(device, frames);
                if stream.leases.len() >= LEASES {
                    return None;
                }
            }
            let shown = match import(device, queue, pass, &picture) {
                Ok(shown) => shown,
                Err(error) => {
                    stream.refused = true;
                    refuse(frames, error, "could not import a stream's DMA-BUF picture");
                    return None;
                }
            };
            let image = match shown.image() {
                Ok(image) => image,
                Err(error) => {
                    stream.refused = true;
                    refuse(frames, error, "could not show a stream's DMA-BUF picture");
                    return None;
                }
            };
            // From here the worker knows which device to draw for, and that its
            // pictures are shown rather than copied.
            frames.accept_dmabuf(*number);
            stream.leases.push(Lease {
                slot: picture.slot(),
                generation: picture.generation(),
                shown,
            });
            Some(image)
        }

        fn housekeeping(&mut self) {
            let Present {
                device, streams, ..
            } = self;
            streams.retain(|_, stream| !stream.refused && stream.frames.strong_count() > 0);
            // The window's queue finishing with a picture is the worker's to hear
            // whether or not another picture came: a slot the reader took is the
            // worker's again only once the queue has drawn a frame without it, and
            // a worker whose slot this window still holds has no picture to hand
            // it — so retiring on a take alone would wait for the very picture it
            // is needed for. A second of that and the share gives the picture path
            // up, writing its pixels from then on. This runs every tick, on a
            // stream that has nothing new as much as on one that has.
            for stream in streams.values_mut() {
                if let Some(frames) = stream.frames.upgrade() {
                    stream.retire_drawn(device, &frames);
                }
            }
        }
    }

    impl Stream {
        fn new(frames: &Arc<Frames>) -> Self {
            Self {
                frames: Arc::downgrade(frames),
                leases: Leases::new(),
                refused: false,
            }
        }

        /// Retires the pictures the window's queue has drawn a frame without: the
        /// worker may draw into those slots again, and the memory it shared is
        /// free of this process's reading of it. The newest lease is the picture
        /// on the screen and is never retired here.
        fn retire_drawn(&mut self, device: &wgpu::Device, frames: &Arc<Frames>) {
            while queue_empty(device) {
                let Some(lease) = self.leases.oldest() else {
                    break;
                };
                let Lease {
                    slot,
                    generation,
                    shown,
                } = lease;
                // The picture goes before the slot is the worker's again: its
                // texture, the image it was imported into and the descriptors
                // behind it are closed here, and only then may the worker draw
                // into the memory again.
                drop(shown);
                frames.retire(slot, generation);
            }
        }
    }

    /// Imports one picture on the window's device, and says what Skia is shown
    /// of it.
    fn import(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut Option<Pass>,
        picture: &dmabuf::Picture,
    ) -> Result<Shown, String> {
        let plan = plane::layout(&plane::Numbers::of(picture))?;
        // The import takes a duplicate of every descriptor it is given, and the
        // picture keeps its own until it is dropped here: the memory belongs to
        // the worker's channel until this process has its own reference to it.
        let imported = unsafe { texture(device, &plan) }?;
        let tile = match plan.shape {
            plane::Shape::Rgba | plane::Shape::Bgra => None,
            plane::Shape::Nv12 | plane::Shape::P010 | plane::Shape::P016 => {
                let pass = match pass.as_mut() {
                    Some(pass) => pass,
                    None => pass.insert(Pass::new(device)?),
                };
                Some(pass.tile(device, queue, &imported, &plan)?)
            }
        };
        Ok(Shown {
            picture: imported,
            tile,
        })
    }

    /// A texture over the memory a picture's descriptors name, made on the
    /// window's device through Vulkan's external memory: no pixel of the picture
    /// is copied anywhere.
    ///
    /// The image is the picture's own shape: the fourcc's Vulkan format, the DRM
    /// modifier with each plane's offset and row pitch given explicitly — a
    /// modifier says how the driver laid the buffer out as a whole, and the
    /// import is what says where this picture's planes start in it — and
    /// `VK_EXTERNAL_MEMORY_HANDLE_TYPE_DMA_BUF_BIT_EXT` in its creation chain.
    /// One `VkDeviceMemory` is imported for each buffer the planes are in, from
    /// a duplicate this process owns of that buffer's descriptor, and bound to
    /// every plane in it.
    ///
    /// # Safety
    ///
    /// The plan must be what the picture says, and the picture must have been
    /// taken from its stream's socket, so that the descriptors in it are the
    /// memory the worker shared.
    unsafe fn texture(device: &wgpu::Device, plan: &plane::Plan) -> Result<wgpu::Texture, String> {
        if !device
            .features()
            .contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF)
        {
            return Err(
                "the window's Vulkan device can't import DMA-BUF memory: VK_EXT_external_memory_dma_buf and VK_EXT_image_drm_format_modifier are not both enabled on it"
                    .to_string(),
            );
        }
        if plan.shape == plane::Shape::Nv12
            && !device
                .features()
                .contains(wgpu::Features::TEXTURE_FORMAT_NV12)
        {
            return Err(
                "the window's Vulkan device has no NV12 texture, which is the card's picture"
                    .to_string(),
            );
        }
        // A 10-bit picture is imported as a texture of 16-bit planes, which is
        // the feature wgpu gates behind P010's name whatever the fourcc was.
        if plan.shape.tiled()
            && plan.shape.depth() != 0
            && !device
                .features()
                .contains(wgpu::Features::TEXTURE_FORMAT_P010)
        {
            return Err(
                "the window's Vulkan device has no 16-bit texture for a 10-bit picture (P010)"
                    .to_string(),
            );
        }
        // SAFETY: the device is the window's and is alive here.
        let hal = unsafe { device.as_hal::<wgpu::hal::vulkan::Api>() }
            .ok_or("the window's device is not Vulkan")?;
        let raw = hal.raw_device();
        let instance = hal.shared_instance().raw_instance();
        let physical = hal.raw_physical_device();
        let fd_fn = khr::external_memory_fd::Device::new(hal.shared_instance().raw_instance(), raw);

        // Where each plane starts and how its rows are spaced, in the shape
        // Vulkan asks for them: the explicit layout of a DRM modifier image.
        let layouts: Vec<vk::SubresourceLayout> = plan
            .planes()
            .map(|placed| vk::SubresourceLayout {
                offset: u64::from(placed.offset),
                row_pitch: u64::from(placed.stride),
                size: 0,
                array_pitch: 0,
                depth_pitch: 0,
            })
            .collect();
        let mut external = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
        let mut modifier = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
            .drm_format_modifier(plan.modifier)
            .plane_layouts(&layouts);
        let mut flags = vk::ImageCreateFlags::empty();
        if plan.disjoint() {
            // A picture whose planes are in different buffers is one image whose
            // planes each have their own memory.
            flags |= vk::ImageCreateFlags::DISJOINT;
        }
        let info = vk::ImageCreateInfo::default()
            .flags(flags)
            .image_type(vk::ImageType::TYPE_2D)
            .format(plan.shape.vk())
            .extent(vk::Extent3D {
                width: plan.width,
                height: plan.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            // Not OPTIMAL and not LINEAR: the tiling is the modifier's, and
            // Vulkan reads it out of the structure chained below.
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(
                if plan
                    .shape
                    .uses()
                    .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
                {
                    vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::COLOR_ATTACHMENT
                } else {
                    vk::ImageUsageFlags::SAMPLED
                },
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        // Each structure is chained as it is taken: ash's builder hands the
        // image's description back rather than filling it in place.
        let info = info.push_next(&mut external);
        let info = info.push_next(&mut modifier);
        // SAFETY: the image is made on the device the plan is for, and the two
        // structures it chains are alive for the call.
        let image = unsafe { raw.create_image(&info, None) }
            .map_err(|error| format!("Vulkan wouldn't make the picture's image: {error}"))?;
        // From here the image and every memory that is bound to it are let go of
        // unless the import finishes: a picture half-imported is memory the
        // window holds for nothing.
        let mut fresh = Fresh {
            device: raw.handle(),
            destroy_image: raw.fp_v1_0().destroy_image,
            free_memory: raw.fp_v1_0().free_memory,
            image,
            memories: [None; MAX_PLANES],
            handed_over: false,
        };

        for (index, fd) in plan.memories().enumerate() {
            let fd = duplicate(fd)?;
            // What this buffer has to be: the image's whole requirement when
            // every plane is in this buffer, and the plane's own when they are
            // not.
            let requirements = if plan.disjoint() {
                let mut plane_info = vk::ImagePlaneMemoryRequirementsInfo::default()
                    .plane_aspect(plane_aspect(index));
                let asked = vk::ImageMemoryRequirementsInfo2::default()
                    .image(image)
                    .push_next(&mut plane_info);
                let mut answer = vk::MemoryRequirements2::default();
                // SAFETY: the image is this one and the structure describes
                // which plane is asked about.
                unsafe { raw.get_image_memory_requirements2(&asked, &mut answer) };
                answer.memory_requirements
            } else {
                // SAFETY: the image is this one, alive, and made on this device.
                unsafe { raw.get_image_memory_requirements(image) }
            };
            fresh.memories[index] = Some(import_memory(
                raw,
                instance,
                physical,
                &fd_fn,
                fd,
                image,
                &requirements,
            )?);
        }

        if plan.disjoint() {
            for (index, memory) in fresh.memories.iter().flatten().enumerate() {
                let mut plane_info =
                    vk::BindImagePlaneMemoryInfo::default().plane_aspect(plane_aspect(index));
                let bind = vk::BindImageMemoryInfo::default()
                    .image(image)
                    .memory(*memory)
                    .push_next(&mut plane_info);
                // SAFETY: the memory was imported for this plane, and the image
                // is the one it is being bound to.
                unsafe { raw.bind_image_memory2(&[bind]) }.map_err(|error| {
                    format!("Vulkan wouldn't bind a picture's plane memory: {error}")
                })?;
            }
        } else if let Some(memory) = fresh.memories[0] {
            // SAFETY: the memory was imported for this image, whose planes are
            // all in that one buffer.
            unsafe { raw.bind_image_memory(image, memory, 0) }
                .map_err(|error| format!("Vulkan wouldn't bind a picture's memory: {error}"))?;
        }

        let hal_desc = wgpu::hal::TextureDescriptor {
            label: Some("a stream's DMA-BUF picture"),
            size: wgpu::Extent3d {
                width: plan.width,
                height: plan.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: plan.shape.wgpu(),
            usage: if plan
                .shape
                .uses()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            {
                wgpu::wgt::TextureUses::RESOURCE | wgpu::wgt::TextureUses::COLOR_TARGET
            } else {
                wgpu::wgt::TextureUses::RESOURCE
            },
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: Vec::new(),
        };
        let memories = fresh.memories;
        let (device_handle, destroy_image, free_memory) =
            (fresh.device, fresh.destroy_image, fresh.free_memory);
        // SAFETY: the image is one this function made on this device, is not
        // owned by anything else, and the memory it is bound to is the callback's
        // to free: `External` says wgpu-hal frees neither, and the callback runs
        // when the last texture over it is gone.
        let hal_texture = unsafe {
            hal.texture_from_raw(
                image,
                &hal_desc,
                Some(Box::new(move || {
                    // SAFETY: the device is alive for as long as a texture it
                    // made is, and nothing else has freed this memory or
                    // destroyed this image: this is the only place that does.
                    // This block is the one above, which a closure body is
                    // inside of.
                    for memory in memories.iter().flatten() {
                        (free_memory)(device_handle, *memory, ptr::null());
                    }
                    (destroy_image)(device_handle, image, ptr::null());
                })),
                wgpu::hal::vulkan::TextureMemory::External,
            )
        };
        fresh.handed_over = true;
        let desc = wgpu::TextureDescriptor {
            label: Some("a stream's DMA-BUF picture"),
            size: hal_desc.size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: plan.shape.wgpu(),
            usage: plan.shape.uses(),
            view_formats: &[],
        };
        // SAFETY: the hal texture was made from this device's own handle, with
        // the description above, and the memory behind it holds the picture.
        Ok(unsafe {
            device.create_texture_from_hal::<wgpu::hal::vulkan::Api>(
                hal_texture,
                &desc,
                wgpu::wgt::TextureUses::UNINITIALIZED,
            )
        })
    }

    /// The image and the memories bound to it, until they are wgpu's: a picture
    /// whose import stopped half way has its memory freed and its image destroyed
    /// here, rather than held for as long as the window.
    struct Fresh {
        device: vk::Device,
        destroy_image: vk::PFN_vkDestroyImage,
        free_memory: vk::PFN_vkFreeMemory,
        image: vk::Image,
        memories: [Option<vk::DeviceMemory>; MAX_PLANES],
        handed_over: bool,
    }

    impl Drop for Fresh {
        fn drop(&mut self) {
            if self.handed_over {
                return;
            }
            // SAFETY: nothing else was given this image or these memories: they
            // were made by the import that owns this, on this device.
            unsafe {
                for memory in self.memories.iter().flatten() {
                    (self.free_memory)(self.device, *memory, ptr::null());
                }
                (self.destroy_image)(self.device, self.image, ptr::null());
            }
        }
    }

    /// Imports one buffer's memory, from a descriptor this process owns, and
    /// says the memory Vulkan made of it. The descriptor is Vulkan's from here:
    /// it keeps it until the memory is freed, which is what keeps the worker's
    /// memory there after this process lets go of its own descriptor.
    fn import_memory(
        raw: &ash::Device,
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        fd_fn: &khr::external_memory_fd::Device,
        fd: OwnedFd,
        image: vk::Image,
        requirements: &vk::MemoryRequirements,
    ) -> Result<vk::DeviceMemory, String> {
        use std::os::fd::IntoRawFd;

        // Vulkan takes ownership of the descriptor on a successful allocation,
        // and leaves it to the caller when the allocation fails.
        let fd = fd.into_raw_fd();
        let mut properties = vk::MemoryFdPropertiesKHR::default();
        // SAFETY: the descriptor is one this process holds, of DMA-BUF memory,
        // and the structure is the one this call fills in.
        if let Err(error) = unsafe {
            fd_fn.get_memory_fd_properties(
                vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
                fd,
                &mut properties,
            )
        } {
            // SAFETY: Vulkan didn't consume the descriptor, so it is still this
            // process's to close.
            unsafe { libc::close(fd) };
            return Err(format!(
                "Vulkan wouldn't read a picture's descriptor: {error}"
            ));
        }
        let Some(memory_type) = memory_type(
            instance,
            physical,
            requirements.memory_type_bits & properties.memory_type_bits,
        ) else {
            // SAFETY: as above: nothing consumed the descriptor.
            unsafe { libc::close(fd) };
            return Err(
                "no memory type on the window's device takes the picture's buffer".to_string(),
            );
        };
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let mut imported = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(fd);
        let allocate = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type)
            .push_next(&mut imported)
            .push_next(&mut dedicated);
        // SAFETY: the descriptor is a DMA-BUF's, the size and type come from the
        // image's own requirements, and a picture's memory is dedicated to the
        // image it is imported for.
        match unsafe { raw.allocate_memory(&allocate, None) } {
            Ok(memory) => Ok(memory),
            Err(error) => {
                // SAFETY: the allocation failed, so Vulkan didn't take the
                // descriptor and this process still owns it.
                unsafe { libc::close(fd) };
                Err(format!(
                    "Vulkan wouldn't import a picture's memory: {error}"
                ))
            }
        }
    }

    /// A memory type the image can be bound to that the buffer's own device
    /// memory is: the first the two agree on, preferring the one the card keeps
    /// to itself.
    fn memory_type(
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        bits: u32,
    ) -> Option<u32> {
        // SAFETY: the physical device is the one the window's device was made
        // on, and Vulkan answers its memory properties at any time.
        let properties = unsafe { instance.get_physical_device_memory_properties(physical) };
        for (index, kind) in properties.memory_types_as_slice().iter().enumerate() {
            let bit = 1u32 << index;
            if bits & bit != 0
                && kind
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            {
                return Some(index as u32);
            }
        }
        // No device-local type takes it: an importer takes what it is given.
        (0..32).find(|index| bits & (1 << index) != 0)
    }

    /// A descriptor this process owns for the same memory `fd` names: the
    /// picture's own are its channel's and are closed when it is, and Vulkan
    /// takes the one it is given. `F_DUPFD_CLOEXEC`, so the copy is nobody's but
    /// this import's and no process this one starts inherits it.
    fn duplicate(fd: RawFd) -> Result<OwnedFd, String> {
        // SAFETY: the number is one the picture holds, and a duplicate of it is
        // what this asks the kernel for.
        let duplicated = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
        if duplicated < 0 {
            return Err(format!(
                "could not duplicate a picture's descriptor: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: the number is one the kernel just made for this process, and
        // nothing else here holds it.
        Ok(unsafe { OwnedFd::from_raw_fd(duplicated) })
    }

    /// Which plane of a disjoint image index `index` names.
    fn plane_aspect(index: usize) -> vk::ImageAspectFlags {
        match index {
            0 => vk::ImageAspectFlags::PLANE_0,
            1 => vk::ImageAspectFlags::PLANE_1,
            _ => vk::ImageAspectFlags::PLANE_2,
        }
    }

    /// What the pass reads and writes: the tile Skia samples, in the format the
    /// Direct3D path's pictures are in, so both look the same.
    const TILE: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

    /// The pass that turns the card's picture into what Skia samples: one
    /// triangle over the tile, reading the picture's luma and chroma planes and
    /// writing colour, which is also where the tile's own size is the scale.
    struct Pass {
        pipeline: wgpu::RenderPipeline,
        layout: wgpu::BindGroupLayout,
        sampler: wgpu::Sampler,
        /// The colour signal the shader reads — the range, matrix, curve and
        /// depth — in one uniform: it is the same for every picture of a stream,
        /// so it is written only when a picture changes it.
        signal: wgpu::Buffer,
        written: RefCell<Option<[u32; 4]>>,
    }

    impl Pass {
        fn new(device: &wgpu::Device) -> Result<Self, String> {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("a stream's picture planes"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty,
                count: None,
            };
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("a stream's picture planes"),
                entries: &[
                    entry(
                        0,
                        wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                    ),
                    entry(
                        1,
                        wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                    ),
                    entry(
                        2,
                        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    ),
                    entry(
                        3,
                        wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                    ),
                ],
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("a stream's picture tile"),
                // One set, at 0, and no immediate data: wgpu 30 names every
                // group that is there rather than only counting the fixed ones.
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("a stream's picture tile"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: TILE,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                // One layer, so no multiview: wgpu 30 asks for the mask it
                // would be used with rather than the layer count.
                multiview_mask: None,
                cache: None,
            });
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("a stream's picture planes"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            // Four numbers, the uniform block's own size: nothing here is read
            // back, and the pass only ever writes it from the processor.
            let signal = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("a stream's picture colour"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            Ok(Self {
                pipeline,
                layout,
                sampler,
                signal,
                written: RefCell::new(None),
            })
        }

        /// Reads the picture's two planes into a tile the picture's own size, on
        /// the window's queue, and says the texture it wrote. The plan says how
        /// deep the samples are and the signal they are in, which the shader
        /// turns into the colour that was coded.
        fn tile(
            &self,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            picture: &wgpu::Texture,
            plan: &plane::Plan,
        ) -> Result<wgpu::Texture, String> {
            let (width, height, shape, colour) = (plan.width, plan.height, plan.shape, plan.colour);
            let plane = |label, plane| {
                let (format, aspect) = shape.view(plane);
                picture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some(label),
                    format: Some(format),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    usage: None,
                    aspect,
                    base_mip_level: 0,
                    mip_level_count: None,
                    base_array_layer: 0,
                    array_layer_count: None,
                })
            };
            let luma = plane("a stream's picture luma", 0);
            let chroma = plane("a stream's picture chroma", 1);
            // The numbers the shader reads: the range, the matrix, the curve,
            // and how deep the samples are, which is what says where in a 16-bit
            // word the 10 bits are.
            let signal = [
                colour.range as u32,
                colour.matrix as u32,
                colour.transfer as u32,
                shape.depth(),
            ];
            if *self.written.borrow() != Some(signal) {
                let mut bytes = [0; 16];
                for (word, number) in bytes.chunks_exact_mut(4).zip(signal) {
                    word.copy_from_slice(&number.to_ne_bytes());
                }
                queue.write_buffer(&self.signal, 0, &bytes);
                *self.written.borrow_mut() = Some(signal);
            }
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("a stream's picture planes"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&luma),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&chroma),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.signal.as_entire_binding(),
                    },
                ],
            });
            let tile = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("a stream's picture tile"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TILE,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let target = tile.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("a stream's picture tile"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("a stream's picture tile"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.draw(0..3, 0..1);
            }
            queue.submit(Some(encoder.finish()));
            Ok(tile)
        }
    }

    /// One triangle over the whole tile: its vertices are named by their index,
    /// so it needs no buffer, and its texture coordinates come out of the same
    /// numbers. The chroma planes are sampled at their own resolution, which the
    /// sampler does with the picture's texels.
    ///
    /// The 8-bit path is what it has always been: the card's NV12 is BT.709
    /// limited range, and that is what those six numbers convert. A 10-bit
    /// picture is one of two matrices, a range, and a curve, which the shim read
    /// out of the stream and the uniform carries in.
    const SHADER: &str = r#"
struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Vertex {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: Vertex;
    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0) var luma: texture_2d<f32>;
@group(0) @binding(1) var chroma: texture_2d<f32>;
@group(0) @binding(2) var planes: sampler;

// The colour signal of the picture being shown, in the numbers the worker's
// shim read out of the stream: x = the range (0 limited, 1 full), y = the matrix
// (0 BT.709, 1 BT.2020's non-constant-luminance), z = the curve (0 SDR, 1 PQ),
// and w = how deep the samples are (0 eight bits, 1 P010, 2 P016).
@group(0) @binding(3) var<uniform> signal: vec4<u32>;

// The 10-bit codes a limited-range sample spans: luma 64..940 and chroma
// 64..960 out of 0..1023, which is the studio range of 10 bits.
const LUMA_LIMITED = vec2<f32>(64.0, 940.0);
const CHROMA_LIMITED = vec2<f32>(64.0, 960.0);

// One 16-bit plane sample as its 10-bit code, 0..1023. A P010 word keeps the
// ten bits at its top, so dividing by 64 is reading them; a P016 word keeps
// them at its bottom, which is what is left below the top six.
fn ten(bit: f32, depth: u32) -> f32 {
    let word = bit * 65535.0;
    var code = word / 64.0;
    if depth == 2u {
        code = word - 64.0 * floor(word / 64.0);
    }
    return clamp(code, 0.0, 1023.0);
}

// A 10-bit code in the range it uses, as 0..1: 64..940 for luma and 64..960 for
// chroma where the stream is limited, and the whole of 0..1023 where it is full.
fn expand(code: f32, limited: vec2<f32>, full: bool) -> f32 {
    if full {
        return code / 1023.0;
    }
    return (code - limited.x) / (limited.y - limited.x);
}

// SMPTE ST 2084's inverse EOTF: a PQ code, 0..1, as the light it stands for,
// 0..1 of the 10000 nits the curve maps.
fn from_pq(e: f32) -> f32 {
    let m1 = 2610.0 / 16384.0;
    let m2 = (2523.0 / 4096.0) * 128.0;
    let c1 = 3424.0 / 4096.0;
    let c2 = (2413.0 / 4096.0) * 32.0;
    let c3 = (2392.0 / 4096.0) * 32.0;
    let p = pow(max(e, 0.0), 1.0 / m2);
    return pow(max(p - c1, 0.0) / max(c2 - c3 * p, 1e-5), 1.0 / m1);
}

// The light a PQ picture holds, tone mapped to what SDR shows. `light` is in
// units of 100 nits, which is SDR's white: everything below the 0.85 the knee
// starts at — 85 nits, which is most of a picture most of the time — is left
// exactly as it is, and the highlights roll off through the Hermite spline a
// BT.2390 tone map is made of, to white at 400 nits, with the colour scaled by
// how far its own luminance moved so that it keeps its hue. A BT.2390 EETF is
// the shape the standard asks for; the knee and the peak are this window's own,
// because a stream is shown on a desktop rather than on a graded monitor.
fn tone_map(light: vec3<f32>) -> vec3<f32> {
    // BT.2020's luma coefficients, which is the light this picture is in.
    let luma = dot(light, vec3<f32>(0.2627, 0.6780, 0.0593));
    if luma <= 0.0 {
        return light;
    }
    let knee = 0.85;
    let peak = 4.0;
    var mapped = luma;
    if luma > knee {
        let t = min((luma - knee) / (peak - knee), 1.0);
        mapped = knee + (1.0 - knee) * (t * t * (3.0 - 2.0 * t));
    }
    return light * (mapped / luma);
}

// Linear light in BT.2020's primaries as linear light in BT.709's, which is the
// matrix BT.2087 gives and what shrinking a wide gamut into a narrow one is. A
// negative channel is a colour BT.709 has no way to show — the gamut clamp — and
// `encoded` takes it to none of that channel.
fn to_bt709(light: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        1.6605 * light.x - 0.5876 * light.y - 0.0729 * light.z,
        -0.1246 * light.x + 1.1329 * light.y - 0.0083 * light.z,
        -0.0182 * light.x - 0.1006 * light.y + 1.1187 * light.z,
    );
}

// Linear light as the display-referred codes the tile holds: BT.1886's 2.4
// gamma, which is what SDR video is, so that the tile shows the light a tone
// map chose rather than a code that happens to be near it.
fn encoded(light: vec3<f32>) -> vec3<f32> {
    return pow(clamp(light, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.4));
}

@fragment
fn fs_main(in: Vertex) -> @location(0) vec4<f32> {
    // Sampled out here, so that every branch below reads the same texels.
    let luma_bit = textureSample(luma, planes, in.uv).r;
    let chroma_bit = textureSample(chroma, planes, in.uv).rg;
    if signal.w == 0u {
        // Eight bits: the 8-bit 4:2:0 path, BT.709 limited range.
        let y = 1.164 * (luma_bit - 16.0 / 255.0);
        let c = chroma_bit - vec2<f32>(128.0 / 255.0);
        let rgb = vec3<f32>(
            y + 1.793 * c.y,
            y - 0.213 * c.x - 0.533 * c.y,
            y + 2.112 * c.x,
        );
        return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
    }
    // Ten bits, at the depth the picture's own words hold them.
    let full = signal.x == 1u;
    let depth = signal.w;
    let y = expand(ten(luma_bit, depth), LUMA_LIMITED, full);
    let c = vec2<f32>(
        expand(ten(chroma_bit.r, depth), CHROMA_LIMITED, full) - 0.5,
        expand(ten(chroma_bit.g, depth), CHROMA_LIMITED, full) - 0.5,
    );
    // The picture's own non-linear R'G'B', which is what its matrix and curve
    // are said about.
    var rgb: vec3<f32>;
    if signal.y == 1u {
        // BT.2020's non-constant-luminance matrix, which HDR10 is coded with.
        rgb = vec3<f32>(
            y + 1.4746 * c.y,
            y - 0.16455 * c.x - 0.57135 * c.y,
            y + 1.8814 * c.x,
        );
    } else {
        // BT.709's, which is what a 10-bit stream that names no matrix is.
        rgb = vec3<f32>(
            y + 1.5748 * c.y,
            y - 0.1873 * c.x - 0.4681 * c.y,
            y + 1.8556 * c.x,
        );
    }
    if signal.z == 1u {
        // PQ: the light the codes stand for, in units of 100 nits, tone mapped
        // into what SDR shows and shrunk into BT.709's gamut.
        let light = vec3<f32>(from_pq(rgb.x), from_pq(rgb.y), from_pq(rgb.z)) * 100.0;
        return vec4<f32>(encoded(to_bt709(tone_map(light))), 1.0);
    }
    // SDR: BT.1886's gamma is what the picture was graded in and what the tile
    // holds, so only the gamut matrix is left to do.
    let light = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(2.4));
    return vec4<f32>(encoded(to_bt709(light)), 1.0);
}
"#;

    /// The pass's shader is checked as it is written rather than when a window
    /// with a card under it first shows a picture: wgpu parses and validates WGSL
    /// as it makes the module, and a machine with no Vulkan device would never
    /// say so. The same naga does both here, with no device at all.
    #[cfg(test)]
    mod shader {
        #[test]
        fn the_tile_pass_shader_parses_and_validates() {
            let module =
                naga::front::wgsl::parse_str(super::SHADER).expect("the tile pass's WGSL parses");
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .expect("the tile pass's WGSL validates");
        }
    }
}
