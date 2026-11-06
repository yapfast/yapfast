//! Windows Graphics Capture as textures: native/wgc.cc opens a capture of a
//! display, or of one window, and hands out one borrowed `ID3D11Texture2D` per
//! frame, so a screen reaches an encoder on the GPU without its pixels crossing
//! the processor.
//!
//! It is the other end of [`crate::d3d11`]: that encoder reads D3D11 textures,
//! and this is a capture source that makes them. A frame therefore goes from the
//! compositor to the driver untouched, which is the difference between about 5.4
//! ms of the processor per 3440x1440 frame and about 0.8 as the spike that
//! proved the path measured it (.local/spike/wgc-nvenc/RESULTS.md).
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
//! [`Capture::device`], which an encoder has to read it on. Either side can be
//! the one that makes that device: an encoder opened with
//! [`D3d11Encoder::open_on`](crate::d3d11::D3d11Encoder::open_on) joins this
//! capture's, and [`Capture::open_on`] joins an encoder's, and either way there
//! is one device for the pair rather than two.
//!
//! What is captured is a [`Target`], and its two kinds are never the same thing:
//! a display is asked for through `CreateForMonitor` and a window through
//! `CreateForWindow`, a window's handle is never read as a monitor's, and a window
//! capture that can't be opened fails rather than capturing the screen behind it.
//! A window is also something a display isn't — it can be resized, minimised,
//! closed, and can take itself out of every capture — so [`Arrival`] carries what
//! those mean for a caller in the same words it already answers a display with.

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
    fn yapfast_wgc_open_window(window: *mut c_void, why: *mut c_char, why_size: usize) -> *mut Raw;
    fn yapfast_wgc_open_on(
        device: *mut c_void,
        monitor: *mut c_void,
        why: *mut c_char,
        why_size: usize,
    ) -> *mut Raw;
    fn yapfast_wgc_open_window_on(
        device: *mut c_void,
        window: *mut c_void,
        why: *mut c_char,
        why_size: usize,
    ) -> *mut Raw;
    fn yapfast_wgc_frame(
        capture: *mut Raw,
        texture: *mut *mut c_void,
        why: *mut c_char,
        why_size: usize,
    ) -> c_int;
    fn yapfast_wgc_content_size(capture: *mut Raw, width: *mut c_uint, height: *mut c_uint);
    fn yapfast_wgc_device(capture: *mut Raw) -> *mut c_void;
    fn yapfast_wgc_borderless(capture: *mut Raw) -> c_int;
    fn yapfast_wgc_can_ask_border(capture: *mut Raw) -> c_int;
    fn yapfast_wgc_border(capture: *mut Raw, text: *mut c_char, text_size: usize);
    fn yapfast_wgc_describe(capture: *mut Raw, text: *mut c_char, text_size: usize);
    fn yapfast_wgc_ending(capture: *mut Raw, text: *mut c_char, text_size: usize);
    fn yapfast_wgc_build() -> c_uint;
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
    /// What was being captured closed, or it left every capture: its window was
    /// closed by its owner, its display's session ended, or — a window's own doing
    /// — `WDA_EXCLUDEFROMCAPTURE` took it out of what Windows Graphics Capture can
    /// see. The capture is over: it hands out no more frames and should be
    /// dropped, and the pool's last picture must not be sent again, since a closed
    /// window has none and an excluded one is content its owner kept out of
    /// captures. This is how a window capture ends when its owner quits, so it is
    /// not an error; [`Capture::ending`] says which of them happened.
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

/// Which window to capture: the platform's own handle for it, the `HWND` that
/// `Source::Window` names.
///
/// It is an identity and not memory this crate reads: the shim asks Windows
/// whether there is a window behind it, refuses by name when there isn't, and
/// hands it to `CreateForWindow`. There is no null window and no default one —
/// a capture that guessed would be of the desktop, or of whatever was in front,
/// and neither is the window the caller chose.
#[derive(Clone, Copy, Debug)]
pub struct Window(*mut c_void);

impl Window {
    /// The window `handle` names, from whichever enumeration the caller already
    /// has — its own list of windows, `GetForegroundWindow`, or the handle a
    /// window picker answered with.
    pub const fn new(handle: *mut c_void) -> Self {
        Self(handle)
    }
}

/// What a capture is of: one display, or one window on it.
///
/// The two are separate calls into Windows Graphics Capture
/// (`CreateForMonitor` and `CreateForWindow`) and separate kinds of handle, which
/// is why this is a choice rather than one handle that might be either. Nothing
/// here falls back from one to the other: a window that can't be captured is an
/// error naming it, never a capture of the screen behind it, because a share that
/// promised one window and delivered a desktop is worse than one that refused.
#[derive(Clone, Copy, Debug)]
pub enum Target {
    /// A display, by its monitor handle, or [`Monitor::PRIMARY`].
    Monitor(Monitor),
    /// One window, by the handle [`Window::new`] was given.
    Window(Window),
}

impl From<Monitor> for Target {
    fn from(monitor: Monitor) -> Self {
        Self::Monitor(monitor)
    }
}

impl From<Window> for Target {
    fn from(window: Window) -> Self {
        Self::Window(window)
    }
}

impl Target {
    /// Refuses a target that names nothing, before the shim is asked: a monitor
    /// handle may be null and mean the primary display, and a window's may not,
    /// since there is no default window and the shim's answer to a null one would
    /// only be a longer way to say this.
    fn named(self) -> Result<Self> {
        match self {
            Self::Window(window) if window.0.is_null() => bail!(
                "no window was named: a capture of a window needs that window's handle, which is \
                 not the desktop and not the primary display"
            ),
            other => Ok(other),
        }
    }

    /// Which of the shim's two constructors this goes to, and the handle it
    /// names: a window's through `CreateForWindow`, a display's through
    /// `CreateForMonitor`.
    fn split(self) -> (bool, *mut c_void) {
        match self {
            Self::Monitor(monitor) => (false, monitor.0),
            Self::Window(window) => (true, window.0),
        }
    }
}

/// A capture of a display or a window, delivering BGRA textures on one D3D11
/// device.
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
    /// Opens a capture of `target`, and starts it.
    ///
    /// The shim finds the adapter that drives what is being captured — the display
    /// itself, or the display a window is composited on — and makes its device
    /// there, since a capture on another adapter's device would fill textures the
    /// encoder can't read. A caller that already has a device on that adapter —
    /// an encoder's — uses [`Capture::open_on`] instead, and saves the second
    /// device.
    ///
    /// A [`Target::Window`] is checked before anything is opened: a window that has
    /// gone, a handle that isn't a window, a child window, and the desktop are each
    /// a failure naming what was looked at, since a share that asked for one window
    /// must never be handed a picture of the screen it sits on.
    pub fn open(target: impl Into<Target>) -> Result<Self> {
        let (window, handle) = target.into().named()?.split();
        let mut why = [0u8; 256];
        let size = why.len();
        let said = why.as_mut_ptr().cast();
        // SAFETY: the buffer is as large as its size says, and the shim ends it
        // with a zero. The handle is only compared against the system's own — a
        // display's, or a window's — and never read through.
        let raw = unsafe {
            if window {
                yapfast_wgc_open_window(handle, said, size)
            } else {
                yapfast_wgc_open(handle, said, size)
            }
        };
        if raw.is_null() {
            bail!("no Windows Graphics Capture opened: {}", text(&why));
        }
        Ok(Self {
            raw,
            texture: std::ptr::null_mut(),
        })
    }

    /// Opens a capture of `target` on `device`, an `ID3D11Device` somebody has
    /// already made — in a stream, the encoder's, so that the texture a frame
    /// arrives in is a texture the encoder reads with nothing copied in between.
    ///
    /// `device` is **borrowed**: this does not AddRef it and dropping the capture
    /// does not release it, so the caller keeps it alive for at least as long as
    /// the capture. It has to have been made on the adapter that drives what is
    /// being captured and with `D3D11_CREATE_DEVICE_BGRA_SUPPORT`; the shim checks
    /// both and refuses by name, since either would otherwise show up as a pool
    /// that wouldn't be created or as a frame that encodes as noise.
    ///
    /// [`Capture::device`] answers with the same pointer back, which is the whole
    /// of the sharing: one device for a capture and its encoder.
    //
    // `device` is an opaque handle in the same sense [`Capture::texture`] is:
    // nothing in this crate reads through it, it is handed to the shim, and the
    // shim holds it for the capture's length without taking a reference of its
    // own. What a caller owes is the lifetime above, which a capture beside the
    // encoder is how a stream keeps.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn open_on(device: *mut c_void, target: impl Into<Target>) -> Result<Self> {
        let (window, handle) = target.into().named()?.split();
        let mut why = [0u8; 256];
        let size = why.len();
        let named = why.as_mut_ptr().cast();
        // SAFETY: the buffer is as large as its size says and the shim ends it
        // with a zero; the device is the caller's and outlives the call, which is
        // what borrowing it means; the handle is only compared, never read.
        let raw = unsafe {
            if window {
                yapfast_wgc_open_window_on(device, handle, named, size)
            } else {
                yapfast_wgc_open_on(device, handle, named, size)
            }
        };
        if raw.is_null() {
            bail!(
                "no Windows Graphics Capture opened on that device: {}",
                text(&why)
            );
        }
        Ok(Self {
            raw,
            texture: std::ptr::null_mut(),
        })
    }

    /// Asks for the next frame, of the compositor's making: one call is one look,
    /// and [`Arrival::Idle`] means nothing has been drawn since the last one.
    ///
    /// For a window that also covers the moment it is minimised, when the
    /// compositor has nothing of it to give.
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
        let arrival = arrival(found, &why)?;
        if arrival == Arrival::Frame {
            self.texture = texture;
        }
        Ok(arrival)
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

    /// The `ID3D11Device` the frames' textures are on: the one this capture made,
    /// or the one it was opened on with [`Capture::open_on`]. It is what an
    /// encoder opens its session on, with
    /// [`D3d11Encoder::open_on`](crate::d3d11::D3d11Encoder::open_on), so that it
    /// reads these frames. It belongs to this capture when the capture made it
    /// and to whoever made it otherwise; nothing may release it but its maker.
    pub fn device(&self) -> *mut c_void {
        // SAFETY: raw is an open capture.
        unsafe { yapfast_wgc_device(self.raw) }
    }

    /// What this capture is and how it went, in one line: the display, or the
    /// window with its title and class and the display it is composited on, the
    /// card and output, the size, and whether the border is off and what it took
    /// to turn it off. WGC's yellow indicator is a capture that works, so it is
    /// reported here rather than failed — Windows 10 has no way to remove it, and
    /// a window's item draws it just as a display's does. A caller deciding
    /// whether to keep this capture reads [`Capture::borderless`] rather than this
    /// line.
    pub fn describe(&self) -> String {
        // Larger than the line usually is: a window's own description carries its
        // title, its class and the display it is on as well as the border's and
        // the pointer's answers, and a truncated report is worse than a long one.
        let mut line = [0u8; 1024];
        // SAFETY: raw is an open capture, and the buffer is as large as its size.
        unsafe { yapfast_wgc_describe(self.raw, line.as_mut_ptr().cast(), line.len()) };
        text(&line)
    }

    /// Why the capture ended, when it did: the sentence the shim wrote for the
    /// [`Arrival::Closed`] it answered, and empty while the capture still runs.
    ///
    /// Both answers mean the same thing to a caller — stop, drop the capture — but
    /// a window whose owner closed it and a window that took itself out of every
    /// capture are different things to a person reading a log, and which one
    /// happened is worth a share saying rather than guessing.
    pub fn ending(&self) -> String {
        let mut line = [0u8; 256];
        // SAFETY: raw is an open capture, and the buffer is as large as its size.
        unsafe { yapfast_wgc_ending(self.raw, line.as_mut_ptr().cast(), line.len()) };
        text(&line)
    }

    /// Whether the yellow border is actually off: true only when
    /// `IsBorderRequired(false)` was asked and took. A capture on a Windows
    /// whose session has no interface to ask, or whose request to remove the
    /// border was refused, says false and is still a working capture.
    pub fn borderless(&self) -> bool {
        // SAFETY: raw is an open capture.
        unsafe { yapfast_wgc_borderless(self.raw) != 0 }
    }

    /// Whether this build of Windows has the interface that can take the border
    /// off at all: `IGraphicsCaptureSession3`, which exists from Windows 11
    /// 22H2. False means no answer to [`Capture::borderless`] could have been
    /// true, which is what a share choosing between capture ways needs to know:
    /// a Windows 10 has nothing to ask, and Desktop Duplication draws no border.
    pub fn can_ask_borderless(&self) -> bool {
        // SAFETY: raw is an open capture.
        unsafe { yapfast_wgc_can_ask_border(self.raw) != 0 }
    }

    /// The border's answer as the clause [`Capture::describe`] includes, for a
    /// share that lets this capture go for another way and has to say why.
    pub fn border(&self) -> String {
        let mut clause = [0u8; 192];
        // SAFETY: raw is an open capture, and the buffer is as large as its size.
        unsafe { yapfast_wgc_border(self.raw, clause.as_mut_ptr().cast(), clause.len()) };
        text(&clause)
    }
}

/// The build number of the Windows this process runs on, from ntdll's own
/// answer rather than the manifest-shimmed one; 0 when it can't be asked at all.
/// It is what says whether Graphics Capture has the interface that can take its
/// border off, before there is a capture to ask; see [`crate::capture`].
pub fn build() -> u32 {
    // SAFETY: the call takes nothing and answers a number.
    unsafe { yapfast_wgc_build() }
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

/// The shim's answer to a frame, read as the arrival it is. A number it doesn't
/// have is refused rather than taken as the nearest one: the two sides of this
/// boundary disagreeing about the vocabulary is a build that is out of step, and
/// reading it as `Idle` or as a frame would hide that.
fn arrival(found: c_int, why: &[u8]) -> Result<Arrival> {
    match found {
        FRAME => Ok(Arrival::Frame),
        IDLE => Ok(Arrival::Idle),
        RESIZED => Ok(Arrival::Resized),
        CLOSED => Ok(Arrival::Closed),
        FAILED => bail!("the capture stopped, having found no frame: {}", text(why)),
        other => bail!("the capture answered {other}, which native/wgc.cc doesn't have"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    // The one call these tests make to the system themselves. user32 is linked for
    // the shim's own use, and what is wanted here is a real window of somebody
    // else's process rather than one this test would have had to make and drive.
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut c_void;
    }

    /// The numbers native/wgc.cc answers with, read as the arrivals they mean, and
    /// one it doesn't have refused rather than taken as the nearest answer: this is
    /// the whole vocabulary of the boundary, and none of it needs a screen. The
    /// two answers that are not a frame are the ones worth pinning, since a window
    /// capture reaches both of them every day — a still window, and a window whose
    /// owner quit.
    #[test]
    fn the_shims_answers_are_read_as_arrivals_and_nothing_else() {
        let why: [u8; 0] = [];
        assert_eq!(arrival(FRAME, &why).expect("a frame"), Arrival::Frame);
        assert_eq!(arrival(IDLE, &why).expect("nothing new"), Arrival::Idle);
        assert_eq!(arrival(RESIZED, &why).expect("a resize"), Arrival::Resized);
        assert_eq!(
            arrival(CLOSED, &why).expect("a closed capture"),
            Arrival::Closed
        );
        let failed = arrival(FAILED, &why).expect_err("a failure");
        assert!(failed.to_string().contains("stopped"), "{failed}");
        let unknown = arrival(7, &why).expect_err("an answer the shim doesn't have");
        assert!(unknown.to_string().contains('7'), "{unknown}");
    }

    /// A window target has to name a window. The null handle a monitor uses for
    /// "the primary display" means nothing for a window — there is no default
    /// window, and guessing would capture the desktop or whatever is in front — so
    /// it is refused here, before the shim is asked, and the handle a target does
    /// carry is the one that reaches the shim's own constructor.
    #[test]
    fn a_window_that_names_nothing_is_refused_and_a_primary_display_is_not() {
        let refused = Target::from(Window::new(std::ptr::null_mut()))
            .named()
            .expect_err("a window with no handle");
        assert!(
            refused.to_string().contains("no window was named"),
            "{refused}"
        );
        assert!(Target::from(Monitor::PRIMARY).named().is_ok());

        let handle = 0x1c as *mut c_void;
        assert_eq!(Target::from(Window::new(handle)).split(), (true, handle));
        assert_eq!(Target::from(Monitor::new(handle)).split(), (false, handle));
    }

    /// A handle that is not a window is refused by the shim's own check, which runs
    /// before a device is made and before Windows Graphics Capture is started at
    /// all: the answer names the handle and says what it found. No window and no
    /// display is needed for this, which is why it runs rather than being ignored.
    #[test]
    fn a_handle_that_is_not_a_window_is_refused_and_named() {
        // A dangling pointer is a handle no window can be at, which is the kind of
        // stale handle a caller hands over after a window has gone.
        let nothing = std::ptr::dangling_mut::<c_void>();
        let Err(refused) = Capture::open(Window::new(nothing)) else {
            panic!("a handle that is no window opened as a capture");
        };
        let said = format!("{refused:#}");
        assert!(said.contains("is not a window any more"), "{said}");
        assert!(said.contains("IsWindow"), "{said}");
    }

    /// The window named is the window captured, and the shim says so: a capture
    /// asked for a window comes back describing that window rather than a display,
    /// with a size of its own, and hands over frames of it. Nothing is resized,
    /// minimised or closed here, because that would be driving a window this test
    /// doesn't own; the answers those need are what a live share has to show.
    #[test]
    #[ignore = "needs a real window on a running desktop: it captures the foreground window"]
    fn the_named_window_is_the_one_captured() {
        // SAFETY: the call takes nothing and answers the desktop's own handle.
        let window = unsafe { GetForegroundWindow() };
        if window.is_null() {
            eprintln!("no foreground window on this desktop, so there is nothing to capture");
            return;
        }
        let mut capture = Capture::open(Window::new(window)).expect("a capture of that window");
        let described = capture.describe();
        eprintln!("{described}");
        assert!(described.contains("the window \""), "{described}");
        let (width, height) = capture.content_size();
        assert!(width > 0 && height > 0, "the window is {width}x{height}");

        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            match capture.frame().expect("a frame") {
                Arrival::Frame => {
                    let texture = capture.texture();
                    let (width, height) = capture.content_size();
                    eprintln!("a frame of the window: {width}x{height}, texture {texture:p}");
                    assert!(!texture.is_null(), "a frame arrived with no texture");
                    assert!(
                        width > 0 && height > 0,
                        "a frame arrived at {width}x{height}"
                    );
                    return;
                }
                Arrival::Idle => std::thread::sleep(Duration::from_millis(5)),
                Arrival::Resized => {}
                Arrival::Closed => panic!(
                    "the window closed while it was being captured: {}",
                    capture.ending()
                ),
            }
        }
        eprintln!(
            "no frame in 10 s: that window isn't drawing, and the capture is open on it anyway"
        );
    }

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
            // Windows Graphics Capture makes a frame when the screen draws, not
            // when a clock ticks, so a still desktop is a capture with nothing to
            // hand over — not a failure. A quiet display says so and stops; the
            // open, and the geometry it reports, are what this asserts.
            if started.elapsed() > Duration::from_secs(10) {
                let (width, height) = capture.content_size();
                eprintln!(
                    "no frame in 10 s: this screen isn't drawing, and a capture has nothing to \
                     deliver until it does ({width}x{height}, open and started)"
                );
                return;
            }
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
