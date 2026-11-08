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

    /// A window this test makes and owns, so that a capture of one can be driven
    /// through what its owner does to it — resized, minimised, restored, closed,
    /// taken out of every capture — instead of through whatever the desktop's
    /// foreground window happens to be doing.
    ///
    /// It is a `STATIC`, one of the classes Windows registers for itself: a window
    /// of the system's own, with a background brush and a title, so the compositor
    /// has something to fill a frame with without this test registering a class
    /// and handing user32 a structure whose layout has to be right on two
    /// architectures for no answer the system's own class doesn't already give.
    ///
    /// The ten calls are declared here rather than taken from the `windows`
    /// crate's `Win32_UI_WindowsAndMessaging` bindings: they take plain arguments,
    /// and that feature is code the whole program would carry on Windows for a
    /// window that only exists in a test. user32 and kernel32 are linked already.
    mod own {
        use std::ffi::c_void;

        // SAFETY: each is declared as Windows declares it, and every pointer
        // passed is a window this module made, a buffer of its own, or null.
        unsafe extern "system" {
            #[allow(clippy::too_many_arguments)]
            fn CreateWindowExW(
                ex_style: u32,
                class: *const u16,
                title: *const u16,
                style: u32,
                x: i32,
                y: i32,
                width: i32,
                height: i32,
                parent: *mut c_void,
                menu: *mut c_void,
                instance: *mut c_void,
                param: *mut c_void,
            ) -> *mut c_void;
            fn DestroyWindow(window: *mut c_void) -> i32;
            fn ShowWindow(window: *mut c_void, command: i32) -> i32;
            #[allow(clippy::too_many_arguments)]
            fn SetWindowPos(
                window: *mut c_void,
                after: *mut c_void,
                x: i32,
                y: i32,
                width: i32,
                height: i32,
                flags: u32,
            ) -> i32;
            fn SetWindowDisplayAffinity(window: *mut c_void, affinity: u32) -> i32;
            fn PeekMessageW(
                message: *mut c_void,
                window: *mut c_void,
                first: u32,
                last: u32,
                remove: u32,
            ) -> i32;
            fn TranslateMessage(message: *const c_void) -> i32;
            fn DispatchMessageW(message: *const c_void) -> isize;
            fn GetCurrentProcess() -> *mut c_void;
            fn GetProcessHandleCount(process: *mut c_void, count: *mut u32) -> i32;
        }

        /// `WS_OVERLAPPEDWINDOW`: a title bar and a sizeable frame, which is the
        /// window a person has and a capture of one meets.
        const OVERLAPPED: u32 = 0x00CF_0000;
        const CHILD: u32 = 0x4000_0000;
        const VISIBLE: u32 = 0x1000_0000;
        const SW_MINIMIZE: i32 = 6;
        const SW_RESTORE: i32 = 9;
        const PM_REMOVE: u32 = 1;
        const SWP_NOMOVE: u32 = 0x0002;
        const SWP_NOZORDER: u32 = 0x0004;
        const SWP_NOACTIVATE: u32 = 0x0010;
        /// `WDA_EXCLUDEFROMCAPTURE`: out of every capture, Windows Graphics
        /// Capture's and the compositor's alike. Windows 10 2004 added it.
        const EXCLUDED_FROM_CAPTURE: u32 = 0x0000_0011;

        /// A string Windows takes: UTF-16 with the zero it ends at.
        fn wide(text: &str) -> Vec<u16> {
            text.encode_utf16().chain([0]).collect()
        }

        /// A window this test made, destroyed when it goes.
        pub(super) struct Owned {
            window: *mut c_void,
        }

        impl Owned {
            /// A shown, title-barred window of `size` that this process owns.
            pub(super) fn new((width, height): (i32, i32)) -> Self {
                let class = wide("STATIC");
                let title = wide("yapfast's own test window");
                // SAFETY: both strings end with a zero and outlive the call; a
                // `STATIC` is a class Windows registers itself, so no instance, no
                // parent and no menu are needed.
                let window = unsafe {
                    CreateWindowExW(
                        0,
                        class.as_ptr(),
                        title.as_ptr(),
                        OVERLAPPED | VISIBLE,
                        40,
                        40,
                        width,
                        height,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                };
                assert!(!window.is_null(), "a window of this test's own");
                Self { window }
            }

            /// The handle a capture of it is opened with.
            pub(super) fn handle(&self) -> *mut c_void {
                self.window
            }

            /// A window of the same class inside this one, whose capture item would
            /// be this window's rather than its own.
            pub(super) fn child(&self) -> *mut c_void {
                let class = wide("STATIC");
                let title = wide("");
                // SAFETY: both strings end with a zero and outlive the call, and the
                // parent is this test's own window, which destroys its children
                // when it goes.
                unsafe {
                    CreateWindowExW(
                        0,
                        class.as_ptr(),
                        title.as_ptr(),
                        CHILD | VISIBLE,
                        0,
                        0,
                        20,
                        20,
                        self.window,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                }
            }

            /// Answers the messages a shown window is sent, which is what paints it:
            /// the compositor has a picture of a window that has answered its own
            /// `WM_PAINT`, and nothing of one that hasn't.
            pub(super) fn pump(&self) {
                // Room for a `MSG` without declaring one: the calls below fill it
                // and it is handed straight back, and nothing here reads a field.
                let mut message = [0u64; 16];
                // SAFETY: the buffer is larger than a `MSG` and aligned like one,
                // and every message taken out of the queue is dispatched before the
                // next is asked for.
                unsafe {
                    while PeekMessageW(
                        message.as_mut_ptr().cast(),
                        std::ptr::null_mut(),
                        0,
                        0,
                        PM_REMOVE,
                    ) != 0
                    {
                        TranslateMessage(message.as_ptr().cast());
                        DispatchMessageW(message.as_ptr().cast());
                    }
                }
            }

            /// Drags the window to `size`, as a person does with the frame.
            pub(super) fn resize(&self, (width, height): (i32, i32)) {
                // SAFETY: the window is this test's own and alive, and the flags
                // leave its place and its order on the desktop alone.
                let moved = unsafe {
                    SetWindowPos(
                        self.window,
                        std::ptr::null_mut(),
                        0,
                        0,
                        width,
                        height,
                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                };
                assert!(moved != 0, "a window of this test's own is resized");
            }

            /// Minimises it, which is a window that composites nothing.
            pub(super) fn minimise(&self) {
                // SAFETY: the window is this test's own and alive.
                unsafe { ShowWindow(self.window, SW_MINIMIZE) };
            }

            /// Puts it back on the desktop at its own size.
            pub(super) fn restore(&self) {
                // SAFETY: the window is this test's own and alive.
                unsafe { ShowWindow(self.window, SW_RESTORE) };
            }

            /// Takes it out of every capture, as a password manager's window does.
            /// False on a Windows that won't set the flag, which is a Windows this
            /// can't be shown on rather than a capture that went wrong.
            pub(super) fn exclude_from_capture(&self) -> bool {
                // SAFETY: the window is this test's own and alive, and the affinity
                // is the one Windows defines for it.
                unsafe { SetWindowDisplayAffinity(self.window, EXCLUDED_FROM_CAPTURE) != 0 }
            }

            /// Closes it, as its owner does when they quit: what is left is a handle
            /// no window is behind any more.
            pub(super) fn close(&mut self) {
                // SAFETY: a window this test made and hasn't given back is destroyed
                // once, and the handle is dropped here so the drop below has nothing
                // left to do.
                unsafe { DestroyWindow(self.window) };
                self.window = std::ptr::null_mut();
            }
        }

        impl Drop for Owned {
            fn drop(&mut self) {
                if !self.window.is_null() {
                    // SAFETY: a window this test made and hasn't destroyed yet.
                    unsafe { DestroyWindow(self.window) };
                }
            }
        }

        /// How many handles this process holds, which is one of the things a
        /// capture that opened and closed without letting go of its device or its
        /// pool grows.
        pub(super) fn handles() -> u32 {
            let mut count = 0;
            // SAFETY: the process is this one and the count is a place of its own.
            let asked = unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) };
            assert!(asked != 0, "this process's handle count can be asked");
            count
        }
    }

    /// What the card has of its own memory in use, as DXGI reports it: the whole
    /// machine's usage, not this process's, which is why a test prints it instead
    /// of asserting on it. `None` on a machine whose driver doesn't answer.
    fn video_usage() -> Option<u64> {
        use windows::Win32::Graphics::Dxgi::{
            CreateDXGIFactory1, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, IDXGIAdapter3, IDXGIFactory1,
        };
        use windows::core::Interface;

        // SAFETY: DXGI is asked for its factory, then for its first adapter, and
        // both are released when they go out of scope.
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
            let adapter = factory.EnumAdapters1(0).ok()?;
            let budget: IDXGIAdapter3 = adapter.cast().ok()?;
            let mut info = Default::default();
            budget
                .QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info)
                .ok()?;
            Some(info.CurrentUsage)
        }
    }

    /// The next answer that isn't "nothing new", pumping the window meanwhile for
    /// up to `wait`: a still desktop delivers nothing, which is not a failure.
    fn wait(capture: &mut Capture, window: &own::Owned, wait: Duration) -> Option<Arrival> {
        let started = Instant::now();
        while started.elapsed() < wait {
            window.pump();
            match capture.frame().expect("the shim's answer") {
                Arrival::Idle => std::thread::sleep(Duration::from_millis(5)),
                arrival => return Some(arrival),
            }
        }
        None
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

    /// A window that is not a window of its own is refused by name, before
    /// anything is opened: the desktop, which is a display and is captured as
    /// one, and a child window, whose capture item would be its application's
    /// whole window. A share that promised one window and handed over the screen
    /// behind it is the thing this refuses, and the desktop's handle is a real
    /// one that no check of nullness could have caught.
    #[test]
    fn the_desktop_and_a_child_window_are_refused_as_a_window() {
        let owner = own::Owned::new((320, 240));
        let child = owner.child();
        assert!(!child.is_null(), "a child of this test's own window");
        let mut refused = Vec::new();
        for (why, window) in [
            ("the desktop", unsafe { GetDesktopWindow() }),
            ("the shell's window", unsafe { GetShellWindow() }),
            ("a child window", child),
        ] {
            if window.is_null() {
                eprintln!("this desktop has no {why} to ask about");
                continue;
            }
            let Err(error) = Capture::open(Window::new(window)) else {
                panic!("{why} opened as a capture of a window");
            };
            refused.push((why, format!("{error:#}")));
        }
        for (why, said) in &refused {
            // Each refusal names what it found: the desktop is one, a child's
            // top-level window is named as the window a capture of it would be.
            assert!(
                said.contains("desktop") || said.contains("top-level window"),
                "{why}: {said}"
            );
        }
        assert_eq!(refused.len(), 3, "all three were refused");
    }

    /// A window its owner closes ends the capture, and the end names a window
    /// rather than a display: the shim asks Windows whether a window is still
    /// behind the handle before it touches the pool, so this is the same answer on
    /// a desktop where nothing ever draws, which is what makes it a test that
    /// needs no picture. It is how a share of somebody's window finishes when
    /// they quit.
    #[test]
    fn a_window_that_closes_ends_the_capture() {
        let mut window = own::Owned::new((320, 240));
        let mut capture = Capture::open(Window::new(window.handle()))
            .expect("a capture of a window this test made");
        assert!(
            capture.describe().contains("the window \""),
            "what is captured is a window rather than a display: {}",
            capture.describe()
        );
        window.close();
        assert_eq!(
            capture.frame().expect("the shim's answer"),
            Arrival::Closed,
            "a capture of a window that closed is over"
        );
        // Either sentence is the same end: the item's own `Closed` event fires as
        // the window goes, and the per-frame check finds the same thing when the
        // event is slower than the next look.
        let ending = capture.ending();
        assert!(
            ending.to_lowercase().contains("window"),
            "the end names the window: {ending:?}"
        );
    }

    /// A window that takes itself out of every capture is one no picture of can be
    /// had, so a capture of it ends rather than handing over the last picture it
    /// took: the pool still holds one from before the flag was set, and sending
    /// that again is the content its owner kept out of captures.
    #[test]
    fn a_window_that_leaves_every_capture_ends_it() {
        let window = own::Owned::new((320, 240));
        let mut capture = Capture::open(Window::new(window.handle()))
            .expect("a capture of a window this test made");
        if !window.exclude_from_capture() {
            eprintln!(
                "this Windows won't set WDA_EXCLUDEFROMCAPTURE on a window, so a window that \
                 leaves every capture can't be shown here"
            );
            return;
        }
        assert_eq!(
            capture.frame().expect("the shim's answer"),
            Arrival::Closed,
            "a capture of a window that left every capture is over"
        );
        drop(capture);
        // Which of the two things ended it is the shim's own sentence, and a
        // capture opened after the flag is set is one no `Closed` event has fired
        // on: it answers with the flag itself.
        let mut after = Capture::open(Window::new(window.handle()))
            .expect("a capture of a window that left every capture");
        assert_eq!(
            after.frame().expect("the shim's answer"),
            Arrival::Closed,
            "a window that is out of every capture hands over nothing"
        );
        assert!(
            after.ending().contains("WDA_EXCLUDEFROMCAPTURE"),
            "the end is the window's own exclusion: {:?}",
            after.ending()
        );
    }

    /// A minimised window composites nothing, so a capture of one answers that
    /// there is nothing new — not a thumbnail at the window's own size, which
    /// would read as a resize a frame — and a restored window is a picture again.
    #[test]
    fn a_minimised_window_hands_over_nothing_new_until_it_is_restored() {
        let window = own::Owned::new((320, 240));
        window.pump();
        let mut capture = Capture::open(Window::new(window.handle()))
            .expect("a capture of a window this test made");
        window.minimise();
        window.pump();
        assert_eq!(
            capture.frame().expect("the shim's answer"),
            Arrival::Idle,
            "a minimised window is nothing new"
        );
        window.restore();
        window.pump();
        match wait(&mut capture, &window, Duration::from_secs(10)) {
            // A window put back on the desktop is the picture it was, at its own
            // size, or at worst one that reports the size it came back at.
            Some(Arrival::Frame | Arrival::Resized) => {}
            Some(other) => panic!("a restored window is a picture again, not {other:?}"),
            None => eprintln!(
                "no picture in 10 s: this desktop isn't drawing the window, and the capture is \
                 open on it"
            ),
        }
    }

    /// A window dragged to a new size is captured at the new one: the shim rebuilds
    /// its pool at the frame's own size and answers that the size changed, so an
    /// encoder built for the old size is never handed a picture at the new one.
    /// This is the one transition that needs the compositor: a window that nothing
    /// draws has no frame to report a size with, so a desktop that gives none says
    /// so instead of failing.
    #[test]
    fn a_resized_window_is_captured_at_its_new_size() {
        let window = own::Owned::new((320, 240));
        window.pump();
        let mut capture = Capture::open(Window::new(window.handle()))
            .expect("a capture of a window this test made");
        if wait(&mut capture, &window, Duration::from_secs(10)).is_none() {
            eprintln!(
                "no picture in 10 s: this desktop isn't drawing the window, so a resize can't be \
                 seen here"
            );
            return;
        }
        let (before_width, before_height) = capture.content_size();
        window.resize((480, 360));
        window.pump();
        let started = Instant::now();
        let mut resized = None;
        while started.elapsed() < Duration::from_secs(10) {
            window.pump();
            match capture.frame().expect("the shim's answer") {
                Arrival::Resized => {
                    resized = Some(capture.content_size());
                    break;
                }
                // A frame the compositor handed over at the size before the new
                // one, which is nothing to report.
                Arrival::Frame | Arrival::Idle => std::thread::sleep(Duration::from_millis(5)),
                Arrival::Closed => panic!(
                    "the window closed rather than being resized: {}",
                    capture.ending()
                ),
            }
        }
        let Some((width, height)) = resized else {
            panic!(
                "a window resized to 480x360 was never reported at its new size, from \
                 {before_width}x{before_height}"
            );
        };
        assert!(
            width > before_width && height > before_height,
            "{width}x{height} came of a window that was {before_width}x{before_height}"
        );
    }

    /// Switching between windows again and again leaves the process where it was:
    /// the device, the frame pool and the session that a capture of the window let
    /// go of are all given back with it, which is what a share that changes what
    /// it shares over and over must not grow. Two windows are taken in turns, the
    /// way a share switches — one capture let go of before the other opens.
    ///
    /// Handles are counted because they are this process's own. What the card has
    /// in use is printed rather than asserted: DXGI's usage is the whole machine's
    /// and not this share's, so a difference between two readings of it is
    /// nobody's in particular.
    #[test]
    fn switching_between_windows_again_and_again_takes_no_handles_with_it() {
        let first = own::Owned::new((320, 240));
        let second = own::Owned::new((480, 360));
        // One capture of each first: the first device, the driver's first session
        // and the thread pool behind them take handles once, and those aren't this
        // test's to count.
        for window in [first.handle(), second.handle()] {
            drop(Capture::open(Window::new(window)).expect("a capture of a window this test made"));
        }
        let before = own::handles();
        let usage = video_usage();
        for turn in 0..20 {
            // The capture of what was shared before is let go before this one
            // opens, as a switch does.
            let window = if turn % 2 == 0 {
                first.handle()
            } else {
                second.handle()
            };
            drop(
                Capture::open(Window::new(window)).expect("a capture of the window this turn took"),
            );
        }
        let after = own::handles();
        assert!(
            after <= before + 2,
            "20 switches between two windows took {} handles with them ({before} to {after})",
            after.saturating_sub(before)
        );
        eprintln!(
            "20 switches between two windows: {before} handles to {after}, card memory {usage:?} \
             to {:?}",
            video_usage()
        );
    }

    // Two window calls of Windows' own, for the desktop and the shell's window: the
    // handles a capture of a window must take as a display rather than as a window.
    unsafe extern "system" {
        fn GetDesktopWindow() -> *mut c_void;
        fn GetShellWindow() -> *mut c_void;
    }
}
