//! Moving pictures shown in the window, such as a camera in a call: each is
//! made at the size it's shown, straight into memory the UI shows without a
//! copy, in a few buffers [`Feed`] reuses frame after frame.

use std::{
    any::Any,
    ffi::{c_int, c_uchar},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// A picture's planes as native code lays them out; see `native/picture.c`.
#[repr(C)]
pub(crate) struct RawPicture {
    pub width: c_int,
    pub height: c_int,
    pub y: *const c_uchar,
    pub u: *const c_uchar,
    pub v: *const c_uchar,
    pub y_stride: c_int,
    pub u_stride: c_int,
    pub v_stride: c_int,
}

unsafe extern "C" {
    fn yapfast_i420_to_rgba(
        picture: *const RawPicture,
        scratch: *mut c_uchar,
        rgba: *mut c_uchar,
        stride: c_int,
        width: c_int,
        height: c_int,
    );
}

/// An I420 picture's planes, borrowed from whoever made it.
#[derive(Clone, Copy)]
pub struct I420<'a> {
    pub width: u32,
    pub height: u32,
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    /// Bytes a row of luma, then of each chroma plane.
    pub strides: [u32; 3],
}

impl<'a> I420<'a> {
    /// The planes `raw` points at, or none for an empty picture.
    ///
    /// # Safety
    /// `raw`'s planes must hold its height in rows of luma and half as many
    /// of each chroma plane, at its strides, for `'a`.
    #[cfg(any(target_os = "linux", windows))]
    pub(crate) unsafe fn from_raw(raw: &RawPicture) -> Option<Self> {
        if raw.width <= 0
            || raw.height <= 0
            || raw.y_stride <= 0
            || raw.u_stride <= 0
            || raw.v_stride <= 0
        {
            return None;
        }
        let (width, height) = (raw.width as u32, raw.height as u32);
        let chroma = height.div_ceil(2) as usize;
        // SAFETY: as the caller promised.
        let plane = |data: *const c_uchar, stride: c_int, rows: usize| unsafe {
            std::slice::from_raw_parts(data, stride as usize * rows)
        };
        Some(Self {
            width,
            height,
            y: plane(raw.y, raw.y_stride, height as usize),
            u: plane(raw.u, raw.u_stride, chroma),
            v: plane(raw.v, raw.v_stride, chroma),
            strides: [
                raw.y_stride as u32,
                raw.u_stride as u32,
                raw.v_stride as u32,
            ],
        })
    }

    pub(crate) fn raw(&self) -> RawPicture {
        RawPicture {
            width: self.width as c_int,
            height: self.height as c_int,
            y: self.y.as_ptr(),
            u: self.u.as_ptr(),
            v: self.v.as_ptr(),
            y_stride: self.strides[0] as c_int,
            u_stride: self.strides[1] as c_int,
            v_stride: self.strides[2] as c_int,
        }
    }

    /// Writes the picture scaled to `width` by `height` as RGBA into `rgba`,
    /// `width * 4` bytes a row, scaling through `scratch`, which it sizes
    /// once and keeps for the next picture.
    pub fn to_rgba(&self, scratch: &mut Vec<u8>, rgba: &mut [u8], width: u32, height: u32) {
        assert!(rgba.len() >= width as usize * height as usize * 4);
        // libyuv reads each plane at its stride, trusting it to be there.
        let (w, h) = (self.width as usize, self.height as usize);
        let rows = [h, h.div_ceil(2), h.div_ceil(2)];
        let widths = [w, w.div_ceil(2), w.div_ceil(2)];
        for (index, plane) in [self.y, self.u, self.v].into_iter().enumerate() {
            let stride = self.strides[index] as usize;
            assert!(stride >= widths[index] && plane.len() >= stride * rows[index]);
        }
        if (width, height) != (self.width, self.height) {
            let chroma = width.div_ceil(2) as usize * height.div_ceil(2) as usize;
            scratch.resize(width as usize * height as usize + 2 * chroma, 0);
        }
        let raw = self.raw();
        // SAFETY: the planes outlive the call, `scratch` holds an I420
        // picture of the target size when it's needed, and `rgba` its RGBA.
        unsafe {
            yapfast_i420_to_rgba(
                &raw,
                scratch.as_mut_ptr(),
                rgba.as_mut_ptr(),
                (width * 4) as c_int,
                width as c_int,
                height as c_int,
            );
        }
    }
}

/// The size a `width` by `height` picture takes to fit inside `bounds`,
/// keeping its shape, never larger than it is, at even sizes.
pub fn fit(width: u32, height: u32, (max_width, max_height): (u32, u32)) -> (u32, u32) {
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(max_width) / f64::from(width.max(1)),
            f64::from(max_height) / f64::from(height.max(1)),
        ),
    );
    let even = |value: f64| ((value as u32) & !1).max(2);
    (
        even(f64::from(width) * scale),
        even(f64::from(height) * scale),
    )
}

/// Memory a frame's RGBA pixels are made in, which the UI shows as it is.
pub trait FrameBuffer: Any + Send {
    fn size(&self) -> (u32, u32);
    /// The pixels to write, `width * 4` bytes a row.
    fn pixels_mut(&mut self) -> &mut [u8];
}

/// A frame buffer without a UI: a plain vector.
struct Plain {
    size: (u32, u32),
    pixels: Vec<u8>,
}

impl FrameBuffer for Plain {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }
}

/// Makes a buffer for a `width` by `height` frame.
pub type MakeFrame = fn(u32, u32) -> Box<dyn FrameBuffer>;

static MAKE_FRAME: OnceLock<MakeFrame> = OnceLock::new();

/// Makes every frame buffer with `make` from now on: the UI's own kind, so
/// it shows them without a copy.
pub fn set_frame_buffers(make: MakeFrame) {
    let _ = MAKE_FRAME.set(make);
}

fn make_frame(width: u32, height: u32) -> Box<dyn FrameBuffer> {
    match MAKE_FRAME.get() {
        Some(make) => make(width, height),
        None => Box::new(Plain {
            size: (width, height),
            pixels: vec![0; width as usize * height as usize * 4],
        }),
    }
}

/// The most buffers a feed keeps: one being written, one waiting for the
/// UI, the one on screen, and the one before it, which the renderer may
/// still hold until it draws the new one.
const BUFFERS: usize = 4;

/// One place on screen a moving picture shows, such as someone's tile in a
/// call: the size the UI shows it at, and its newest frame. Whoever makes
/// the pictures writes frames at that size; the UI takes the newest when it
/// draws. Neither waits on the other for longer than a swap.
#[derive(Clone, Default)]
pub struct Feed(Arc<Shared>);

#[derive(Default)]
struct Shared {
    /// The size shown, in pixels, as width and height in a word; zero while
    /// it isn't on screen.
    shown_at: AtomicU64,
    /// Whether it shows large, as the call's focused tile.
    large: AtomicBool,
    buffers: Mutex<Buffers>,
    /// Told whenever the size shown changes, or whether it's large, for a
    /// sender to match.
    shown: Option<Arc<tokio::sync::Notify>>,
}

#[derive(Default)]
struct Buffers {
    /// Written and not taken yet.
    fresh: Option<Box<dyn FrameBuffer>>,
    /// On screen, and the one before it.
    shown: Option<Box<dyn FrameBuffer>>,
    retired: Option<Box<dyn FrameBuffer>>,
    spare: Vec<Box<dyn FrameBuffer>>,
    /// Buffers made, including the one being written.
    made: usize,
}

impl Buffers {
    fn give_back(&mut self, buffer: Box<dyn FrameBuffer>) {
        self.spare.push(buffer);
    }
}

impl std::fmt::Debug for Feed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Feed")
            .field("shown_at", &self.shown_at())
            .finish_non_exhaustive()
    }
}

impl PartialEq for Feed {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Feed {}

impl Feed {
    pub fn new() -> Self {
        Self::default()
    }

    /// A feed that tells `shown` whenever the size it shows at changes, or
    /// whether it shows large, so whoever sends its pictures can send them
    /// to match, or stop.
    pub fn watched(shown: Arc<tokio::sync::Notify>) -> Self {
        Self(Arc::new(Shared {
            shown: Some(shown),
            ..Shared::default()
        }))
    }

    /// The UI shows the feed at `width` by `height` pixels, or nowhere.
    pub fn show_at(&self, size: Option<(u32, u32)>) {
        let word = size.map_or(0, |(width, height)| {
            u64::from(width) << 32 | u64::from(height)
        });
        if self.0.shown_at.swap(word, Ordering::Relaxed) != word {
            if size.is_none() {
                self.release();
            }
            if let Some(shown) = &self.0.shown {
                shown.notify_one();
            }
        }
    }

    /// The UI shows the feed large, as the tile in focus, or not.
    pub fn show_large(&self, large: bool) {
        if self.0.large.swap(large, Ordering::Relaxed) != large
            && let Some(shown) = &self.0.shown
        {
            shown.notify_one();
        }
    }

    /// Whether the UI shows the feed large.
    pub fn shown_large(&self) -> bool {
        self.0.large.load(Ordering::Relaxed)
    }

    /// The size the UI shows the feed at, in pixels, while it's on screen.
    pub fn shown_at(&self) -> Option<(u32, u32)> {
        let word = self.0.shown_at.load(Ordering::Relaxed);
        (word != 0).then_some(((word >> 32) as u32, word as u32))
    }

    /// The size a `width` by `height` picture is made at for this feed:
    /// fitted inside where it shows. None while it isn't on screen.
    pub fn size_for(&self, width: u32, height: u32) -> Option<(u32, u32)> {
        self.shown_at().map(|bounds| fit(width, height, bounds))
    }

    /// Makes the next frame, `width` by `height` RGBA pixels, with `fill`.
    pub fn write(&self, width: u32, height: u32, fill: impl FnOnce(&mut [u8])) {
        let reuse = {
            let mut buffers = self.lock();
            // A frame the UI hasn't taken was never on screen, so it's free.
            let reused = buffers.fresh.take().or_else(|| buffers.spare.pop());
            match reused {
                Some(buffer) if buffer.size() == (width, height) => Some(buffer),
                // The size changed: that buffer goes, and a new one comes.
                Some(_) => None,
                None if buffers.made >= BUFFERS => {
                    // Every buffer is on screen or about to be; drop this frame.
                    return;
                }
                None => {
                    buffers.made += 1;
                    None
                }
            }
        };
        let mut buffer = reuse.unwrap_or_else(|| make_frame(width, height));
        fill(buffer.pixels_mut());
        let mut buffers = self.lock();
        // Released while this one was written: it counts again.
        buffers.made = buffers.made.max(1);
        if let Some(older) = buffers.fresh.replace(buffer) {
            buffers.give_back(older);
        }
    }

    /// Writes each picture it's given into this feed, at the size it shows.
    /// Off screen, pictures go nowhere.
    pub fn sink(&self) -> impl FnMut(&I420<'_>) + Send + 'static {
        self.sink_with(false)
    }

    /// As [`Self::sink`], mirrored, as people see themselves: a camera
    /// filming its own person shows them as a mirror would.
    pub fn mirrored_sink(&self) -> impl FnMut(&I420<'_>) + Send + 'static {
        self.sink_with(true)
    }

    fn sink_with(&self, mirror: bool) -> impl FnMut(&I420<'_>) + Send + 'static {
        let feed = self.clone();
        let mut scratch = vec![];
        move |picture| {
            if let Some((width, height)) = feed.size_for(picture.width, picture.height) {
                feed.write(width, height, |rgba| {
                    picture.to_rgba(&mut scratch, rgba, width, height);
                    if mirror {
                        for row in rgba.chunks_exact_mut(width as usize * 4) {
                            row.as_chunks_mut::<4>().0.reverse();
                        }
                    }
                });
            }
        }
    }

    /// Calls `show` with the newest frame, if one came since the last call.
    /// The UI shows it until the next frame it takes.
    pub fn take<T>(&self, show: impl FnOnce(&dyn FrameBuffer) -> T) -> Option<T> {
        let mut buffers = self.lock();
        let fresh = buffers.fresh.take()?;
        let shown = show(fresh.as_ref());
        // The one before last can't be on screen any more.
        let retired = buffers.shown.replace(fresh);
        if let Some(gone) = std::mem::replace(&mut buffers.retired, retired) {
            buffers.give_back(gone);
        }
        Some(shown)
    }

    /// Lets go of every buffer, as when the picture leaves the screen. The
    /// UI keeps its own handle on what it shows.
    pub fn release(&self) {
        *self.lock() = Buffers::default();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Buffers> {
        self.0
            .buffers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(buffer: &dyn FrameBuffer) -> u8 {
        (buffer as &dyn Any)
            .downcast_ref::<Plain>()
            .map_or(0, |plain| plain.pixels[0])
    }

    #[test]
    fn the_newest_frame_shows_and_buffers_come_round_again() {
        let feed = Feed::new();
        assert_eq!(feed.size_for(1280, 720), None, "not on screen");
        feed.show_at(Some((320, 240)));
        assert_eq!(feed.size_for(1280, 720), Some((320, 180)));
        assert!(feed.take(first).is_none());
        feed.write(2, 2, |pixels| pixels.fill(1));
        feed.write(2, 2, |pixels| pixels.fill(2));
        assert_eq!(feed.take(first), Some(2), "only the newest");
        assert!(feed.take(first).is_none(), "taken once");
        for value in 3..20 {
            feed.write(2, 2, |pixels| pixels.fill(value));
            assert_eq!(feed.take(first), Some(value));
        }
        assert!(feed.lock().made <= BUFFERS, "{} buffers", feed.lock().made);
        // Off screen, the buffers go.
        feed.show_at(None);
        assert_eq!(feed.lock().made, 0);
        assert!(feed.lock().shown.is_none());
    }

    #[test]
    fn a_watched_feed_says_when_its_size_changes() {
        let shown = Arc::new(tokio::sync::Notify::new());
        let feed = Feed::watched(shown.clone());
        let told = || futures_util::FutureExt::now_or_never(shown.notified()).is_some();
        feed.show_at(None);
        assert!(!told(), "nothing changed");
        feed.show_at(Some((320, 180)));
        assert!(told());
        feed.show_at(Some((320, 180)));
        assert!(!told(), "the same size");
        feed.show_at(None);
        assert!(told(), "scrolled away");
        feed.show_large(true);
        assert!(told(), "in focus");
        feed.show_large(true);
        assert!(!told(), "still in focus");
    }

    #[test]
    fn a_new_size_replaces_the_buffers() {
        let feed = Feed::new();
        feed.write(2, 2, |pixels| pixels.fill(1));
        feed.write(4, 2, |pixels| {
            assert_eq!(pixels.len(), 32);
            pixels.fill(5);
        });
        assert_eq!(feed.take(|buffer| buffer.size()), Some((4, 2)));
    }

    #[test]
    fn converts_and_scales_to_rgba() {
        // A 4 by 2 mid-grey picture, shown at 2 by 2.
        let (y, u, v) = ([126u8; 8], [128u8; 2], [128u8; 2]);
        let picture = I420 {
            width: 4,
            height: 2,
            y: &y,
            u: &u,
            v: &v,
            strides: [4, 2, 2],
        };
        let (mut scratch, mut rgba) = (vec![], vec![0; 16]);
        picture.to_rgba(&mut scratch, &mut rgba, 2, 2);
        assert!(
            rgba.chunks(4)
                .all(|pixel| pixel[0].abs_diff(128) < 4 && pixel[3] == 255),
            "{rgba:?}"
        );
        assert_eq!(fit(1920, 1080, (640, 640)), (640, 360));
    }
}
