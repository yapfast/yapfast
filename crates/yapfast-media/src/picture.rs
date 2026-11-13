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

use livekit::webrtc::{prelude::VideoBuffer, video_frame::I420Buffer};

/// A picture's planes as native code lays them out; see `native/picture.c`.
#[repr(C)]
pub struct RawPicture {
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
    /// Converts `argb` into the I420 planes given, `width` by `height`, every
    /// channel through `tone` on the way; see `native/picture.c`.
    fn yapfast_tone_argb_to_i420(
        argb: *const c_uchar,
        stride: c_int,
        tone: *const c_uchar,
        y: *mut c_uchar,
        y_stride: c_int,
        u: *mut c_uchar,
        u_stride: c_int,
        v: *mut c_uchar,
        v_stride: c_int,
        width: c_int,
        height: c_int,
    );
    /// Scales `picture` into the I420 planes given, of `width` by `height`;
    /// see `native/picture.c`.
    fn yapfast_i420_scale(
        picture: *const RawPicture,
        y: *mut c_uchar,
        y_stride: c_int,
        u: *mut c_uchar,
        u_stride: c_int,
        v: *mut c_uchar,
        v_stride: c_int,
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
    pub unsafe fn from_raw(raw: &RawPicture) -> Option<Self> {
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

    pub fn raw(&self) -> RawPicture {
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
        self.check();
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

    /// Writes the picture scaled to `width` by `height` into `planes`, one
    /// y, u and v, whose rows are `strides` bytes apart: where a shared frame
    /// of another size goes, in the buffer kept for the frames after it.
    pub fn scale_into(
        &self,
        (y, u, v): (&mut [u8], &mut [u8], &mut [u8]),
        (y_stride, u_stride, v_stride): (u32, u32, u32),
        width: u32,
        height: u32,
    ) {
        self.check();
        let raw = self.raw();
        // SAFETY: the picture's planes hold its size at its strides, as
        // `check` says, and libyuv writes `width` by `height` of the planes
        // given, which their sizes say they hold.
        unsafe {
            yapfast_i420_scale(
                &raw,
                y.as_mut_ptr(),
                y_stride as c_int,
                u.as_mut_ptr(),
                u_stride as c_int,
                v.as_mut_ptr(),
                v_stride as c_int,
                width as c_int,
                height as c_int,
            );
        }
    }

    /// Checks the planes hold the picture's size at its strides, as libyuv
    /// reads them.
    fn check(&self) {
        let (w, h) = (self.width as usize, self.height as usize);
        let rows = [h, h.div_ceil(2), h.div_ceil(2)];
        let widths = [w, w.div_ceil(2), w.div_ceil(2)];
        for (index, plane) in [self.y, self.u, self.v].into_iter().enumerate() {
            let stride = self.strides[index] as usize;
            assert!(stride >= widths[index] && plane.len() >= stride * rows[index]);
        }
    }
}

/// What each of the 256 values a channel can take has on an SDR screen, for a
/// capture whose frames come from a screen showing HDR: the tonemap a share
/// needs, made once when the share starts and read for every pixel after.
///
/// A screen showing HDR hands its frames over as 8-bit codes with no word on
/// what they stand for, and libyuv reads them as sRGB's, which they are not:
/// PQ writes the white an SDR screen has as 149 of 255, so a desktop read that
/// way comes over dull grey where its white belongs, and the light above it
/// darker still. The table is what the codes mean, so that a share shows the
/// brightness of the screen it came from.
///
/// [`Tone::SDR`] is no table at all, which leaves a share of an SDR screen on
/// libyuv's own conversion: the frames are what they were, down to the byte,
/// and cost what they did.
#[derive(Clone)]
pub struct Tone(Option<Box<[u8; 256]>>);

impl Tone {
    /// What frames from a screen showing SDR come through: nothing at all.
    pub const SDR: Tone = Tone(None);

    /// What frames from a screen showing HDR come through: PQ's codes read as
    /// the SDR values they stand for.
    pub fn pq() -> Tone {
        let mut table = Box::new([0; 256]);
        for (code, value) in table.iter_mut().enumerate() {
            *value = sdr_code(pq_light(f64::from(code as u8) / 255.0));
        }
        Tone(Some(table))
    }

    /// The table this tone reads each channel of each pixel through, or none
    /// where the frames are SDR's already.
    pub fn table(&self) -> Option<&[u8; 256]> {
        self.0.as_deref()
    }

    /// Whether these are HDR's codes, which need a curve before they are a
    /// picture: a share whose path has no curve for them is a share of the wrong
    /// colour, which is what a bright window on an HDR screen looked like.
    pub fn is_hdr(&self) -> bool {
        self.0.is_some()
    }
}

/// Where the straight line up to SDR white stops, as a share of the range.
/// Everything below it is what it was, so that a desktop's greys keep their
/// brightness, and what is left above is the room a highlight rolls off in.
/// BT.2408 puts SDR white at about nine tenths of the signal a screen shows,
/// which is where a knee here leaves it.
const KNEE: f64 = 0.75;

/// The white an SDR screen has, in nits: BT.2408's reference, and what a
/// screen showing HDR means by white in the frames it hands over.
const SDR_WHITE: f64 = 203.0;

/// The brightest a screen showing HDR is taken to be, in nits: the top of the
/// range content is mastered in. A screen that peaks lower shows less, and
/// what it does show stays short of white rather than clipping at it.
const HDR_PEAK: f64 = 4000.0;

/// The light `code`, a PQ value from 0 to 1, stands for, in nits.
fn pq_light(code: f64) -> f64 {
    const M1: f64 = 2610.0 / 16384.0;
    const M2: f64 = 2523.0 / 4096.0 * 128.0;
    const C1: f64 = 3424.0 / 4096.0;
    const C2: f64 = 2413.0 / 4096.0 * 32.0;
    const C3: f64 = 2392.0 / 4096.0 * 32.0;
    let raised = code.powf(1.0 / M2);
    ((raised - C1).max(0.0) / (C2 - C3 * raised)).powf(1.0 / M1) * 10_000.0
}

/// The value an SDR screen would show `nits` of light as, 0 to 255.
fn sdr_code(nits: f64) -> u8 {
    let relative = nits / SDR_WHITE;
    // Reinhard's shoulder over what is left of the range, reaching white where
    // the content is mastered to peak: it rises with the light and never turns
    // back, so brighter stays brighter and the highlights keep their order
    // instead of flattening into one another at white.
    let shown = if relative <= KNEE {
        relative
    } else {
        let above = ((relative - KNEE) / (HDR_PEAK / SDR_WHITE - KNEE)).min(1.0);
        KNEE + (1.0 - KNEE) * 2.0 * above / (1.0 + above)
    };
    (srgb(shown) * 255.0).round().clamp(0.0, 255.0) as u8
}

/// sRGB's transfer function, in reverse: the signal a linear `value` is sent
/// as. The pixels a screen hands over are written this way, which is how libyuv
/// reads them.
fn srgb(value: f64) -> f64 {
    if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// Sends `argb` through `tone` into `picture`: a capture's conversion where the
/// screen it came from shows HDR, in place of libyuv's, which has no way to
/// tonemap on the way.
pub fn toned_into(picture: &mut I420Buffer, argb: &[u8], stride: u32, tone: &[u8; 256]) {
    let (width, height) = (picture.width(), picture.height());
    let (y_stride, u_stride, v_stride) = picture.strides();
    let rows = (height as usize).div_ceil(2);
    let (y, u, v) = picture.data_mut();
    // SAFETY: the frame holds `height` rows of `stride` bytes, which the caller
    // has of the capturer; the picture's planes hold its size at its strides;
    // and the table is all 256 values a channel can take.
    unsafe {
        yapfast_tone_argb_to_i420(
            argb.as_ptr(),
            stride as c_int,
            tone.as_ptr(),
            y.as_mut_ptr(),
            y_stride as c_int,
            u.as_mut_ptr(),
            u_stride as c_int,
            v.as_mut_ptr(),
            v_stride as c_int,
            width as c_int,
            height as c_int,
        );
    }
    debug_assert_eq!(u.len(), (u_stride as usize) * rows);
}

/// The tone a capture of the screen `monitor` needs, asked of the system: a
/// screen showing HDR hands its frames over as HDR's codes, and one that isn't
/// hands over SDR's, which are what they look like. What the system can't
/// answer for is taken as SDR: a share left as it is looks right, where a curve
/// over frames that never needed one doesn't.
#[cfg(windows)]
pub fn tone_of_screen(monitor: u64) -> Tone {
    if display::showing_hdr(monitor as *mut std::ffi::c_void) {
        Tone::pq()
    } else {
        Tone::SDR
    }
}

/// The tone a capture of the window `window` needs: the tone of the screen it
/// is shown on.
#[cfg(windows)]
pub fn tone_of_window(window: u64) -> Tone {
    // SAFETY: it reads no state of ours; a handle that isn't a window comes
    // back as the nearest screen's, and one that is no screen at all as none.
    let monitor = unsafe {
        display::MonitorFromWindow(window as *mut std::ffi::c_void, display::NEAREST_SCREEN)
    };
    tone_of_screen(monitor as u64)
}

/// The name the system gives the screen `monitor`, as its display settings show
/// it: `DISPLAY1` and the like, without the device path's prefix. Windows numbers
/// its screens that way, which is what a person's display arrangement and every
/// other app call them, and the capturer's own order is not that order.
#[cfg(windows)]
pub fn screen_name(monitor: u64) -> Option<String> {
    let device = display::device_name(monitor as *mut std::ffi::c_void)?;
    Some(device.trim_start_matches(r"\.\").to_owned())
}

#[cfg(not(windows))]
pub fn screen_name(_monitor: u64) -> Option<String> {
    None
}

/// Whether `monitor` is the desktop's primary screen.
#[cfg(windows)]
pub fn screen_is_primary(monitor: u64) -> bool {
    display::is_primary(monitor as *mut std::ffi::c_void)
}

#[cfg(not(windows))]
pub fn screen_is_primary(_monitor: u64) -> bool {
    false
}

/// On Linux the system's dialog picks what is shared, and what the compositor
/// hands over is what the screen shows: nothing to ask, and no curve.
#[cfg(not(windows))]
pub fn tone_of_screen(_monitor: u64) -> Tone {
    Tone::SDR
}

/// A window's screen, which the compositor hands over as it shows it.
#[cfg(not(windows))]
pub fn tone_of_window(_window: u64) -> Tone {
    Tone::SDR
}

/// Asking the system's display configuration whether a screen is showing HDR.
///
/// The frames themselves say nothing: a `DesktopFrame` is four bytes a pixel
/// whatever the screen shows, and no colour space comes with it. The system is
/// the only one that knows, so this is the only truthful word on whether a
/// share needs a curve at all — not a guess from the pixels, which would put a
/// curve over the frames of a screen showing SDR.
#[cfg(windows)]
mod display {
    use std::{ffi::c_void, mem::size_of};

    /// The system's name for a screen app is drawing on.
    pub(super) type Monitor = *mut c_void;

    /// Ask for the screen nearest the window, rather than none at all.
    pub(super) const NEAREST_SCREEN: u32 = 2;

    /// A locally unique id: which graphics adapter a target is on.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Luid {
        low: u32,
        high: i32,
    }

    /// A rate, as a fraction.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rational {
        numerator: u32,
        denominator: u32,
    }

    /// The head of every structure the display configuration is asked for.
    #[repr(C)]
    struct DeviceInfoHeader {
        kind: u32,
        size: u32,
        adapter: Luid,
        target: u32,
    }

    /// What a screen is called: the name the system knows it by, beside the one
    /// GDI knows it by, which is how a captured screen is matched to it.
    #[repr(C)]
    struct TargetName {
        header: DeviceInfoHeader,
        flags: u32,
        output_technology: u32,
        edid_manufacture: u16,
        edid_product: u16,
        connector: u32,
        monitor_path: [u16; 128],
        friendly_name: [u16; 64],
        gdi_name: [u16; 32],
    }

    /// Whether a target is showing HDR: the flags, then the encoding and the
    /// bits a channel it is showing them with.
    #[repr(C)]
    struct AdvancedColor {
        header: DeviceInfoHeader,
        value: u32,
        encoding: u32,
        bits_per_channel: u32,
    }

    /// The screen a path comes from.
    #[repr(C)]
    struct PathSource {
        adapter: Luid,
        id: u32,
        mode: u32,
        status: u32,
    }

    /// The target, or screen, that path is shown on.
    #[repr(C)]
    struct PathTarget {
        adapter: Luid,
        id: u32,
        mode: u32,
        output_technology: u32,
        rotation: u32,
        scaling: u32,
        refresh: Rational,
        scan_line: u32,
        available: u32,
        status: u32,
    }

    /// A screen, and what shows it.
    #[repr(C)]
    struct Path {
        source: PathSource,
        target: PathTarget,
        flags: u32,
    }

    /// A screen as GDI names it: the name a captured screen is known by.
    #[repr(C)]
    struct MonitorInfo {
        size: u32,
        monitor: [i32; 4],
        work: [i32; 4],
        flags: u32,
        device: [u16; 32],
    }

    // The sizes the system's own headers give these, which these must be to be
    // read by it at all.
    const _: () = {
        assert!(size_of::<DeviceInfoHeader>() == 20);
        assert!(size_of::<Path>() == 72);
        assert!(size_of::<TargetName>() == 484);
        assert!(size_of::<AdvancedColor>() == 32);
        assert!(size_of::<MonitorInfo>() == 104);
    };

    const GET_TARGET_NAME: u32 = 2;
    const GET_ADVANCED_COLOR_INFO: u32 = 9;
    /// The screen is showing HDR now, rather than being able to.
    const ADVANCED_COLOR_ENABLED: u32 = 0x2;
    /// The screens that are on, not every one that ever was.
    const ONLY_ACTIVE_PATHS: u32 = 2;

    #[allow(non_snake_case)]
    #[link(name = "user32")]
    unsafe extern "system" {
        pub(super) fn MonitorFromWindow(window: *mut c_void, flags: u32) -> *mut c_void;
        fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfo) -> i32;
        fn GetDisplayConfigBufferSizes(flags: u32, paths: *mut u32, modes: *mut u32) -> i32;
        fn QueryDisplayConfig(
            flags: u32,
            paths: *mut u32,
            path_array: *mut Path,
            modes: *mut u32,
            mode_array: *mut u64,
            topology: *mut u32,
        ) -> i32;
        fn DisplayConfigGetDeviceInfo(request: *mut DeviceInfoHeader) -> i32;
    }

    impl TargetName {
        fn of(adapter: Luid, target: u32) -> Self {
            Self {
                header: DeviceInfoHeader {
                    kind: GET_TARGET_NAME,
                    size: size_of::<Self>() as u32,
                    adapter,
                    target,
                },
                flags: 0,
                output_technology: 0,
                edid_manufacture: 0,
                edid_product: 0,
                connector: 0,
                monitor_path: [0; 128],
                friendly_name: [0; 64],
                gdi_name: [0; 32],
            }
        }
    }

    impl AdvancedColor {
        fn of(adapter: Luid, target: u32) -> Self {
            Self {
                header: DeviceInfoHeader {
                    kind: GET_ADVANCED_COLOR_INFO,
                    size: size_of::<Self>() as u32,
                    adapter,
                    target,
                },
                value: 0,
                encoding: 0,
                bits_per_channel: 0,
            }
        }
    }

    /// Whether the screen `monitor` is showing HDR now, as the system's display
    /// configuration says.
    pub(super) fn showing_hdr(monitor: Monitor) -> bool {
        let Some(device) = device_name(monitor) else {
            return false;
        };
        let Some((adapter, target)) = target_showing(&device) else {
            return false;
        };
        let mut info = AdvancedColor::of(adapter, target);
        // SAFETY: the header names the structure's kind and its size, and the
        // system writes nothing past it.
        let asked = unsafe { DisplayConfigGetDeviceInfo(&mut info.header) };
        asked == 0 && info.value & ADVANCED_COLOR_ENABLED != 0
    }

    /// The name GDI knows the screen `monitor` by, `\\.\DISPLAY1` and the like.
    /// Whether the screen is the desktop's primary one, as the system says.
    pub(super) fn is_primary(monitor: Monitor) -> bool {
        const PRIMARY: u32 = 1;
        let mut info = MonitorInfo {
            size: size_of::<MonitorInfo>() as u32,
            monitor: [0; 4],
            work: [0; 4],
            flags: 0,
            device: [0; 32],
        };
        // SAFETY: the size is the structure's, which is what the call reads.
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return false;
        }
        info.flags & PRIMARY != 0
    }

    pub(super) fn device_name(monitor: Monitor) -> Option<String> {
        let mut info = MonitorInfo {
            size: size_of::<MonitorInfo>() as u32,
            monitor: [0; 4],
            work: [0; 4],
            flags: 0,
            device: [0; 32],
        };
        // SAFETY: the size is the structure's, which is what the call reads.
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return None;
        }
        Some(name(&info.device))
    }

    /// The adapter and target of the path showing the screen named `device`.
    fn target_showing(device: &str) -> Option<(Luid, u32)> {
        let (mut paths, mut modes) = (0u32, 0u32);
        // SAFETY: it reads nothing; it only says how long the two arrays are.
        if unsafe { GetDisplayConfigBufferSizes(ONLY_ACTIVE_PATHS, &mut paths, &mut modes) } != 0 {
            return None;
        }
        // A path is the system's structure; only the adapter and target it
        // names are read here.
        let mut path_array: Vec<Path> = (0..paths).map(|_| Path::empty()).collect();
        // The modes are what the call insists on a place for, and what this
        // never looks at: eight words are as long as one is.
        let mut mode_array = vec![0u64; modes as usize * 8];
        let mut topology = 0u32;
        // SAFETY: both arrays are as long as the sizes the call asked for, and
        // they outlive it.
        if unsafe {
            QueryDisplayConfig(
                ONLY_ACTIVE_PATHS,
                &mut paths,
                path_array.as_mut_ptr(),
                &mut modes,
                mode_array.as_mut_ptr(),
                &mut topology,
            )
        } != 0
        {
            return None;
        }
        for path in path_array.iter().take(paths as usize) {
            let mut name = TargetName::of(path.target.adapter, path.target.id);
            // SAFETY: as in `showing_hdr`: the header names the kind and size.
            if unsafe { DisplayConfigGetDeviceInfo(&mut name.header) } != 0 {
                continue;
            }
            if name_of(&name.gdi_name) == device {
                return Some((path.target.adapter, path.target.id));
            }
        }
        None
    }

    impl Path {
        fn empty() -> Self {
            Self {
                source: PathSource {
                    adapter: Luid { low: 0, high: 0 },
                    id: 0,
                    mode: 0,
                    status: 0,
                },
                target: PathTarget {
                    adapter: Luid { low: 0, high: 0 },
                    id: 0,
                    mode: 0,
                    output_technology: 0,
                    rotation: 0,
                    scaling: 0,
                    refresh: Rational {
                        numerator: 0,
                        denominator: 0,
                    },
                    scan_line: 0,
                    available: 0,
                    status: 0,
                },
                flags: 0,
            }
        }
    }

    /// The text of `units`, a name the system wrote as UTF-16 up to its end.
    fn name_of(units: &[u16]) -> String {
        let end = units
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(units.len());
        String::from_utf16_lossy(&units[..end])
    }

    /// The same, from a GDI name whose array is the caller's.
    fn name(units: &[u16]) -> String {
        name_of(units)
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

/// How a picture turns to stand upright: quarter turns clockwise, then a
/// flip left to right, as a phone's camera says of each frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Turn {
    /// Quarter turns clockwise, 0 to 3.
    pub turns: u8,
    pub flip: bool,
}

impl Turn {
    /// The upright size of a `width` by `height` picture.
    pub fn upright(self, width: u32, height: u32) -> (u32, u32) {
        if self.turns % 2 == 1 {
            (height, width)
        } else {
            (width, height)
        }
    }
}

/// Turns `width` by `height` RGBA pixels a quarter clockwise (`turns` 1) or
/// three quarters (3) into `upright`, `height` by `width`.
fn turn_quarter(pixels: &[u8], width: u32, height: u32, turns: u8, upright: &mut [u8]) {
    let (width, height) = (width as usize, height as usize);
    let from = pixels.as_chunks::<4>().0;
    let to = &mut upright.as_chunks_mut::<4>().0[..width * height];
    // The upright picture is `height` wide: its row y is the source's
    // column y, read bottom up for a clockwise turn, top down otherwise.
    for (y, row) in to.chunks_exact_mut(height).enumerate() {
        for (x, pixel) in row.iter_mut().enumerate() {
            let (column, line) = if turns == 1 {
                (y, height - 1 - x)
            } else {
                (width - 1 - y, x)
            };
            *pixel = from[line * width + column];
        }
    }
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
        let mut sink = self.turning_sink(false);
        move |picture| sink(picture, Turn::default())
    }

    /// As [`Self::sink`], mirrored, as people see themselves: a camera
    /// filming its own person shows them as a mirror would.
    pub fn mirrored_sink(&self) -> impl FnMut(&I420<'_>) + Send + 'static {
        let mut sink = self.turning_sink(true);
        move |picture| sink(picture, Turn::default())
    }

    /// As [`Self::sink`], each picture turned upright as it says first, as
    /// a phone's camera sends them, and made at the size the upright
    /// picture fits where it shows. An upright picture costs nothing more;
    /// one turned a quarter is scaled into a buffer of the shown size, kept
    /// while pictures come turned, and turned from there into the frame.
    pub fn turning_sink(&self, mirror: bool) -> impl FnMut(&I420<'_>, Turn) + Send + 'static {
        let feed = self.clone();
        let (mut scratch, mut turned) = (vec![], vec![]);
        move |picture, turn| {
            let quarter = turn.turns % 2 == 1;
            let (upright_width, upright_height) = turn.upright(picture.width, picture.height);
            let Some((width, height)) = feed.size_for(upright_width, upright_height) else {
                return;
            };
            if !quarter {
                // Not needed while upright or upside down.
                turned = vec![];
            }
            feed.write(width, height, |rgba| {
                let rgba = &mut rgba[..width as usize * height as usize * 4];
                if quarter {
                    // Scaled as it came, on its side, then turned.
                    turned.resize(rgba.len(), 0);
                    picture.to_rgba(&mut scratch, &mut turned, height, width);
                    turn_quarter(&turned, height, width, turn.turns, rgba);
                } else {
                    picture.to_rgba(&mut scratch, rgba, width, height);
                    if turn.turns == 2 {
                        rgba.as_chunks_mut::<4>().0.reverse();
                    }
                }
                if mirror != turn.flip {
                    for row in rgba.chunks_exact_mut(width as usize * 4) {
                        row.as_chunks_mut::<4>().0.reverse();
                    }
                }
            });
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

    /// PQ's code for `nits` of light, the way a screen showing HDR writes it:
    /// what the tests below put through the table.
    fn pq_code(nits: f64) -> u8 {
        const M1: f64 = 2610.0 / 16384.0;
        const M2: f64 = 2523.0 / 4096.0 * 128.0;
        const C1: f64 = 3424.0 / 4096.0;
        const C2: f64 = 2413.0 / 4096.0 * 32.0;
        const C3: f64 = 2392.0 / 4096.0 * 32.0;
        let light = nits / 10_000.0;
        let encoded = ((C1 + C2 * light.powf(M1)) / (1.0 + C3 * light.powf(M1))).powf(M2);
        (encoded * 255.0).round() as u8
    }

    /// A frame from a screen showing SDR comes through as it is, which is no
    /// table at all: the conversion libyuv does is the conversion it got.
    #[test]
    fn an_sdr_frame_has_no_curve() {
        assert!(Tone::SDR.table().is_none());
        assert!(Tone::pq().table().is_some());
    }

    /// A screen showing HDR writes light of its own scale, and the grey an SDR
    /// screen shows as 118 of 255 comes over as 105: read as sRGB's the way
    /// libyuv reads it, that is a shade of a grey the desktop never showed.
    /// Through the table it comes out the grey it stood for.
    #[test]
    fn an_hdr_mid_grey_tonemaps_to_an_sdr_mid_grey() {
        let tone = Tone::pq();
        let table = tone.table().expect("a curve");
        // SDR's mid-grey is an eighth of white's light.
        let code = pq_code(SDR_WHITE * 0.18);
        assert!(
            (100..=110).contains(&code),
            "a mid-grey is written as {code}"
        );
        let shown = table[usize::from(code)];
        // SDR's own mid-grey is 118 of 255, or thereabouts: an eighth of the
        // light through sRGB's curve.
        assert!(
            (104..=132).contains(&shown),
            "a mid-grey sent as {code} shows as {shown}"
        );
        // Read as sRGB's, a PQ code is darker than the light it stands for:
        // the table brings it back up to the grey the desktop showed.
        assert!(shown > code, "it comes up: {shown} of {code}");
    }

    /// A highlight above SDR white rolls off short of white instead of
    /// clipping at it: the shape of what is bright stays, where reading the
    /// code as sRGB's would have flattened it into white.
    #[test]
    fn a_highlight_rolls_off_below_white() {
        let tone = Tone::pq();
        let table = tone.table().expect("a curve");
        let mid = table[usize::from(pq_code(SDR_WHITE * 0.18))];
        let highlight = table[usize::from(pq_code(1000.0))];
        assert!(
            (mid..=247).contains(&highlight),
            "a thousand nits shows as {highlight}"
        );
        assert!(
            table[usize::from(pq_code(4000.0))] > highlight,
            "brighter still is brighter"
        );
    }

    /// The curve never turns back on itself: the frame's order of brightness is
    /// the order it is shown in, so nothing that was apart is drawn as one.
    #[test]
    fn the_curve_never_turns_back() {
        let tone = Tone::pq();
        let table = tone.table().expect("a curve");
        assert!(table.windows(2).all(|pair| pair[0] <= pair[1]), "{table:?}");
    }

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

    #[test]
    fn turns_pictures_upright() {
        // 3 by 2, pixels numbered across then down:
        //   0 1 2
        //   3 4 5
        let pixels: Vec<u8> = (0..6u8).flat_map(|n| [n, 0, 0, 255]).collect();
        let turned = |turns| {
            let mut upright = vec![0; 24];
            turn_quarter(&pixels, 3, 2, turns, &mut upright);
            upright.chunks(4).map(|pixel| pixel[0]).collect::<Vec<_>>()
        };
        // A quarter clockwise, 2 by 3:  3 0 / 4 1 / 5 2.
        assert_eq!(turned(1), [3, 0, 4, 1, 5, 2]);
        // Three quarters:  2 5 / 1 4 / 0 3.
        assert_eq!(turned(3), [2, 5, 1, 4, 0, 3]);
        assert_eq!(
            Turn {
                turns: 1,
                flip: false
            }
            .upright(1280, 720),
            (720, 1280)
        );
        assert_eq!(
            Turn {
                turns: 2,
                flip: true
            }
            .upright(1280, 720),
            (1280, 720)
        );
    }

    /// A picture sent on its side shows upright, at the size the upright
    /// picture fits the tile, and upside down or flipped as it says.
    #[test]
    fn a_turning_sink_shows_pictures_upright() {
        // 4 by 2, the left half dark and the right light.
        let y = [16, 16, 235, 235, 16, 16, 235, 235];
        let (u, v) = ([128u8; 2], [128u8; 2]);
        let picture = I420 {
            width: 4,
            height: 2,
            y: &y,
            u: &u,
            v: &v,
            strides: [4, 2, 2],
        };
        let shown = |turn: Turn| {
            let feed = Feed::new();
            feed.show_at(Some((40, 40)));
            feed.turning_sink(false)(&picture, turn);
            feed.take(|buffer| {
                let plain = (buffer as &dyn Any).downcast_ref::<Plain>().unwrap();
                let (width, _) = plain.size;
                let light = |x: u32, y: u32| plain.pixels[((y * width + x) * 4) as usize] > 128;
                (plain.size, light(0, 0), light(width - 1, plain.size.1 - 1))
            })
            .unwrap()
        };
        let turns = |turns| Turn { turns, flip: false };
        assert_eq!(shown(turns(0)), ((4, 2), false, true), "as it came");
        // Clockwise, the left goes on top.
        assert_eq!(shown(turns(1)), ((2, 4), false, true), "a quarter");
        assert_eq!(shown(turns(2)), ((4, 2), true, false), "upside down");
        assert_eq!(shown(turns(3)), ((2, 4), true, false), "three quarters");
        assert_eq!(
            shown(Turn {
                turns: 0,
                flip: true
            }),
            ((4, 2), true, false),
            "flipped"
        );
    }
}
