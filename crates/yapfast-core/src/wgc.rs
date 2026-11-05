//! Windows Graphics Capture as textures: native/wgc.cc opens a capture of a
//! display and hands out one borrowed `ID3D11Texture2D` per frame, so a screen
//! reaches an encoder on the GPU without its pixels crossing the processor.
//!
//! It is the other end of [`crate::d3d11`]: that encoder reads D3D11 textures,
//! and this is a capture source that makes them. A frame therefore goes from the
//! compositor to the driver untouched, which is the difference between about 5.4
//! ms of the processor per 3440x1440 frame and about 0.8 as the spike that
//! proved the path measured it (spike/wgc-nvenc/RESULTS.md).
//!
//! Nothing here is wired into a stream yet: the capture loop that should hold a
//! [`Capture`], poll it and hand frames to an encoder is the next stage. What
//! this already fixes is the shape of it, which is one texture borrowed per
//! frame and let go by the next call — not a queue, and nothing per frame that
//! allocates.
//!
//! Two things a caller has to know that the shim can't enforce: the texture is
//! the pool's, so it lives only until the next [`Capture::frame`] or the
//! `Capture` being dropped, and the device a frame's texture is on is
//! [`Capture::device`], which an encoder has to open its session on to read it.

use std::ffi::{c_char, c_int, c_uint, c_void};

use anyhow::{Result, bail};

/// A capture as the shim owns it.
#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

// SAFETY: the shim's own declarations; every pointer passed is one it returned,
// and the buffers are as large as their sizes say.
unsafe extern "C" {
    fn yapfast_wgc_open(monitor: *mut c_void, why: *mut c_char, why_size: usize) -> *mut Raw;
    fn yapfast_wgc_frame(
        capture: *mut Raw,
        texture: *mut *mut c_void,
        why: *mut c_char,
        why_size: usize,
    ) -> c_int;
    fn yapfast_wgc_content_size(capture: *mut Raw, width: *mut c_uint, height: *mut c_uint);
    fn yapfast_wgc_device(capture: *mut Raw) -> *mut c_void;
    fn yapfast_wgc_describe(capture: *mut Raw, text: *mut c_char, text_size: usize);
    fn yapfast_wgc_close(capture: *mut Raw);
}

/// What asking a [`Capture`] for its next frame found. A screen that hasn't
/// changed produces nothing, and a capture whose size changed has to be rebuilt
/// before its frames are usable, so neither of those is a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrival {
    /// A frame arrived: [`Capture::texture`] is its picture, at
    /// [`Capture::content_size`].
    Frame,
    /// Nothing has been composited since the last frame: ask again, after a
    /// sleep rather than at once, since this is the compositor's own rate.
    Idle,
    /// The thing captured was resized, and the shim has already rebuilt its pool
    /// at the new size, which [`Capture::content_size`] reports. An encoder built
    /// for the old size reads the next frame wrongly, so it is rebuilt here and
    /// the next frame is asked for at the new dimensions.
    Resized,
    /// What was being captured closed, or its session ended. The capture is over:
    /// it hands out no more frames and should be dropped. This is how a window
    /// capture ends when its owner quits, so it is not an error.
    Closed,
}

/// The shim's answers to [`Capture::frame`], as native/wgc.cc numbers them.
const CLOSED: c_int = -1;
const FAILED: c_int = -2;
const IDLE: c_int = 0;
const FRAME: c_int = 1;
const RESIZED: c_int = 2;

/// Which display to capture: the platform's own handle for a monitor, as its
/// display enumeration hands them out, or [`Monitor::PRIMARY`] for the screen the
/// desktop starts on.
///
/// It is an identity and not memory this crate reads: the shim compares it
/// against DXGI's outputs to find the adapter that drives it, and hands it to the
/// capture API. A handle for a display that has gone away is answered with a
/// failure naming what the machine does have, never with a read of the handle.
#[derive(Clone, Copy, Debug)]
pub struct Monitor(*mut c_void);

impl Monitor {
    /// The display `handle` names, which comes from whichever enumeration the
    /// caller already has — `EnumDisplayMonitors`, `MonitorFromPoint`, or the
    /// UI's own list.
    pub const fn new(handle: *mut c_void) -> Self {
        Self(handle)
    }

    /// The primary display, which is where a caller that hasn't chosen a screen
    /// yet starts.
    pub const PRIMARY: Self = Self(std::ptr::null_mut());
}

/// A capture of a display, delivering BGRA textures on one D3D11 device.
///
/// It owns the frame pool and everything under it, and lets them go when it is
/// dropped: one texture borrowed at a time, released by the next frame or by the
/// drop, so a stopped stream stops holding a compositor buffer.
pub struct Capture {
    raw: *mut Raw,
    /// The texture the last [`Arrival::Frame`] delivered, which the shim keeps
    /// until the next [`Capture::frame`]: held here so that asking for it again
    /// doesn't have to disturb the capture, and null between frames.
    texture: *mut c_void,
}

// SAFETY: the pool is created free-threaded, so it has no thread affinity and
// `TryGetNextFrame` may be called from any thread; one thread uses a capture at a
// time, and nothing in the shim keeps state that a move would invalidate.
unsafe impl Send for Capture {}

impl Capture {
    /// Opens a capture of `monitor`, and starts it.
    ///
    /// The shim finds the adapter that drives that display and makes its device
    /// there, since a capture on another adapter's device would fill textures the
    /// encoder can't read.
    pub fn open(monitor: Monitor) -> Result<Self> {
        let mut why = [0u8; 256];
        // SAFETY: the buffer is as large as its size says, and the shim ends it
        // with a zero. The monitor handle is only compared, never read.
        let raw = unsafe { yapfast_wgc_open(monitor.0, why.as_mut_ptr().cast(), why.len()) };
        if raw.is_null() {
            bail!("no Windows Graphics Capture opened: {}", text(&why));
        }
        Ok(Self {
            raw,
            texture: std::ptr::null_mut(),
        })
    }

    /// Asks for the next frame, of the compositor's making: one call is one look,
    /// and [`Arrival::Idle`] means nothing has been drawn since the last one.
    ///
    /// A frame the previous call delivered is let go here, so its texture must
    /// not be read after this returns.
    pub fn frame(&mut self) -> Result<Arrival> {
        let mut why = [0u8; 256];
        let mut texture = std::ptr::null_mut();
        // SAFETY: raw is an open capture, the texture pointer and buffer are
        // places of their own, and they stay put during the call.
        let found = unsafe {
            yapfast_wgc_frame(self.raw, &mut texture, why.as_mut_ptr().cast(), why.len())
        };
        self.texture = std::ptr::null_mut();
        match found {
            FRAME => {
                self.texture = texture;
                Ok(Arrival::Frame)
            }
            IDLE => Ok(Arrival::Idle),
            RESIZED => Ok(Arrival::Resized),
            CLOSED => Ok(Arrival::Closed),
            FAILED => bail!("the capture stopped, having found no frame: {}", text(&why)),
            // The shim and this file disagreeing about the numbers is the two
            // sides of the boundary being out of step, which is worth refusing
            // rather than reading as one of the answers above.
            other => bail!("the capture answered {other}, which native/wgc.cc doesn't have"),
        }
    }

    /// The picture [`Arrival::Frame`] brought: an `ID3D11Texture2D` on
    /// [`Capture::device`], in BGRA, **borrowed** — the pool's, valid until the
    /// next [`Capture::frame`] or this capture being dropped, and never to be
    /// released by the caller. Null when the last answer was not a frame.
    ///
    /// A caller that has to hold one longer AddRefs the texture and Releases that
    /// reference itself, which is also the only way to feed an encoder that keeps
    /// the textures it has been given.
    pub fn texture(&self) -> *mut c_void {
        self.texture
    }

    /// The size the frames are at now: the display's own, or the window's, and
    /// the new one after [`Arrival::Resized`].
    pub fn content_size(&self) -> (u32, u32) {
        let (mut width, mut height) = (0, 0);
        // SAFETY: raw is an open capture and both pointers are places of their own.
        unsafe { yapfast_wgc_content_size(self.raw, &mut width, &mut height) };
        (width, height)
    }

    /// The `ID3D11Device` the frames' textures are on, for an encoder that has to
    /// open its session on the same device: NVENC reads a texture only on the
    /// device it was opened on, and refuses these from any other.
    pub fn device(&self) -> *mut c_void {
        // SAFETY: raw is an open capture.
        unsafe { yapfast_wgc_device(self.raw) }
    }

    /// What this capture is and how it went, in one line: the display, the card
    /// and output it is on, the size, and whether the border is off and what it
    /// took to turn it off. WGC's yellow indicator is a capture that works, so it
    /// is reported here rather than failed — Windows 10 has no way to remove it.
    pub fn describe(&self) -> String {
        let mut line = [0u8; 384];
        // SAFETY: raw is an open capture, and the buffer is as large as its size.
        unsafe { yapfast_wgc_describe(self.raw, line.as_mut_ptr().cast(), line.len()) };
        text(&line)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: raw came from the shim's constructor and is freed once; a
        // capture whose end already came is what this expects to find.
        unsafe { yapfast_wgc_close(self.raw) };
    }
}

/// The text the shim wrote into `bytes`, which ends with a zero.
fn text(bytes: &[u8]) -> String {
    std::ffi::CStr::from_bytes_until_nul(bytes).map_or_else(
        |_| String::new(),
        |written| written.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    /// The primary display, captured and asked for three frames: each one is a
    /// texture the compositor filled, at the size the capture reports, and the
    /// capture closes without complaining about anything. What is checked is that
    /// the path from the screen to a texture is there at all, and what a stream
    /// would get told — the line [`Capture::describe`] writes says which card and
    /// output it is on and whether the border could be turned off, which on
    /// Windows 10 it cannot.
    #[test]
    #[ignore = "needs a screen: it captures the primary display"]
    fn three_frames_of_the_primary_display_arrive_as_textures() {
        let mut capture =
            Capture::open(Monitor::PRIMARY).expect("a capture of the primary display");
        eprintln!("{}", capture.describe());
        let device = capture.device();
        eprintln!("device {device:p}");
        assert!(!device.is_null());

        let started = Instant::now();
        let mut seen = 0;
        while seen < 3 {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "no frame in 10 s, which is what a capture of a screen that isn't drawing does"
            );
            match capture.frame().expect("a frame") {
                Arrival::Frame => {
                    let (width, height) = capture.content_size();
                    let texture = capture.texture();
                    seen += 1;
                    eprintln!(
                        "frame {seen}: {width}x{height}, texture {texture:p}, {}",
                        if texture.is_null() {
                            "null"
                        } else {
                            "not null"
                        }
                    );
                    assert!(!texture.is_null(), "a frame arrived with no texture");
                    assert!(
                        width > 0 && height > 0,
                        "a frame arrived at {width}x{height}"
                    );
                }
                // The compositor hasn't made a frame yet: the first one is late
                // by a few tens of milliseconds. Sleeping rather than asking
                // again at once keeps the wait off the processor.
                Arrival::Idle => std::thread::sleep(Duration::from_millis(5)),
                Arrival::Resized => {
                    let (width, height) = capture.content_size();
                    eprintln!("the capture was resized to {width}x{height}");
                }
                Arrival::Closed => panic!("the primary display's capture closed"),
            }
        }
        drop(capture);
    }
}
