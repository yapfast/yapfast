//! Live video: the cameras in a call and a watched Go Live stream.
//!
//! Frames reach the window without a copy. A camera's frames are made by
//! the core straight into the client's memory (`set_frame_buffers`), at the
//! size its tile shows (`Feed::show_at`); a stream's are the stream
//! worker's own slots in the shared mapping (`Frames`). Each frame becomes a
//! new image `Handle` over that memory. On the CPU build the patched
//! renderer offers every picture to [`draw`] (patch 13 in
//! `vendor/iced_tiny_skia`), which recognises frames by their memory and
//! copies or scales them into the window with libyuv, only inside the
//! damaged part, swapping red and blue as it goes, since the core writes
//! RGBA and the window is BGRA.
//!
//! Nothing polls the window. [`frames`] waits on the core's wake for each
//! camera frame made (`set_frame_wake`); a watched stream, whose worker is
//! another process that tells nothing when a frame lands, is looked at
//! every 8 ms while it shows. Only a new frame sends a message, which
//! redraws the window once for all the frames found together. With no video
//! on screen it waits.

use std::{
    any::Any,
    cell::UnsafeCell,
    collections::HashMap,
    pin::pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use iced::futures::{Stream, future, stream};
use iced::{
    Color, Element, Length, Rectangle, Size, Theme,
    advanced::{
        Layout, Widget,
        image::{self as raster, Handle},
        layout, mouse, renderer,
        widget::Tree,
    },
};
use tokio::sync::Notify;
use yapfast_core::{
    frames::Frames,
    picture::{self, Feed, FrameBuffer},
};

use crate::{app::Message, paint};

type Renderer = iced::Renderer;

// ---------------------------------------------------------------------------
// Frame memory, known to the renderer's hook

/// Video memory by its first byte's address, with how many live owners
/// share it: frame buffers, and handles on a stream's slots. The hook draws
/// only pictures whose bytes start here.
static VIDEO: Mutex<Option<HashMap<usize, usize>>> = Mutex::new(None);
/// Entries in `VIDEO`, so a picture costs no lock while no video is on.
static VIDEO_COUNT: AtomicUsize = AtomicUsize::new(0);

fn register(address: usize) {
    let mut video = VIDEO.lock().expect("video memory");
    let count = video.get_or_insert_default().entry(address).or_insert(0);
    if *count == 0 {
        VIDEO_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    *count += 1;
}

fn unregister(address: usize) {
    let mut video = VIDEO.lock().expect("video memory");
    let Some(map) = video.as_mut() else { return };
    if let Some(count) = map.get_mut(&address) {
        *count -= 1;
        if *count == 0 {
            map.remove(&address);
            VIDEO_COUNT.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

#[cfg(feature = "cpu")]
fn is_video(address: usize) -> bool {
    VIDEO_COUNT.load(Ordering::Relaxed) > 0
        && VIDEO
            .lock()
            .expect("video memory")
            .as_ref()
            .is_some_and(|map| map.contains_key(&address))
}

/// A camera frame's pixels, written by the core's decode thread and read by
/// the renderer, which take turns by the feed's protocol rather than by a
/// lock: `Feed` keeps four buffers (one being written, one waiting, the one
/// on screen and the one before it) and hands one out to write only once
/// it's neither of the last two. The renderer reads a handle's bytes to
/// draw it, which it does only for the newest frame, and when it compares
/// two handles of the same id (`Handle`'s `PartialEq` compares ids first),
/// which happens only while that frame is on screen.
struct FrameMemory {
    pixels: UnsafeCell<Box<[u8]>>,
}

// SAFETY: the decode thread writes the pixels only while the feed has the
// buffer off screen, and the UI thread reads them only while it's on screen
// or the one before; see above. Nothing else touches them.
unsafe impl Sync for FrameMemory {}

impl FrameMemory {
    fn new(length: usize) -> Arc<Self> {
        let memory = Arc::new(Self {
            pixels: UnsafeCell::new(vec![0; length].into_boxed_slice()),
        });
        register(memory.address());
        memory
    }

    fn address(&self) -> usize {
        // SAFETY: reads the box's pointer, which never changes, not the
        // pixels another thread may be writing.
        unsafe { (*self.pixels.get()).as_ptr() as usize }
    }
}

impl Drop for FrameMemory {
    fn drop(&mut self) {
        unregister(self.address());
    }
}

/// A camera frame the core makes, in the port's memory.
struct Frame {
    size: (u32, u32),
    memory: Arc<FrameMemory>,
}

impl FrameBuffer for Frame {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn pixels_mut(&mut self) -> &mut [u8] {
        // SAFETY: the feed asks for a buffer to write only while it's off
        // screen, so no handle on it is read meanwhile; see `FrameMemory`.
        unsafe { &mut *self.memory.pixels.get() }
    }
}

/// A handle's view of a camera frame, keeping its memory.
struct FrameBytes(Arc<FrameMemory>);

impl AsRef<[u8]> for FrameBytes {
    fn as_ref(&self) -> &[u8] {
        // SAFETY: the renderer reads a frame only while it's on screen or
        // the one before, when the feed doesn't write it; see `FrameMemory`.
        unsafe { &*self.0.pixels.get() }
    }
}

impl Frame {
    fn handle(&self) -> Handle {
        Handle::from_rgba(
            self.size.0,
            self.size.1,
            bytes::Bytes::from_owner(FrameBytes(self.memory.clone())),
        )
    }
}

/// A handle's view of a stream's slot in the shared mapping. The slot is
/// the UI's until it takes the next frame; the worker then gets it back as
/// the middle slot and writes it only after its next frame, by which time
/// the window draws the newer handle. `Shared::take` takes no frame while
/// the last one hasn't reached the window. The mapping lives while any
/// handle does.
struct SlotBytes {
    _frames: Arc<Frames>,
    address: usize,
    length: usize,
}

impl AsRef<[u8]> for SlotBytes {
    fn as_ref(&self) -> &[u8] {
        // SAFETY: `address` and `length` are the slot `Frames::take` gave,
        // inside the mapping `_frames` keeps open, and the worker doesn't
        // write the slot while the window may still read it; see above.
        unsafe { std::slice::from_raw_parts(self.address as *const u8, self.length) }
    }
}

impl Drop for SlotBytes {
    fn drop(&mut self) {
        unregister(self.address);
    }
}

/// Told each time a camera's frame is made.
static FRAME_MADE: Notify = Notify::const_new();

/// Has cameras' frames made in the client's memory, with a wake for each,
/// and the CPU renderer draw them (and a stream's) through [`draw`].
pub fn install() {
    picture::set_frame_buffers(|width, height| {
        Box::new(Frame {
            size: (width, height),
            memory: FrameMemory::new(width as usize * height as usize * 4),
        })
    });
    picture::set_frame_wake(|| FRAME_MADE.notify_one());
    #[cfg(feature = "cpu")]
    iced_tiny_skia::video::set_hook(draw);
}

// ---------------------------------------------------------------------------
// Drawing: libyuv from Yapfast's libwebrtc archive

#[cfg(feature = "cpu")]
mod yuv {
    use std::ffi::c_int;

    /// libyuv's `FilterMode`.
    pub const BILINEAR: c_int = 2;

    unsafe extern "C" {
        /// Scales a 4-byte-per-pixel picture, writing only `clip` of the
        /// scaled one; channel order passes through.
        pub fn ARGBScaleClip(
            source: *const u8,
            source_stride: c_int,
            source_width: c_int,
            source_height: c_int,
            target: *mut u8,
            target_stride: c_int,
            target_width: c_int,
            target_height: c_int,
            clip_x: c_int,
            clip_y: c_int,
            clip_width: c_int,
            clip_height: c_int,
            filtering: c_int,
        ) -> c_int;
        /// Swaps the first and third byte of each pixel; may work in place.
        pub fn ABGRToARGB(
            source: *const u8,
            source_stride: c_int,
            target: *mut u8,
            target_stride: c_int,
            width: c_int,
            height: c_int,
        ) -> c_int;
    }
}

/// The renderer's hook (patch 13): a frame of ours, RGBA, copied or scaled
/// into the BGRA window, only inside the clip.
#[cfg(feature = "cpu")]
fn draw(draw: &mut iced_tiny_skia::video::Draw<'_>) -> bool {
    if !is_video(draw.source.as_ptr() as usize) {
        return false;
    }
    let [clip_x, clip_y, clip_width, clip_height] = draw.clip;
    if clip_width == 0 || clip_height == 0 || draw.width == 0 || draw.height == 0 {
        return true;
    }
    let stride = draw.target_width as usize * 4;
    let source_stride = draw.width as usize * 4;
    // The clip within the drawn rectangle.
    let (inner_x, inner_y) = (clip_x as i32 - draw.x, clip_y as i32 - draw.y);
    let at = clip_y as usize * stride + clip_x as usize * 4;
    let end = at + (clip_height as usize - 1) * stride + clip_width as usize * 4;
    if end > draw.target.len() {
        return true;
    }
    let target = draw.target.as_mut_ptr();
    // SAFETY: libyuv reads `clip_height` rows of `clip_width` pixels from
    // the source at the clip's place within the drawn rectangle, which the
    // source covers whole (`height` rows of `width` pixels) when drawn one
    // to one, or the whole source when scaled; it writes only the clip,
    // which lies inside the target (checked above). Source and target
    // don't overlap: the source is a frame's memory, the target the window.
    unsafe {
        if (draw.width, draw.height) == (draw.to_width, draw.to_height) {
            // One to one: the swap is the copy.
            let from = draw
                .source
                .as_ptr()
                .add(inner_y as usize * source_stride + inner_x as usize * 4);
            yuv::ABGRToARGB(
                from,
                source_stride as _,
                target.add(at),
                stride as _,
                clip_width as _,
                clip_height as _,
            );
        } else {
            // The scaled picture's corner, which may lie outside the target;
            // libyuv only writes the clip.
            let corner =
                target.wrapping_offset(draw.y as isize * stride as isize + draw.x as isize * 4);
            yuv::ARGBScaleClip(
                draw.source.as_ptr(),
                source_stride as _,
                draw.width as _,
                draw.height as _,
                corner,
                stride as _,
                draw.to_width as _,
                draw.to_height as _,
                inner_x,
                inner_y,
                clip_width as _,
                clip_height as _,
                yuv::BILINEAR,
            );
            yuv::ABGRToARGB(
                target.add(at),
                stride as _,
                target.add(at),
                stride as _,
                clip_width as _,
                clip_height as _,
            );
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Finding new frames

/// Where frames come from while they're on screen: each camera's feed, by
/// participant id, the watched stream, by its streamer's id, and the
/// video playing in a message, by `{item}/{index}`. The stream and the
/// message's video come through a worker's shared mapping.
#[derive(Default)]
pub struct Sources {
    pub feeds: Vec<(String, Feed)>,
    pub stream: Option<(String, Arc<Frames>)>,
    pub clip: Option<(String, Arc<Frames>)>,
}

impl Sources {
    pub fn is_empty(&self) -> bool {
        self.feeds.is_empty() && self.stream.is_none() && self.clip.is_none()
    }

    fn mapped(&self) -> bool {
        self.stream.is_some() || self.clip.is_some()
    }

    fn same(&self, other: &Sources) -> bool {
        let same =
            |a: &Option<(String, Arc<Frames>)>, b: &Option<(String, Arc<Frames>)>| match (a, b) {
                (None, None) => true,
                (Some((a, x)), Some((b, y))) => a == b && Arc::ptr_eq(x, y),
                _ => false,
            };
        self.feeds == other.feeds
            && same(&self.stream, &other.stream)
            && same(&self.clip, &other.clip)
    }
}

/// What the window and [`frames`] share.
#[derive(Default)]
pub struct Shared {
    sources: Mutex<Sources>,
    /// The stream's last frame went out and the window hasn't taken it in:
    /// its slot isn't taken back from the window until it has.
    stream_waiting: AtomicBool,
    /// The same for the message's video.
    clip_waiting: AtomicBool,
    changed: Notify,
}

/// A new frame for the window: whose, and its handle and size.
#[derive(Clone, Debug)]
pub struct Fresh {
    pub key: String,
    pub handle: Handle,
    pub size: (u32, u32),
    /// How far into a message's video the frame is, in milliseconds.
    pub time: Option<u64>,
}

impl Shared {
    /// Looks for frames from these from now on; none stops the task looking.
    pub fn set(&self, sources: Sources) {
        let mut current = self.sources.lock().expect("video sources");
        if current.same(&sources) {
            return;
        }
        *current = sources;
        self.stream_waiting.store(false, Ordering::Relaxed);
        self.clip_waiting.store(false, Ordering::Relaxed);
        self.changed.notify_one();
    }

    /// The window took the stream's frame in.
    pub fn stream_taken(&self) {
        self.stream_waiting.store(false, Ordering::Relaxed);
    }

    /// The window took the message video's frame in.
    pub fn clip_taken(&self) {
        self.clip_waiting.store(false, Ordering::Relaxed);
    }

    fn take(&self) -> Vec<Fresh> {
        let sources = self.sources.lock().expect("video sources");
        let mut found = Vec::new();
        for (key, feed) in &sources.feeds {
            let taken = feed.take(|buffer| {
                let frame = (buffer as &dyn Any).downcast_ref::<Frame>()?;
                Some((frame.handle(), frame.size))
            });
            if let Some(Some((handle, size))) = taken {
                found.push(Fresh {
                    key: key.clone(),
                    handle,
                    size,
                    time: None,
                });
            }
        }
        let mapped = [
            (&sources.stream, &self.stream_waiting, "stream"),
            (&sources.clip, &self.clip_waiting, "clip"),
        ];
        for (source, waiting, kind) in mapped {
            let Some((key, frames)) = source else {
                continue;
            };
            if waiting.load(Ordering::Relaxed) {
                continue;
            }
            let taken = frames.take_timed(|width, height, pixels| {
                let address = pixels.as_ptr() as usize;
                register(address);
                let bytes = SlotBytes {
                    _frames: frames.clone(),
                    address,
                    length: pixels.len(),
                };
                Handle::from_rgba(width, height, bytes::Bytes::from_owner(bytes))
            });
            if let Some((handle, time)) = taken {
                let size = match &handle {
                    raster::Handle::Rgba { width, height, .. } => (*width, *height),
                    _ => (0, 0),
                };
                waiting.store(true, Ordering::Relaxed);
                found.push(Fresh {
                    key: format!("{kind}:{key}"),
                    handle,
                    size,
                    time: Some(time),
                });
            }
        }
        found
    }
}

/// How often a watched stream is looked at while it shows: at 8 ms a frame
/// waits 4 ms on average, and the window redraws at most 125 times a second.
const STREAM_LOOK: Duration = Duration::from_millis(8);

/// New frames as they come, all those found together in one item.
pub fn frames(shared: Arc<Shared>) -> impl Stream<Item = Vec<Fresh>> + Send + 'static {
    stream::unfold(shared, |shared| async move {
        loop {
            let (idle, stream) = {
                let sources = shared.sources.lock().expect("video sources");
                (sources.is_empty(), sources.mapped())
            };
            if idle {
                shared.changed.notified().await;
                continue;
            }
            {
                let changed = pin!(shared.changed.notified());
                if stream {
                    future::select(changed, pin!(tokio::time::sleep(STREAM_LOOK))).await;
                } else {
                    future::select(changed, pin!(FRAME_MADE.notified())).await;
                }
            }
            let found = shared.take();
            if !found.is_empty() {
                return Some((found, shared));
            }
        }
    })
}

// ---------------------------------------------------------------------------
// A frame on screen

/// A video frame fitted inside its tile, keeping its shape (Slint's
/// `image-fit: contain`), the bars beside it in `ground`. The picture is
/// drawn by the renderer's hook: one to one when the core made it at the
/// tile's size, else scaled.
pub struct Video {
    handle: Handle,
    size: (u32, u32),
    ground: Color,
    /// The tile's round corners, cut by drawing what's behind the tile over
    /// them: the radius and the colour behind.
    corners: Option<(u32, u32)>,
}

impl Video {
    pub fn new(handle: Handle, size: (u32, u32), ground: Color) -> Self {
        Self {
            handle,
            size,
            ground,
            corners: None,
        }
    }

    pub fn corners(mut self, radius: u32, behind: u32) -> Self {
        self.corners = Some((radius, behind));
        self
    }
}

thread_local! {
    /// Each corner's picture, by radius, colour and corner, made once.
    static CORNERS: std::cell::RefCell<HashMap<(u32, u32, u8), Handle>> = Default::default();
}

/// A `radius` square, `behind` outside the quarter circle of a tile's
/// corner (0 top left, 1 top right, 2 bottom right, 3 bottom left) and clear
/// inside it, edged by coverage.
fn corner(radius: u32, behind: u32, which: u8) -> Handle {
    CORNERS.with_borrow_mut(|corners| {
        corners
            .entry((radius, behind, which))
            .or_insert_with(|| {
                let side = radius as usize;
                let mut pixels = vec![0u8; side * side * 4];
                let [_, r, g, b] = behind.to_be_bytes();
                for y in 0..side {
                    for x in 0..side {
                        // Distance from the arc's centre, in the top-left corner's terms.
                        let (cx, cy) = match which {
                            0 => (side - x, side - y),
                            1 => (x + 1, side - y),
                            2 => (x + 1, y + 1),
                            _ => (side - x, y + 1),
                        };
                        let (dx, dy) = (cx as f32 - 0.5, cy as f32 - 0.5);
                        let outside =
                            ((dx * dx + dy * dy).sqrt() - radius as f32 + 0.5).clamp(0.0, 1.0);
                        let scale = |c: u8| (f32::from(c) * outside).round() as u8;
                        let at = (y * side + x) * 4;
                        // Premultiplied BGRA, as the port's pictures are drawn.
                        pixels[at..at + 4].copy_from_slice(&[
                            scale(b),
                            scale(g),
                            scale(r),
                            (outside * 255.0).round() as u8,
                        ]);
                    }
                }
                paint::finish(&mut pixels);
                Handle::from_rgba(radius, radius, pixels)
            })
            .clone()
    })
}

impl Widget<Message, Theme, Renderer> for Video {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::{Renderer as _, image::Renderer as _};
        let tile = layout.bounds();
        let (width, height) = (self.size.0.max(1) as f32, self.size.1.max(1) as f32);
        let mut scale = (tile.width / width).min(tile.height / height);
        // The core makes a camera's frame at the tile's size, rounded down
        // to even sides: up to two pixels short, it's drawn one to one, in
        // the middle, not stretched by a fraction.
        if scale > 1.0 && tile.width - width <= 2.0 && tile.height - height <= 2.0 {
            scale = 1.0;
        }
        // Whole pixels: the frame was made at a whole size, so one to one
        // stays one to one.
        let (drawn_width, drawn_height) = ((width * scale).round(), (height * scale).round());
        let x = (tile.x + (tile.width - drawn_width) / 2.0).round();
        let y = (tile.y + (tile.height - drawn_height) / 2.0).round();
        let picture = Rectangle::new(iced::Point::new(x, y), Size::new(drawn_width, drawn_height));
        // The bars only, so the frame's pixels are written once.
        let bars = [
            Rectangle::new(tile.position(), Size::new(tile.width, picture.y - tile.y)),
            Rectangle::new(
                iced::Point::new(tile.x, picture.y + picture.height),
                Size::new(
                    tile.width,
                    tile.y + tile.height - picture.y - picture.height,
                ),
            ),
            Rectangle::new(
                iced::Point::new(tile.x, picture.y),
                Size::new(picture.x - tile.x, picture.height),
            ),
            Rectangle::new(
                iced::Point::new(picture.x + picture.width, picture.y),
                Size::new(
                    tile.x + tile.width - picture.x - picture.width,
                    picture.height,
                ),
            ),
        ];
        for bar in bars {
            if bar.width >= 0.5 && bar.height >= 0.5 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: bar,
                        ..Default::default()
                    },
                    self.ground,
                );
            }
        }
        renderer.draw_image(
            raster::Image {
                handle: self.handle.clone(),
                filter_method: raster::FilterMethod::Linear,
                rotation: iced::Radians(0.0),
                border_radius: Default::default(),
                opacity: 1.0,
                snap: true,
            },
            picture,
            picture,
        );
        if let Some((radius, behind)) = self.corners {
            let side = radius as f32;
            let at = [
                (tile.x, tile.y),
                (tile.x + tile.width - side, tile.y),
                (tile.x + tile.width - side, tile.y + tile.height - side),
                (tile.x, tile.y + tile.height - side),
            ];
            for (which, (x, y)) in at.into_iter().enumerate() {
                let bounds = Rectangle::new(
                    iced::Point::new(x.round(), y.round()),
                    Size::new(side, side),
                );
                renderer.draw_image(
                    raster::Image {
                        handle: corner(radius, behind, which as u8),
                        filter_method: raster::FilterMethod::Linear,
                        rotation: iced::Radians(0.0),
                        border_radius: Default::default(),
                        opacity: 1.0,
                        snap: true,
                    },
                    bounds,
                    bounds,
                );
            }
        }
    }
}

impl<'a> From<Video> for Element<'a, Message> {
    fn from(video: Video) -> Self {
        Element::new(video)
    }
}
