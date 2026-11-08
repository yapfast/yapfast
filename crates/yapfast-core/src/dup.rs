//! Desktop Duplication as textures: native/dup.cc opens a duplication of one
//! display and hands out one borrowed `ID3D11Texture2D` per frame, upright and in
//! BGRA whatever the desktop is set to, with the mouse pointer already drawn in.
//!
//! It is the same end of the stream as [`crate::wgc`] and a different door into
//! it. WGC gives a frame already tonemapped and already upright but draws no
//! cursor; DXGI's duplication gives the compositor's own surface — scRGB
//! half-floats on an HDR desktop, the mode's own orientation on a rotated one,
//! no pointer at all — and the shim does those three jobs on the GPU before the
//! frame is handed over. Which one a stream should use is a question for the loop
//! that isn't written yet; this is the second answer, kept beside the first.
//!
//! Nothing here is wired into a stream: the capture loop that should hold a
//! [`Capture`], poll it and hand frames to an encoder is the next stage.
//!
//! Two things a caller has to know that the shim can't enforce: the texture is
//! the capture's own, so its picture is overwritten by the next call that has
//! something to draw, and it must never be released; and the device a frame's
//! texture is on is [`Capture::device`], which an encoder has to open its session
//! on to read it.

use std::ffi::{c_char, c_int, c_uint, c_void};
use std::time::Duration;

use anyhow::{Result, bail};

/// A capture as the shim owns it.
#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

// SAFETY: the shim's own declarations; every pointer passed is one it returned,
// and the buffers are as large as their sizes say.
unsafe extern "C" {
    fn yapfast_dup_primary(
        luid_high: *mut c_uint,
        luid_low: *mut c_uint,
        output: *mut c_uint,
        why: *mut c_char,
        why_size: usize,
    ) -> c_int;
    fn yapfast_dup_open(
        luid_high: c_uint,
        luid_low: c_uint,
        output: c_uint,
        why: *mut c_char,
        why_size: usize,
    ) -> *mut Raw;
    fn yapfast_dup_open_on(
        device: *mut c_void,
        luid_high: c_uint,
        luid_low: c_uint,
        output: c_uint,
        why: *mut c_char,
        why_size: usize,
    ) -> *mut Raw;
    fn yapfast_dup_frame(
        capture: *mut Raw,
        timeout_ms: c_uint,
        texture: *mut *mut c_void,
        why: *mut c_char,
        why_size: usize,
    ) -> c_int;
    fn yapfast_dup_content_size(capture: *mut Raw, width: *mut c_uint, height: *mut c_uint);
    fn yapfast_dup_rotation(capture: *mut Raw) -> c_uint;
    fn yapfast_dup_format(capture: *mut Raw) -> c_uint;
    fn yapfast_dup_pointer_seen(capture: *mut Raw) -> c_int;
    fn yapfast_dup_sample(
        capture: *mut Raw,
        x: c_uint,
        y: c_uint,
        bgra: *mut c_uint,
        why: *mut c_char,
        why_size: usize,
    ) -> c_int;
    fn yapfast_dup_device(capture: *mut Raw) -> *mut c_void;
    fn yapfast_dup_describe(capture: *mut Raw, text: *mut c_char, text_size: usize);
    fn yapfast_dup_close(capture: *mut Raw);
}

/// What asking a [`Capture`] for its next frame found.
///
/// The third answer is the one this shim adds to [`crate::wgc::Arrival`]: a
/// desktop that only moved its mouse produces no new desktop image at all, and a
/// frame whose picture is the same one redrawn with the pointer where it is now
/// is not a stale frame — it is what the screen shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrival {
    /// A new desktop image arrived: [`Capture::texture`] holds it, upright, in
    /// BGRA, with the pointer in it.
    Frame,
    /// The pointer moved, changed shape or went away, and nothing else did. The
    /// picture is the last one, drawn again with the pointer where it is now.
    Pointer,
    /// Nothing has been drawn and nothing has moved: the screen is as it was.
    /// This is `DXGI_ERROR_WAIT_TIMEOUT`, which is the duplication saying "no
    /// change", not a failure. Ask again after a sleep rather than at once.
    Still,
}

/// The shim's answers to [`Capture::frame`], as native/dup.cc numbers them.
const FAILED: c_int = -1;
const STILL: c_int = 0;
const FRAME: c_int = 1;
const POINTER: c_int = 2;

/// Which display to duplicate: the card by its LUID, and which of that card's
/// outputs, as DXGI's own enumeration numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    luid_high: u32,
    luid_low: u32,
    output: u32,
}

impl Target {
    /// The display this names, as an adapter's LUID and an output index — what
    /// DXGI reports for an adapter and its `EnumOutputs`.
    pub const fn new(luid_high: u32, luid_low: u32, output: u32) -> Self {
        Self {
            luid_high,
            luid_low,
            output,
        }
    }

    /// The primary display: the one the desktop starts on. It is what a caller
    /// that hasn't chosen a screen yet captures, and the shim finds it by looking
    /// for the output whose desktop rectangle has the origin for a corner.
    pub fn primary() -> Result<Self> {
        let mut why = [0u8; 256];
        let mut high = 0;
        let mut low = 0;
        let mut output = 0;
        // SAFETY: the three places are of their own, and the buffer is as large as
        // its size says; the shim ends it with a zero.
        let found = unsafe {
            yapfast_dup_primary(
                &mut high,
                &mut low,
                &mut output,
                why.as_mut_ptr().cast(),
                why.len(),
            )
        };
        if found == 0 {
            bail!("no primary display to duplicate: {}", text(&why));
        }
        Ok(Self::new(high, low, output))
    }

    /// The LUID's two halves, as [`Target::new`] takes them.
    pub const fn luid(self) -> (u32, u32) {
        (self.luid_high, self.luid_low)
    }

    /// Which of the card's outputs this is.
    pub const fn output(self) -> u32 {
        self.output
    }
}

/// Which way the display is turned, as DXGI reports it. The frame this shim
/// hands out is already upright, so this is for a log line rather than something
/// a caller acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    Identity,
    /// A quarter turn clockwise.
    Quarter,
    Half,
    /// Three quarters of a turn clockwise.
    ThreeQuarters,
    /// A rotation this build has no name for.
    Unnamed,
}

impl Rotation {
    const fn from_dxgi(named: u32) -> Self {
        match named {
            1 => Self::Identity,
            2 => Self::Quarter,
            3 => Self::Half,
            4 => Self::ThreeQuarters,
            _ => Self::Unnamed,
        }
    }
}

/// What the surface's pixels were before the shim converted them: the frame
/// itself is always 8-bit BGRA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pixels {
    Bgra8,
    /// 10-bit RGB, which passes through untouched. An HDR10 desktop arrives this
    /// way, PQ-coded, and this shim does not undo that — see the note in
    /// native/dup.cc.
    Rgb10A2,
    /// scRGB half-floats: linear light with 1.0 at 80 nits, which the shim turns
    /// into 8-bit BT.709 with a highlight shoulder.
    Scrgb,
    /// A format this build only passes through.
    Other,
}

/// A duplication of one display, delivering BGRA textures on one D3D11 device.
///
/// It owns the device, the duplication, two full-screen textures and the pointer's
/// shape, and lets them go when it is dropped.
pub struct Capture {
    raw: *mut Raw,
    /// The texture the last [`Arrival`] delivered, which the shim keeps until the
    /// capture is closed: held here so that asking for it again doesn't have to
    /// disturb the capture, and null before the first frame.
    texture: *mut c_void,
}

// SAFETY: the duplication is bound to the thread that opened it, and this wrapper
// is what keeps one thread at a time on it — the shim keeps no thread-local state
// of its own, and nothing in it is touched from anywhere else, so moving the
// capture between threads is moving the whole of it.
unsafe impl Send for Capture {}

impl Capture {
    /// Opens a duplication of `target` and makes its device on the card that
    /// drives it, since a duplication only exists on its own adapter's device. A
    /// caller that already has a device there — an encoder's — uses
    /// [`Capture::open_on`] instead, and saves the second device.
    ///
    /// It fails on a display another duplicator already holds, on a secure
    /// desktop, and on an output that cannot be duplicated at all; the shim's
    /// message says which.
    pub fn open(target: Target) -> Result<Self> {
        let mut why = [0u8; 256];
        // SAFETY: the buffer is as large as its size says, and the shim ends it
        // with a zero.
        let raw = unsafe {
            yapfast_dup_open(
                target.luid_high,
                target.luid_low,
                target.output,
                why.as_mut_ptr().cast(),
                why.len(),
            )
        };
        if raw.is_null() {
            bail!("no Desktop Duplication opened: {}", text(&why));
        }
        Ok(Self {
            raw,
            texture: std::ptr::null_mut(),
        })
    }

    /// Opens a duplication of `target` on `device`, an `ID3D11Device` somebody has
    /// already made — in a stream, the encoder's, so that the frame's texture is a
    /// texture the encoder reads with nothing copied in between.
    ///
    /// `device` is **borrowed**: this does not AddRef it and dropping the capture
    /// does not release it, so the caller keeps it alive for at least as long as
    /// the capture. It has to have been made on the adapter that drives the output
    /// being duplicated, which is DXGI's own requirement for `DuplicateOutput`
    /// rather than this crate's: a device from another card is refused naming both
    /// cards. [`Capture::device`] answers with the same pointer back.
    //
    // `device` is an opaque handle in the same sense [`Capture::texture`] is:
    // nothing in this crate reads through it, it is handed to the shim, and the
    // shim holds it for the capture's length without taking a reference of its
    // own. What a caller owes is the lifetime above, which a capture beside the
    // encoder is how a stream keeps.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn open_on(device: *mut c_void, target: Target) -> Result<Self> {
        let mut why = [0u8; 256];
        // SAFETY: the buffer is as large as its size says and the shim ends it
        // with a zero; the device is the caller's and outlives the call, which is
        // what borrowing it means.
        let raw = unsafe {
            yapfast_dup_open_on(
                device,
                target.luid_high,
                target.luid_low,
                target.output,
                why.as_mut_ptr().cast(),
                why.len(),
            )
        };
        if raw.is_null() {
            bail!(
                "no Desktop Duplication opened on that device: {}",
                text(&why)
            );
        }
        Ok(Self {
            raw,
            texture: std::ptr::null_mut(),
        })
    }

    /// Asks for the next frame, waiting at most `timeout` for the desktop to
    /// change.
    ///
    /// The picture the previous call delivered is overwritten when this call has
    /// something to draw, so it must not be read after this returns.
    pub fn frame(&mut self, timeout: Duration) -> Result<Arrival> {
        let mut why = [0u8; 256];
        let mut texture = std::ptr::null_mut();
        let wait = c_uint::try_from(timeout.as_millis()).unwrap_or(c_uint::MAX);
        // SAFETY: raw is an open capture, the texture pointer and the buffer are
        // places of their own, and they stay put during the call.
        let found = unsafe {
            yapfast_dup_frame(
                self.raw,
                wait,
                &mut texture,
                why.as_mut_ptr().cast(),
                why.len(),
            )
        };
        match found {
            FRAME => {
                self.texture = texture;
                Ok(Arrival::Frame)
            }
            POINTER => {
                self.texture = texture;
                Ok(Arrival::Pointer)
            }
            STILL => Ok(Arrival::Still),
            FAILED => bail!(
                "the duplication stopped, having found no frame: {}",
                text(&why)
            ),
            // The shim and this file disagreeing about the numbers is the two
            // sides of the boundary being out of step, which is worth refusing
            // rather than reading as one of the answers above.
            other => bail!("the duplication answered {other}, which native/dup.cc doesn't have"),
        }
    }

    /// The picture [`Arrival::Frame`] or [`Arrival::Pointer`] brought: an
    /// `ID3D11Texture2D` on [`Capture::device`], upright, in BGRA, **borrowed** —
    /// the capture's own, whose picture the next call may overwrite, and never to
    /// be released by the caller. Null before the first frame.
    pub fn texture(&self) -> *mut c_void {
        self.texture
    }

    /// The desktop's size, which is the frame's: a portrait screen is 1080x1920
    /// here and not the 1920x1080 surface a rotated display hands over.
    pub fn content_size(&self) -> (u32, u32) {
        let mut width = 0;
        let mut height = 0;
        // SAFETY: raw is an open capture and the two places are of their own.
        unsafe { yapfast_dup_content_size(self.raw, &mut width, &mut height) };
        (width, height)
    }

    /// Which way the display is turned. The frame is already upright.
    pub fn rotation(&self) -> Rotation {
        // SAFETY: raw is an open capture.
        Rotation::from_dxgi(unsafe { yapfast_dup_rotation(self.raw) })
    }

    /// What the surface's pixels were, before the shim converted them.
    pub fn pixels(&self) -> Pixels {
        // SAFETY: raw is an open capture.
        match unsafe { yapfast_dup_format(self.raw) } {
            0 => Pixels::Bgra8,
            1 => Pixels::Rgb10A2,
            2 => Pixels::Scrgb,
            _ => Pixels::Other,
        }
    }

    /// Whether a visible pointer has been seen at all. It is separate from
    /// [`Capture::texture`] on purpose: a shape arriving, and being drawn into
    /// the picture, are the two halves of the pointer working, and a machine
    /// whose mouse nobody touches still reports the first.
    pub fn pointer_seen(&self) -> bool {
        // SAFETY: raw is an open capture. Parenthesised because an `unsafe` block
        // in statement position is a statement and not a value.
        (unsafe { yapfast_dup_pointer_seen(self.raw) }) != 0
    }

    /// The `ID3D11Device` the frame's texture is on: the one this capture made, or
    /// the one it was opened on with [`Capture::open_on`]. It is what an encoder
    /// opens its session on, with
    /// [`D3d11Encoder::open_on`](crate::d3d11::D3d11Encoder::open_on), so that it
    /// reads these frames; it belongs to this capture when the capture made it and
    /// to whoever made it otherwise.
    pub fn device(&self) -> *mut c_void {
        // SAFETY: raw is an open capture.
        unsafe { yapfast_dup_device(self.raw) }
    }

    /// One pixel of the frame [`Arrival::Frame`] or [`Arrival::Pointer`] last
    /// delivered, as `0xRRGGBB`.
    ///
    /// It is a diagnostic and not part of the frame path: it copies the whole
    /// frame to a staging texture on the way to the processor, which is the one
    /// thing a stream never does. It exists because a rotation and a colour
    /// conversion can both be wrong in a way a non-null texture doesn't show.
    pub fn sample(&self, x: u32, y: u32) -> Result<u32> {
        let mut why = [0u8; 256];
        let mut bgra = 0;
        // SAFETY: raw is an open capture and the place is the colour's own.
        let got = unsafe {
            yapfast_dup_sample(
                self.raw,
                x,
                y,
                &mut bgra,
                why.as_mut_ptr().cast(),
                why.len(),
            )
        };
        if got == 0 {
            bail!("no pixel at ({x},{y}): {}", text(&why));
        }
        Ok(bgra)
    }

    /// What this capture is, in one line: the card and output, both sizes, the
    /// rotation and what the surface's pixels were.
    pub fn describe(&self) -> String {
        let mut line = [0u8; 384];
        // SAFETY: raw is an open capture, and the buffer is as large as its size.
        unsafe { yapfast_dup_describe(self.raw, line.as_mut_ptr().cast(), line.len()) };
        text(&line)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: raw came from the shim's constructor and is freed once.
        unsafe { yapfast_dup_close(self.raw) };
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

    /// The primary display, duplicated and asked for a few frames: each one is a
    /// texture the compositor filled, at the size the capture reports, and the
    /// capture closes without complaining about anything. What is checked is that
    /// the path from the screen to an upright BGRA texture is there at all and
    /// what a stream would be told — the size, the rotation that was undone, what
    /// the surface's pixels were, and whether the pointer ever turned up.
    #[test]
    #[ignore = "needs a screen: it captures the primary display"]
    fn a_few_frames_of_the_primary_display_arrive_with_the_pointer() {
        let target = Target::primary().expect("a primary display to duplicate");
        eprintln!(
            "duplicating output {} of LUID {:08X}:{:08X}",
            target.output(),
            target.luid().0,
            target.luid().1
        );
        let mut capture = Capture::open(target).expect("a duplication of the primary display");
        eprintln!("{}", capture.describe());
        let device = capture.device();
        eprintln!("device {device:p}");
        assert!(!device.is_null());

        let started = Instant::now();
        let mut frames = 0;
        while frames < 3 {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "no frame in 10 s, which is what a capture of a screen that isn't drawing does"
            );
            let arrival = capture.frame(Duration::from_millis(100)).expect("a frame");
            match arrival {
                Arrival::Frame | Arrival::Pointer => {
                    let (width, height) = capture.content_size();
                    let texture = capture.texture();
                    let pointer = if capture.pointer_seen() {
                        "pointer seen"
                    } else {
                        "no pointer seen yet"
                    };
                    eprintln!(
                        "{frames}: {arrival:?}, {width}x{height}, rotation {:?}, pixels {:?}, texture \
                         {texture:p}, {pointer}",
                        capture.rotation(),
                        capture.pixels()
                    );
                    assert!(!texture.is_null(), "a frame arrived with no texture");
                    assert!(
                        width > 0 && height > 0,
                        "a frame arrived at {width}x{height}"
                    );
                    if frames == 2 {
                        // What the picture actually holds, at a spread of points: a
                        // rotation that went the wrong way, a surface read as BGRA
                        // when it was scRGB half-floats, and a black frame all look
                        // the same from a non-null texture, and none of them look the
                        // same here. The third arrival rather than the first, because
                        // the picture a new duplication opens with can be an empty
                        // one — on the machine this was written on it was.
                        for (x, y) in [
                            (100, 20),
                            (100, 720),
                            (100, 1431),
                            (1700, 720),
                            (2300, 720),
                            (3300, 20),
                            (3300, 1431),
                        ] {
                            match capture.sample(x, y) {
                                Ok(colour) => {
                                    eprintln!("  ({x},{y}) = #{colour:06X}");
                                }
                                Err(why) => eprintln!("  ({x},{y}) unreadable: {why}"),
                            }
                        }
                    }
                    frames += 1;
                }
                // Nothing has changed: a static desktop is what a duplication
                // spends most of its life on, so waiting for a change rather than
                // asking again at once is what keeps the wait off the processor.
                Arrival::Still => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        eprintln!(
            "closed with the pointer {}",
            if capture.pointer_seen() {
                "seen"
            } else {
                "never seen"
            }
        );
        drop(capture);
    }

    /// A duplication opened on a device someone else made: `DuplicateOutput`
    /// takes any device made on the output's own adapter rather than one this
    /// file made, which is what lets a stream hand it the encoder's and read the
    /// frame where it already is. In a stream the device comes from an encoder on
    /// the same card as the display; here it is an encoder's, opened the way a
    /// stream without a capture yet opens one.
    ///
    /// On a machine whose display is on a different card from the one the GPU
    /// setting chose, this can't work for anyone, and the shim says which two
    /// cards it was asked to bridge rather than failing obscurely; the test says
    /// so and stops instead of failing a correct machine.
    #[test]
    #[ignore = "needs a screen and an NVIDIA GPU: it duplicates the primary display on an encoder's device"]
    fn a_duplication_opens_on_a_device_it_was_given() {
        let target = Target::primary().expect("a primary display to duplicate");
        let encoder = crate::d3d11::D3d11Encoder::new(
            yapfast_discord::rtp::Codec::H264,
            1920,
            1080,
            60,
            8000,
        )
        .expect("an NVENC encoder on this machine");
        let device = encoder.device();
        assert!(!device.is_null(), "an encoder with no device");

        let mut capture = match Capture::open_on(device, target) {
            Ok(capture) => capture,
            Err(why) => {
                let said = format!("{why:#}");
                if said.contains("was made on") {
                    eprintln!("the display and the encoder are on different cards: {said}");
                    return;
                }
                panic!("no duplication on the encoder's device: {said}");
            }
        };
        eprintln!("{}", capture.describe());
        assert_eq!(
            capture.device(),
            device,
            "the duplication is on a device of its own rather than the one handed in"
        );

        // One frame is enough to prove the duplication was created with that
        // device: a duplication on another one would not be this capture's at all.
        let started = Instant::now();
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "no frame in 10 s, which is what a capture of a screen that isn't drawing does"
            );
            match capture.frame(Duration::from_millis(100)).expect("a frame") {
                Arrival::Frame | Arrival::Pointer => break,
                Arrival::Still => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        let texture = capture.texture();
        assert!(!texture.is_null(), "a frame arrived with no texture");
        let (width, height) = capture.content_size();
        eprintln!("a {width}x{height} frame on the encoder's own device {device:p}");

        // The capture outlives the device's borrower: the encoder goes first, and
        // the duplication still has its device to close with.
        drop(encoder);
        assert_eq!(
            capture.device(),
            device,
            "the encoder took the device with it"
        );
        drop(capture);
    }
}
