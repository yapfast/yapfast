//! A screen or window as pictures. ScreenCaptureKit pushes a frame only when
//! something on the screen changed, so the newest one is kept in a slot, and
//! whoever shares takes it at the stream's own pace, again and again while
//! nothing changes: a still screen still needs a picture every frame, or the
//! stream stalls and a viewer that joins gets nothing.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use objc2::{AnyThread, rc::Retained};
use objc2_core_media::CMSampleBuffer;
use objc2_foundation::NSArray;
use objc2_screen_capture_kit::{SCContentFilter, SCStreamOutputType, SCWindow};

use super::{
    content::{Capturable, Content, display_pixels},
    stream::{Settings, Stream},
};
use crate::{nv12, picture::I420};

/// A picture, its planes without any padding between rows.
#[derive(Default)]
pub(crate) struct Frame {
    width: u32,
    height: u32,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

impl Frame {
    /// Whether no picture was ever put in.
    pub(crate) fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The picture, to hand on.
    pub(crate) fn picture(&self) -> I420<'_> {
        let chroma_width = self.width.div_ceil(2);
        I420 {
            width: self.width,
            height: self.height,
            y: &self.y,
            u: &self.u,
            v: &self.v,
            strides: [self.width, chroma_width, chroma_width],
        }
    }

    /// Takes a copy of `picture`, reusing what this frame holds.
    fn fill(&mut self, picture: &I420<'_>) {
        let (width, height) = (picture.width as usize, picture.height as usize);
        let chroma_len = width.div_ceil(2) * height.div_ceil(2);
        self.width = picture.width;
        self.height = picture.height;
        self.y.clear();
        self.y.reserve(width * height);
        for row in picture.y.chunks(picture.strides[0] as usize).take(height) {
            self.y.extend_from_slice(&row[..width.min(row.len())]);
        }
        copy_prefix(&mut self.u, picture.u, chroma_len);
        copy_prefix(&mut self.v, picture.v, chroma_len);
    }
}

/// Makes `into` the first `len` bytes of `from`, or all of it if shorter.
fn copy_prefix(into: &mut Vec<u8>, from: &[u8], len: usize) {
    into.clear();
    into.extend_from_slice(&from[..len.min(from.len())]);
}

/// The newest frame, and whether it was not taken yet.
#[derive(Default)]
struct Slot {
    frame: Frame,
    fresh: bool,
    /// How many frames the system sent in all.
    sent: u64,
}

type Shared = Arc<Mutex<Slot>>;

// ---- Pure logic: the size to capture at ----

/// How many pixels a point is: a display's pixels over its points, or one
/// where either is unknown.
pub(crate) fn pixel_scale(pixels: u32, points: f64) -> f64 {
    if points > 0.0 && pixels > 0 {
        f64::from(pixels) / points
    } else {
        1.0
    }
}

/// The size to capture content of `points` at: in pixels, fit into `max`
/// with an even size, as codecs want.
pub(crate) fn capture_size(points: (f64, f64), scale: f64, max: (u32, u32)) -> (u32, u32) {
    let pixels = |points: f64| (points * scale).round().max(1.0) as u32;
    crate::stream_worker::fit(pixels(points.0), pixels(points.1), max.0, max.1)
}

// ---- Capturing ----

/// A screen or window being captured until dropped.
pub(crate) struct Capture {
    _stream: Stream,
    shared: Shared,
}

impl Capture {
    /// Starts capturing `what` at `fps`, in pictures that fit `max`. `on_stop`
    /// hears why, if the system ends the capture.
    pub(crate) fn start(
        what: Capturable,
        max: (u32, u32),
        fps: u32,
        on_stop: impl Fn(String) + Send + Sync + 'static,
    ) -> Result<Self> {
        let content = Content::fetch()?;
        let (filter, size) = filter_for(&content, what, max)?;
        let shared = Shared::default();
        let stream = Stream::start(
            &filter,
            Settings {
                width: size.0,
                height: size.1,
                fps,
                audio: false,
            },
            sample_handler(shared.clone()),
            Box::new(on_stop),
        )?;
        Ok(Self {
            _stream: stream,
            shared,
        })
    }

    /// How many frames the system has sent since the capture began.
    #[cfg(test)]
    pub(crate) fn frames_sent(&self) -> u64 {
        self.shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .sent
    }

    /// Puts the newest frame into `current` if a new one came since the last
    /// call, and says whether `current` holds any picture at all.
    pub(crate) fn take(&self, current: &mut Frame) -> bool {
        let mut slot = self
            .shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.fresh {
            std::mem::swap(&mut slot.frame, current);
            slot.fresh = false;
        }
        !current.is_empty()
    }
}

/// What ScreenCaptureKit captures for `what`, and the size of its pictures.
fn filter_for(
    content: &Content,
    what: Capturable,
    max: (u32, u32),
) -> Result<(Retained<SCContentFilter>, (u32, u32))> {
    match what {
        Capturable::Window(id) => window_filter(content, id, max),
        _ => display_filter(content, what, max),
    }
}

fn display_filter(
    content: &Content,
    what: Capturable,
    max: (u32, u32),
) -> Result<(Retained<SCContentFilter>, (u32, u32))> {
    let display = content.display(what)?;
    // SAFETY: a plain property read on a listed display.
    let id = unsafe { display.displayID() };
    let (pixels_wide, pixels_high) = display_pixels(id);
    let size = crate::stream_worker::fit(pixels_wide, pixels_high, max.0, max.1);
    let none: Retained<NSArray<SCWindow>> = NSArray::new();
    // SAFETY: the display and the array are live.
    let filter = unsafe {
        SCContentFilter::initWithDisplay_excludingWindows(SCContentFilter::alloc(), &display, &none)
    };
    tracing::info!(id, ?size, "capturing a display");
    Ok((filter, size))
}

fn window_filter(
    content: &Content,
    id: u32,
    max: (u32, u32),
) -> Result<(Retained<SCContentFilter>, (u32, u32))> {
    let window = content.window(id)?;
    // A window's size is in points: the main display says how many pixels
    // one is.
    let main = content
        .display(Capturable::MainDisplay)
        .context("There is no screen to capture a window from.")?;
    // SAFETY: plain property reads on listed objects.
    let (frame, main_id, main_points) =
        unsafe { (window.frame(), main.displayID(), main.frame().size.width) };
    let scale = pixel_scale(display_pixels(main_id).0, main_points);
    let size = capture_size((frame.size.width, frame.size.height), scale, max);
    // SAFETY: the window is live.
    let filter = unsafe {
        SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
    };
    tracing::info!(id, ?size, "capturing a window");
    Ok((filter, size))
}

/// What the stream does with each sample: keeps a picture's frame.
fn sample_handler(shared: Shared) -> super::stream::SampleHandler {
    let mut chroma = nv12::Chroma::default();
    let mut warned = false;
    Box::new(move |kind: SCStreamOutputType, sample: &CMSampleBuffer| {
        if kind != SCStreamOutputType::Screen {
            return;
        }
        // A sample without a picture says only that the screen did not change.
        // SAFETY: a plain getter; the sample stays retained for this call.
        let Some(image) = (unsafe { sample.image_buffer() }) else {
            return;
        };
        let kept = nv12::with_i420(&image, &mut chroma, |picture| {
            let mut slot = shared
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            slot.frame.fill(picture);
            slot.fresh = true;
            slot.sent += 1;
        });
        match kept {
            Ok(()) => {}
            Err(nv12::Nv12Error::NotNv12) if !warned => {
                warned = true;
                tracing::warn!("screen frames are not NV12; they are dropped");
            }
            Err(nv12::Nv12Error::NotNv12) => {}
            Err(error) => tracing::warn!(?error, "could not read a screen frame"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retina_display_has_two_pixels_to_the_point() {
        assert_eq!(pixel_scale(3024, 1512.0), 2.0);
        assert_eq!(pixel_scale(1920, 1920.0), 1.0);
    }

    #[test]
    fn an_unknown_size_scales_by_one() {
        assert_eq!(pixel_scale(0, 1512.0), 1.0);
        assert_eq!(pixel_scale(3024, 0.0), 1.0);
    }

    #[test]
    fn a_window_is_captured_in_pixels_that_fit_and_are_even() {
        assert_eq!(capture_size((500.0, 300.0), 2.0, (1920, 1080)), (1000, 600));
        // Larger than the stream: scaled down, keeping its shape.
        assert_eq!(
            capture_size((1920.0, 1200.0), 2.0, (1920, 1080)),
            (1728, 1080)
        );
        // Odd sizes lose a pixel.
        assert_eq!(capture_size((501.0, 301.0), 1.0, (1920, 1080)), (500, 300));
    }

    #[test]
    fn a_frame_drops_row_padding_and_keeps_its_picture() {
        // 2x2 luma with rows padded to 4 bytes; one chroma sample each.
        let picture = I420 {
            width: 2,
            height: 2,
            y: &[1, 2, 0, 0, 3, 4, 0, 0],
            u: &[5],
            v: &[6],
            strides: [4, 1, 1],
        };
        let mut frame = Frame::default();
        assert!(frame.is_empty());
        frame.fill(&picture);
        assert!(!frame.is_empty());
        let kept = frame.picture();
        assert_eq!((kept.width, kept.height), (2, 2));
        assert_eq!(kept.y, [1, 2, 3, 4]);
        assert_eq!((kept.u, kept.v), (&[5][..], &[6][..]));
        assert_eq!(kept.strides, [2, 1, 1]);
    }

    /// How many frames the hardware test waits for.
    const HARDWARE_FRAMES: u64 = 30;
    const HARDWARE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

    /// Lists the displays and windows and captures 30 frames of the main
    /// display, to see on a Mac with the permission granted that capture works.
    /// Run with `--ignored --nocapture`; a still screen sends few frames, so
    /// move something on it.
    #[test]
    #[ignore = "needs a screen and the Screen Recording permission"]
    fn captures_thirty_frames_of_the_main_display() {
        let listing = super::super::content::list().expect("list screens and windows");
        eprintln!("screens: {:?}", listing.screens);
        eprintln!("windows: {}", listing.windows.len());
        for (_, title) in listing.windows.iter().take(5) {
            eprintln!("  window: {title}");
        }
        let capture = Capture::start(Capturable::MainDisplay, (1920, 1080), 30, |message| {
            eprintln!("capture ended: {message}");
        })
        .expect("start capturing the main display");
        let started = std::time::Instant::now();
        let mut current = Frame::default();
        while capture.frames_sent() < HARDWARE_FRAMES && started.elapsed() < HARDWARE_TIMEOUT {
            capture.take(&mut current);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let (frames, seconds) = (capture.frames_sent(), started.elapsed().as_secs_f64());
        capture.take(&mut current);
        let picture = current.picture();
        eprintln!(
            "{frames} frames of {}x{} in {seconds:.2} s: {:.1} fps",
            picture.width,
            picture.height,
            frames as f64 / seconds
        );
        assert!(frames >= HARDWARE_FRAMES, "only {frames} frames came");
    }
}
