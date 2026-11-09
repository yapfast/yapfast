//! Linux's screen capture on the GPU: a screen or a window as the DMA-BUF buffers
//! a PipeWire node carries, so an encoder reads the compositor's own frames on the
//! card without a copy through the processor.
//!
//! It is the Wayland and X11 sibling of `crate::wgc` and `crate::dup`, and it
//! exists for the same reason. libwebrtc's `DesktopCapturer` — what
//! `crate::stream_worker` captures with — hands over I420 in memory, so every
//! frame of a share is copied out of the compositor, converted, and uploaded
//! before an encoder sees it. Here the frames never leave the GPU: the desktop
//! exports a PipeWire node carrying `SPA_DATA_DmaBuf` buffers, and a buffer is an
//! fd naming a GEM object plus the layout of the picture inside it. An encoder
//! that imports that fd on the same card encodes it where it lies.
//!
//! The two halves are [`super::portal`], which asks the desktop for a screen over
//! the session bus, and `native/pipewire_capture.c`, which connects to the node it
//! exported and hands out one buffer at a time. What a caller sees is one object:
//!
//! ```no_run
//! # #[cfg(screencast)] {
//! use std::time::Duration;
//! use yapfast_core::capture::pipewire::{self, Arrival, Source};
//!
//! let mut capture = pipewire::open(Source::Monitors)?;
//! tracing::info!(sharing = capture.describe(), "sharing a screen");
//! loop {
//!     match capture.frame(Duration::from_millis(500))? {
//!         Arrival::Frame(picture) => {
//!             // The fd is the frame: import it on the card, encode it, and let
//!             // the next call take it back.
//!             let _ = (picture.fd(), picture.size(), picture.stride());
//!         }
//!         Arrival::Still => {}
//!         Arrival::Resized => { /* rebuild whatever encodes at the new size */ }
//!         Arrival::Closed => break,
//!     }
//! }
//! # }
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! What a caller has to know, in the order it costs:
//!
//!   - [`open`] blocks until the desktop answers, and the desktop asks the person
//!     which screen to share, so it blocks until that dialog is dealt with. It is
//!     called from the thread that will read frames, before the stream starts.
//!   - a [`Picture`] is borrowed, like `wgc::Capture`'s texture. Its fd belongs to
//!     the stream and is valid until the next [`Capture::frame`] or the `Capture`
//!     being dropped, whichever comes first; a caller that has to hold one longer
//!     `dup`s it with [`Picture::dup`] and closes that copy itself. One buffer is
//!     held at a time, so a stream that has stopped asking stops pinning a
//!     compositor allocation.
//!   - only DMA-BUFs are asked for and only theirs are taken. A desktop that
//!     can't produce one fails with a sentence saying so rather than handing over
//!     memory that would have to be copied.
//!   - the pixel format is whatever the desktop fixated, BGRA or RGBA, with a
//!     modifier — linear unless the card says otherwise. [`Picture::format`] is
//!     that format and [`Picture::modifier`] that modifier, so an importer asks
//!     for an image in exactly the layout it was given.
//!   - frames come when the screen changes, as the compositor decides, not at a
//!     rate a caller sets. A screen that hasn't changed produces [`Arrival::Still`]
//!     when the timeout runs out, which is not a failure: it is the loop's chance
//!     to notice it should stop.
//!
//! Nothing here is wired into `crate::stream_worker` yet. Wiring it in is: picking
//! this capture where the portal is there and libwebrtc's capturer otherwise,
//! teaching the encoder in [`crate::encode`] — and `native/hwenc.c` behind it —
//! about a DMA-BUF, which is an EGL import on the card the compositor used, and
//! passing the frames from [`Capture::frame`] to it. The module is behind the
//! `screencast` feature, off by default, and is compiled only where
//! `libpipewire`'s headers and library are found; a build without either is
//! exactly the build it was before.
use std::{
    ffi::{c_char, c_int},
    os::fd::{AsRawFd, BorrowedFd, OwnedFd},
    time::Duration,
};

use anyhow::{Result, bail};

pub use super::portal::{Ask, Cursor, Source, Stream};

/// A capture as `native/pipewire_capture.c` owns it.
#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

/// One frame as the shim hands it over: an fd, and where the picture is in it.
/// The fields and their order are the C struct's.
#[repr(C)]
#[derive(Default)]
struct RawFrame {
    fd: c_int,
    offset: u32,
    stride: i32,
    size: u32,
    format: u32,
    modifier: u64,
    width: u32,
    height: u32,
}

// SAFETY: the shim's own declarations. Every pointer passed is one it returned,
// the buffers are as large as their sizes say, and the fd a frame names is the
// shim's for as long as it says.
unsafe extern "C" {
    fn yapfast_pw_open(node_id: u32, fd: c_int, why: *mut c_char, why_size: usize) -> *mut Raw;
    fn yapfast_pw_frame(
        capture: *mut Raw,
        frame: *mut RawFrame,
        timeout_ms: c_int,
        why: *mut c_char,
        why_size: usize,
    ) -> c_int;
    fn yapfast_pw_describe(capture: *mut Raw, text: *mut c_char, text_size: usize);
    fn yapfast_pw_close(capture: *mut Raw);
}

/// The shim's answers to [`Capture::frame`], as `native/pipewire_capture.c`
/// numbers them, which are [`crate::wgc`]'s numbers too.
const CLOSED: c_int = -1;
const FAILED: c_int = -2;
const STILL: c_int = 0;
const FRAME: c_int = 1;
const RESIZED: c_int = 2;

/// How large a sentence the shim writes is; its own buffers are this size.
const WHY: usize = 256;

/// The pixel formats a desktop shares a screen in, as `spa/param/video/raw.h`
/// numbers them.
const SPA_VIDEO_FORMAT_RGBX: u32 = 7;
const SPA_VIDEO_FORMAT_BGRX: u32 = 8;
const SPA_VIDEO_FORMAT_RGBA: u32 = 11;
const SPA_VIDEO_FORMAT_BGRA: u32 = 12;
const SPA_VIDEO_FORMAT_RGB: u32 = 15;
const SPA_VIDEO_FORMAT_BGR: u32 = 16;

/// How a desktop shares one thing: what it is, where it is, and how to ask for it
/// again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shared {
    /// The PipeWire node its frames come on.
    pub node_id: u32,
    /// How large the desktop says it is, which a frame need not be: a screen can
    /// be shared at another scale, and then [`Picture::size`] is the truth.
    pub size: (u32, u32),
    /// Which of [`Source`] it turned out to be, where the desktop says: 1 for a
    /// screen, 2 for a window. `None` from a portal older than 4.
    pub source_type: Option<u32>,
}

/// What asking a [`Capture`] for its next frame found. A screen that hasn't
/// changed produces nothing, and a share whose size changed has to have whatever
/// encodes it rebuilt, so neither of those is a failure.
#[derive(Debug)]
pub enum Arrival<'a> {
    /// A frame arrived, borrowed until the next call: see [`Picture`].
    Frame(Picture<'a>),
    /// Nothing has changed since the last frame. Ask again, and let this be where
    /// a loop notices it should stop — a still screen is a normal thing to sit at.
    Still,
    /// The shared thing was resized, or reshared at another format, and the
    /// frames after this one are at the new one, which [`Capture::size`] and
    /// [`Capture::format`] now say. Whatever encodes the frames was built for the
    /// old size, so it is rebuilt here, and the next call asks for the frame that
    /// this news was about.
    Resized,
    /// The desktop stopped sharing: the person ended the share, the window was
    /// closed, or the session went away. The capture is over — it hands out no
    /// more frames and should be dropped — and [`Capture::ending`] says which.
    Closed,
}

/// One frame, as a fd: the compositor's buffer, not a copy of it.
///
/// It is borrowed from the [`Capture`] it came from and lives only until that
/// capture's next [`Capture::frame`] or until the capture is dropped. A caller
/// that has to keep it — an encoder that queues work, a viewer that renders later
/// — takes [`Picture::dup`] and closes that copy when it is done, which is the one
/// place in this path where a caller has to be careful about a lifetime.
#[derive(Debug)]
pub struct Picture<'a> {
    fd: BorrowedFd<'a>,
    offset: u32,
    stride: u32,
    bytes: u32,
    format: Format,
    modifier: u64,
    width: u32,
    height: u32,
}

impl<'a> Picture<'a> {
    /// The DMA-BUF, as it was given. Import it on the card it was rendered on —
    /// the compositor's, which is not necessarily the one an encoder was opened
    /// on — at [`Picture::offset`], with [`Picture::stride`] between rows.
    pub fn fd(&self) -> BorrowedFd<'a> {
        self.fd
    }

    /// A copy of the fd that lives until its owner closes it, for a caller that
    /// has to hold a frame past the next [`Capture::frame`].
    pub fn dup(&self) -> std::io::Result<OwnedFd> {
        self.fd.try_clone_to_owned()
    }

    /// How far into the DMA-BUF the picture starts, which is not always zero: a
    /// plane can begin partway into an allocation.
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// How many bytes apart the rows are.
    pub fn stride(&self) -> u32 {
        self.stride
    }

    /// How many bytes the picture takes, rows and padding together.
    pub fn bytes(&self) -> u32 {
        self.bytes
    }

    /// The pixel format the desktop fixated.
    pub fn format(&self) -> Format {
        self.format
    }

    /// The DRM format modifier, which says how the picture is laid out in memory:
    /// zero for linear, and otherwise what the compositor's tiling is. An importer
    /// has to ask for this exact modifier, or it reads the wrong bytes.
    pub fn modifier(&self) -> u64 {
        self.modifier
    }

    /// The frame's size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The frame as one line for the log.
    pub fn describe(&self) -> String {
        format!(
            "{}x{} {}, modifier {}, stride {}, {} bytes at offset {}",
            self.width,
            self.height,
            self.format.name(),
            if self.modifier == 0 {
                "linear".to_owned()
            } else {
                format!("{:#018x}", self.modifier)
            },
            self.stride,
            self.bytes,
            self.offset
        )
    }
}

/// A pixel format a screen arrives in, as the desktop fixated it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Four bytes a pixel, in memory as B, G, R, A — DRM's `ARGB8888`, and what a
    /// compositor renders a desktop into.
    Bgra,
    /// The same with red and blue the other way round.
    Rgba,
    /// B, G, R and a byte nothing reads.
    Bgrx,
    /// The same with red and blue the other way round.
    Rgbx,
    /// Three bytes a pixel, which a screencast doesn't produce but a graphics
    /// desktop can.
    Bgr,
    /// The same with red and blue the other way round.
    Rgb,
    /// A format this build has no name for, as the number PipeWire gave.
    Other(u32),
}

impl Format {
    /// The format `spa`'s number names.
    const fn from_spa(format: u32) -> Self {
        match format {
            SPA_VIDEO_FORMAT_BGRA => Self::Bgra,
            SPA_VIDEO_FORMAT_RGBA => Self::Rgba,
            SPA_VIDEO_FORMAT_BGRX => Self::Bgrx,
            SPA_VIDEO_FORMAT_RGBX => Self::Rgbx,
            SPA_VIDEO_FORMAT_BGR => Self::Bgr,
            SPA_VIDEO_FORMAT_RGB => Self::Rgb,
            other => Self::Other(other),
        }
    }

    /// What this format is called in a log line.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bgra => "BGRA",
            Self::Rgba => "RGBA",
            Self::Bgrx => "BGRx",
            Self::Rgbx => "RGBx",
            Self::Bgr => "BGR",
            Self::Rgb => "RGB",
            Self::Other(_) => "an unnamed format",
        }
    }

    /// The four bytes a pixel takes, which is what a stride is a multiple of.
    pub const fn bytes(self) -> u32 {
        match self {
            Self::Bgra | Self::Rgba | Self::Bgrx | Self::Rgbx | Self::Other(_) => 4,
            Self::Bgr | Self::Rgb => 3,
        }
    }
}

/// A screen the desktop is sharing, whose frames are DMA-BUFs.
///
/// Dropping it ends the share: the PipeWire stream is closed and the portal's
/// session with it, so the desktop stops capturing and takes its own state down.
#[derive(Debug)]
pub struct Capture {
    raw: *mut Raw,
    session: super::portal::Session,
    size: (u32, u32),
    format: Format,
}

/// SAFETY: the shim's handle is only ever used behind `&mut self`, so it moves
/// between threads but is never used on two at once; it is PipeWire's own thread
/// loop and not the caller's that runs the callbacks, and both the portal's
/// connection and PipeWire's thread loop may be dropped from any thread.
unsafe impl Send for Capture {}

impl Capture {
    /// Asks the desktop for `source`, and returns once it has answered — which
    /// includes the person choosing in the desktop's dialog, so this blocks for as
    /// long as that dialog is open.
    pub fn open(source: Source) -> Result<Self> {
        Self::open_asking(Ask::new(source))
    }

    /// The same, saying everything a capture can say: what to share, how the
    /// pointer is drawn, and the token of a share the desktop already approved.
    pub fn open_asking(ask: Ask) -> Result<Self> {
        let session = super::portal::Session::open(&ask)?;
        let stream = session
            .streams()
            .first()
            .ok_or_else(|| anyhow::anyhow!("the desktop started a share with nothing in it"))?;
        let node_id = stream.node_id;
        let size = (
            u32::try_from(stream.size.0).unwrap_or(0),
            u32::try_from(stream.size.1).unwrap_or(0),
        );
        // The portal names a socket, and it is the only one this node is visible
        // on; PipeWire closes what it is given, so it is given a copy.
        let fd = session.pipewire()?;
        let handed = unsafe { libc::dup(fd.as_raw_fd()) };
        if handed < 0 {
            bail!("the desktop's PipeWire connection couldn't be copied");
        }
        let mut why = [0 as c_char; WHY];
        // SAFETY: the fd is a fresh duplicate, the shim takes it, and the text
        // buffer is `WHY` bytes as it is told.
        let raw = unsafe { yapfast_pw_open(node_id, handed, why.as_mut_ptr(), why.len()) };
        if raw.is_null() {
            bail!("{}", word(&why));
        }
        // The node id is the share; until a format is negotiated the size is the
        // one the portal named, and a capture that never negotiates reports it.
        Ok(Self {
            raw,
            size,
            format: Format::Bgra,
            session,
        })
    }

    /// Waits up to `timeout` for the screen to change, and answers with what
    /// arrived. A frame is borrowed until the next call.
    pub fn frame(&mut self, timeout: Duration) -> Result<Arrival<'_>> {
        let wait = timeout.as_millis().min(c_int::MAX as u128) as c_int;
        let mut frame = RawFrame::default();
        let mut why = [0 as c_char; WHY];
        // SAFETY: `raw` is this capture's, the frame and the text buffer are as
        // large as their sizes say, and the answer is one of the five numbers
        // above.
        let answer =
            unsafe { yapfast_pw_frame(self.raw, &mut frame, wait, why.as_mut_ptr(), why.len()) };
        match answer {
            FRAME => {
                self.size = (frame.width, frame.height);
                self.format = Format::from_spa(frame.format);
                // SAFETY: a frame's fd is the shim's, and it is valid until this
                // capture is asked for another one or dropped, which `&'a mut
                // self` here is what guarantees.
                let fd = unsafe { BorrowedFd::borrow_raw(frame.fd) };
                Ok(Arrival::Frame(Picture {
                    fd,
                    offset: frame.offset,
                    stride: u32::try_from(frame.stride).unwrap_or(0),
                    bytes: frame.size,
                    format: self.format,
                    modifier: frame.modifier,
                    width: frame.width,
                    height: frame.height,
                }))
            }
            RESIZED => Ok(Arrival::Resized),
            STILL => Ok(Arrival::Still),
            CLOSED => Ok(Arrival::Closed),
            FAILED => bail!("{}", word(&why)),
            other => bail!("the screen capture answered something unreadable ({other})"),
        }
    }

    /// The share as one line for the log: the node, what it negotiated, and how
    /// many frames it has carried.
    pub fn describe(&self) -> String {
        let mut text = String::new();
        text.reserve(WHY);
        let mut buffer = [0 as c_char; WHY];
        // SAFETY: the capture is this one's, and the buffer is `WHY` bytes.
        unsafe { yapfast_pw_describe(self.raw, buffer.as_mut_ptr(), buffer.len()) };
        text.push_str(&word(&buffer));
        format!("{}, sharing {}", text, self.session.describe())
    }

    /// The token the desktop will remember this share by, for saving. Handing it
    /// back in [`Ask::restore_token`] is what makes the next share of the same
    /// screen need no dialog.
    pub fn restore_token(&self) -> Option<&str> {
        self.session.restore_token()
    }

    /// What is shared, as the portal described it.
    pub fn shared(&self) -> Vec<Shared> {
        self.session
            .streams()
            .iter()
            .map(|stream| Shared {
                node_id: stream.node_id,
                size: (
                    u32::try_from(stream.size.0).unwrap_or(0),
                    u32::try_from(stream.size.1).unwrap_or(0),
                ),
                source_type: stream.source_type,
            })
            .collect()
    }

    /// The last frame's size in pixels, which a share that has been resized has
    /// changed.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The last frame's pixel format.
    pub fn format(&self) -> Format {
        self.format
    }

    /// Why the share ended, once [`Arrival::Closed`] says it has: the desktop
    /// closed the screen, the window's owner quit, or the session was taken away.
    pub fn ending(&self) -> String {
        self.describe()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // The stream first, so no callback runs while the portal ends the session:
        // a frame's fd is the stream's, and the session is what makes it.
        // SAFETY: `raw` came from the shim, the capture is being dropped, and it is
        // freed once and only here.
        unsafe { yapfast_pw_close(self.raw) };
    }
}

/// Asks the desktop for a screen, and returns a capture of it: the whole of what a
/// caller has to do to get frames on the GPU.
///
/// This is what a stream calls where the portal is there, with libwebrtc's own
/// capturer as the fallback — the same shape as [`crate::capture`] picks a rung
/// on Windows. It fails when there is no session bus, no portal, no dialog
/// answer, or a desktop that shares nothing; every one of those is a share that
/// has to fall back rather than a stream that can't start.
pub fn open(source: Source) -> Result<Capture> {
    Capture::open(source)
}

/// The same, saying everything a capture can say. See [`Capture::open_asking`].
pub fn open_asking(ask: Ask) -> Result<Capture> {
    Capture::open_asking(ask)
}

/// A sentence the shim wrote into a `WHY`-byte buffer, as a `String`.
fn word(buffer: &[c_char; WHY]) -> String {
    // SAFETY: the shim null-terminates what it writes into the buffer it is given,
    // and the buffer is `WHY` bytes either way.
    unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}
