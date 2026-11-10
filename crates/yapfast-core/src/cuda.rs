//! NVIDIA's card as the way a captured screen reaches its encoder on Linux:
//! `native/cudaconvert.c`'s import of a compositor's DMA-BUF through EGL and
//! CUDA, and the two kernels `native/cuda_convert.cu` was compiled to by
//! `build.rs`, which write the stream's picture as NV12 into memory that lives
//! on the card.
//!
//! Nothing of the picture crosses to the processor: the descriptor is imported,
//! the driver's own scale and convert run on the card, and an encoder handed
//! [`Convert::nv12`] reads it where it lies. What this can't take — a driver that
//! hands the picture back as a CUDA array rather than rows, a format that isn't
//! 32-bit BGRA, a device that isn't the one the picture came from — it says in a
//! sentence, which is what the share's ladder falls through on to the processor's
//! own copy.
//!
//! `Convert` is used by the share's encoder on one thread: the resources it holds
//! are the driver's, and they are not shared between threads.

use std::{
    ffi::{CStr, c_char, c_int, c_uint, c_ulonglong, c_void},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, bail};

use yapfast_discord::rtp::Codec;

use crate::dmabuf::Descriptor;

/// The kernels, as `build.rs` compiled them: PTX the driver loads at run time,
/// so no machine needs a CUDA toolkit to run this.
const PTX: &[u8] = include_bytes!(env!("YAPFAST_CUDA_PTX"));

/// The longest refusal the C side writes, in bytes.
const ERROR: usize = 256;

unsafe extern "C" {
    fn yapfast_cudaconvert_new(
        drm_fd: c_int,
        width: c_uint,
        height: c_uint,
        ptx: *const c_void,
        ptx_size: usize,
        error: *mut c_char,
        size: usize,
    ) -> *mut c_void;
    fn yapfast_cudaconvert_convert(
        convert: *mut c_void,
        fd: c_int,
        offset: c_uint,
        stride: c_uint,
        modifier: c_ulonglong,
        fourcc: c_uint,
        width: c_uint,
        height: c_uint,
        error: *mut c_char,
        size: usize,
    ) -> c_int;
    fn yapfast_cudaconvert_nv12(convert: *mut c_void) -> *mut c_void;
    fn yapfast_cudaconvert_context(convert: *mut c_void) -> *mut c_void;
    fn yapfast_cudaconvert_pitch(convert: *mut c_void) -> c_uint;
    fn yapfast_cudaconvert_device(convert: *mut c_void) -> *const c_char;
    fn yapfast_cudaconvert_driver(convert: *mut c_void) -> *const c_char;
    fn yapfast_cudaconvert_copied(convert: *mut c_void) -> c_ulonglong;
    fn yapfast_cudaconvert_free(convert: *mut c_void);
    fn yapfast_cudaenc_new(
        codec: c_uint,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        nv12: *mut c_void,
        pitch: c_uint,
        cuda_context: *mut c_void,
        error: *mut c_char,
        size: usize,
    ) -> *mut c_void;
    fn yapfast_cudaenc_encode(
        encoder: *mut c_void,
        pts: c_ulonglong,
        keyframe: c_int,
        out: *mut c_char,
        capacity: usize,
        error: *mut c_char,
        size: usize,
    ) -> std::ffi::c_long;
    fn yapfast_cudaenc_set_bitrate(
        encoder: *mut c_void,
        kbps: c_uint,
        error: *mut c_char,
        size: usize,
    ) -> c_int;
    fn yapfast_cudaenc_copied(encoder: *mut c_void) -> c_ulonglong;
    fn yapfast_cudaenc_tuning(encoder: *mut c_void, out: *mut c_char, size: usize);
    fn yapfast_cudaenc_config(encoder: *mut c_void, out: *mut c_char, size: usize);
    fn yapfast_cudaenc_error(encoder: *mut c_void) -> *const c_char;
    fn yapfast_cudaenc_free(encoder: *mut c_void);
}

/// A refusal's sentence, as the C side wrote it.
fn said(error: &[c_char; ERROR]) -> String {
    // SAFETY: the C side always leaves a terminated string in this buffer.
    let text = unsafe { CStr::from_ptr(error.as_ptr()) };
    let text = text.to_string_lossy();
    if text.is_empty() {
        "the card's convert refused the picture".to_owned()
    } else {
        text.into_owned()
    }
}

/// A picture on the card, as an encoder reads it: NV12 at `pitch` bytes a row,
/// the luma rows and then the interleaved chroma ones.
#[derive(Clone, Copy, Debug)]
pub struct Nv12 {
    pub pointer: *mut c_void,
    pub pitch: u32,
}

/// The C side's handle, shared between the converter that made it and any
/// session encoding its buffer: a `cuMemAlloc` pointer belongs to the context
/// that made it, so the context and the buffer outlive whichever of the two is
/// dropped first.
struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: made by this module, and let go of once.
        unsafe { yapfast_cudaconvert_free(self.0) }
    }
}

// SAFETY: the driver's handle is used by the thread that made the share.
unsafe impl Send for Handle {}

/// The import and the convert, opened on one DRM device and one stream size.
pub struct Convert {
    handle: Arc<Handle>,
    pitch: u32,
    device: String,
    driver: String,
    width: u32,
    height: u32,
    /// The render nodes not tried yet: a picture can only be imported by the
    /// device whose memory it is, and nothing hands the capture's device to us,
    /// so the one that takes it is found by trying.
    rest: Vec<PathBuf>,
}

// SAFETY: the handle is the driver's own and is handed to the encoder on the
// thread that made the share; nothing here shares it between threads.
unsafe impl Send for Convert {}

impl Convert {
    /// Opens the convert for a stream of `width` by `height` — the size the
    /// kernels scale every picture to — on the first render node that takes it.
    /// Every node opens; which one can import a picture is only known when one
    /// arrives, so [`Self::convert`] moves to the next when a device refuses.
    pub fn open(width: u32, height: u32) -> Result<Self> {
        let mut nodes = render_nodes()?;
        let mut why = String::new();
        while let Some(node) = (!nodes.is_empty()).then(|| nodes.remove(0)) {
            match Self::at(&node, width, height) {
                Ok(mut convert) => {
                    convert.rest = nodes;
                    return Ok(convert);
                }
                Err(error) => why = format!("{}: {error:#}", node.display()),
            }
        }
        Err(anyhow::anyhow!("no NVIDIA device took the convert ({why})"))
    }

    /// Opens on `node`, which is the device the pictures to come belong to.
    pub fn at(node: &Path, width: u32, height: u32) -> Result<Self> {
        let device = std::fs::File::open(node)
            .with_context(|| format!("couldn't open {}", node.display()))?;
        let mut error = [0 as c_char; ERROR];
        // SAFETY: the PTX is this binary's own and outlives the call; the file is
        // open for the length of it.
        let raw = unsafe {
            yapfast_cudaconvert_new(
                device.as_raw_fd(),
                width,
                height,
                PTX.as_ptr().cast(),
                PTX.len(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if raw.is_null() {
            bail!("{}", said(&error));
        }
        let mut convert = Self::from_raw(raw)?;
        convert.width = width;
        convert.height = height;
        Ok(convert)
    }

    fn from_raw(raw: *mut c_void) -> Result<Self> {
        let handle = Arc::new(Handle(raw));
        // SAFETY: `raw` is a handle this module made and nothing else holds.
        let (pitch, device, driver) = unsafe {
            (
                yapfast_cudaconvert_pitch(raw),
                CStr::from_ptr(yapfast_cudaconvert_device(raw))
                    .to_string_lossy()
                    .into_owned(),
                CStr::from_ptr(yapfast_cudaconvert_driver(raw))
                    .to_string_lossy()
                    .into_owned(),
            )
        };
        Ok(Self {
            handle,
            pitch,
            device,
            driver,
            width: 0,
            height: 0,
            rest: Vec::new(),
        })
    }

    /// Scale and convert one captured picture into the card's own memory, and
    /// give back where the encoder reads it.
    pub fn convert(&mut self, picture: &Descriptor<'_>) -> Result<Nv12> {
        loop {
            match self.import(picture) {
                Ok(nv12) => return Ok(nv12),
                Err(error) => {
                    // A device that isn't the picture's refuses to import it, and
                    // only the one whose memory it is takes it: try the next node
                    // once, and report the refusal of the last when none does.
                    //
                    // Not once a session has registered this buffer: replacing the
                    // handle frees the context and the memory that session reads,
                    // so the picture is refused rather than the stream broken.
                    if Arc::strong_count(&self.handle) > 1 {
                        return Err(error.context(
                            "another device's picture, and a session already reads this one's memory",
                        ));
                    }
                    let Some(next) = self.rest.first().cloned() else {
                        return Err(error);
                    };
                    self.rest.remove(0);
                    let mut moved = Self::at(&next, self.width, self.height)?;
                    // The nodes left are the ones this had not tried.
                    moved.rest = std::mem::take(&mut self.rest);
                    *self = moved;
                }
            }
        }
    }

    fn import(&mut self, picture: &Descriptor<'_>) -> Result<Nv12> {
        let plane = picture
            .planes
            .first()
            .context("the picture names no memory")?;
        let fd = *picture.fds.first().context("the picture has no memory")?;
        let mut error = [0 as c_char; ERROR];
        // SAFETY: the descriptor's own numbers; the memory stays the caller's for
        // as long as the picture is held, which it is here.
        let ok = unsafe {
            yapfast_cudaconvert_convert(
                self.handle.0,
                fd,
                plane.offset,
                plane.stride,
                picture.modifier,
                picture.fourcc,
                picture.width,
                picture.height,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if ok == 0 {
            bail!("{}", said(&error));
        }
        // SAFETY: the buffer belongs to the handle and is rewritten in place on
        // the next frame, which is the contract `Sink::dmabuf` keeps.
        let pointer = unsafe { yapfast_cudaconvert_nv12(self.handle.0) };
        Ok(Nv12 {
            pointer,
            pitch: self.pitch,
        })
    }

    /// The CUDA context the picture lives in, which an encoder has to register
    /// the buffer in.
    pub fn context(&self) -> *mut c_void {
        // SAFETY: the handle is this module's.
        unsafe { yapfast_cudaconvert_context(self.handle.0) }
    }

    /// Where the next picture will be: the buffer the kernels write, and the
    /// pitch it is laid out at. The buffer is the same one frame after frame, and
    /// an encoder registered on it reads whatever the last convert left there.
    pub fn nv12(&self) -> Nv12 {
        // SAFETY: the handle is this module's.
        let pointer = unsafe { yapfast_cudaconvert_nv12(self.handle.0) };
        Nv12 {
            pointer,
            pitch: self.pitch,
        }
    }

    /// What opened it, for the share's report: the card and its driver.
    pub fn describe(&self) -> String {
        format!("{} · {}", self.device, self.driver)
    }

    /// Frames' pixels moved by the processor: this path moves none, and says so.
    pub fn copied(&self) -> u64 {
        // SAFETY: the handle is this module's.
        unsafe { yapfast_cudaconvert_copied(self.handle.0) }
    }
}

// `Convert` has no `Drop` of its own: the [`Handle`] it shares with any session
// encoding the buffer is what frees the driver's side, once, when the last of
// them is gone.

/// The render nodes this machine has, in the order they're tried: the first is
/// whichever driver's `renderD128` is, which is the one a single-card desktop
/// captured from. `renderD` and not `card`: a picture is imported where the
/// compositor drew it, and DRI render nodes are what can be opened without
/// owning the display.
fn render_nodes() -> Result<Vec<PathBuf>> {
    let mut nodes: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
        .context("there are no DRI devices")?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("renderD"))
        })
        .collect();
    nodes.sort();
    if nodes.is_empty() {
        bail!("this machine has no DRI render node");
    }
    Ok(nodes)
}

/// An NVENC session on the buffer [`Convert`] fills: the other half of the path,
/// and the one that makes it zero-copy. The picture is registered once, at the
/// open, and every frame after that is a submission of memory that is already on
/// the card — no pixel of it is converted again or sent over the bus by the
/// processor.
pub struct Encoder {
    raw: *mut c_void,
    /// The converter's handle, held so the context and the buffer this session
    /// registered outlive any drop of the [`Convert`] that made them. Never read:
    /// it is the keeping that matters.
    _keep: Arc<Handle>,
    codec: Codec,
    /// The buffer an access unit is handed back in, grown when one doesn't fit.
    out: Vec<u8>,
}

// SAFETY: the session is the driver's and is used by the thread that made the
// share, as [`Convert`] is.
unsafe impl Send for Encoder {}

/// One encoded picture, as it goes out: the access unit and whether it stands on
/// its own.
pub struct Packet {
    pub data: Vec<u8>,
    pub keyframe: bool,
}

/// The number this build calls `codec` by, which the C side takes: 0 H.264, 1
/// HEVC, 2 AV1. VP8 is libvpx's on the processor and never the card's.
fn codec_number(codec: Codec) -> Result<c_uint> {
    Ok(match codec {
        Codec::H264 => 0,
        Codec::H265 => 1,
        Codec::Av1 => 2,
        Codec::Vp8 => bail!("VP8 doesn't encode on the card; libvpx does it on the processor"),
    })
}

impl Encoder {
    /// Opens a session in `convert`'s own CUDA context, encoding the buffer
    /// [`Convert`] fills, at `width` by `height` and `fps`, aiming for `kbps`.
    pub fn open(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        convert: &Convert,
    ) -> Result<Self> {
        if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            bail!("{codec:?} frames need an even size, not {width}x{height}");
        }
        let number = codec_number(codec)?;
        let nv12 = convert.nv12();
        let mut error = [0 as c_char; ERROR];
        // SAFETY: the buffer and its context belong to `convert`, which outlives
        // the session; the driver reads them, and neither is copied here.
        let raw = unsafe {
            yapfast_cudaenc_new(
                number,
                width,
                height,
                fps,
                kbps,
                nv12.pointer,
                nv12.pitch,
                convert.context(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if raw.is_null() {
            bail!(
                "no NVENC encoder opened on the card's own memory: {}",
                said(&error)
            );
        }
        Ok(Self {
            raw,
            _keep: Arc::clone(&convert.handle),
            codec,
            // The driver's own bitstream buffer is 4 MiB, which a 1440p keyframe
            // can pass: a unit bigger than the buffer it is handed back in is one
            // this path can't deliver, and the picture is already the session's,
            // so submitting it again would reference a frame nobody got. Room for
            // two bytes a pixel, made once.
            out: vec![0; ((width as usize * height as usize) * 2).max(4 << 20)],
        })
    }

    /// Submits the picture the buffer holds now, at `pts` in the share's clock.
    /// `keyframe` asks for an IDR, which the session makes on demand.
    pub fn encode(&mut self, pts: u64, keyframe: bool) -> Result<Option<Packet>> {
        let mut error = [0 as c_char; ERROR];
        // SAFETY: the session is open, the buffer has its length, and the picture
        // in the card's memory is the one the last convert wrote.
        let written = unsafe {
            yapfast_cudaenc_encode(
                self.raw,
                pts as c_ulonglong,
                c_int::from(keyframe),
                self.out.as_mut_ptr().cast(),
                self.out.len(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        match written {
            ..0 => bail!("{:?} encoding failed: {}", self.codec, said(&error)),
            0 => Ok(None),
            size => Ok(Some(Packet {
                data: self.out[..size as usize].to_vec(),
                keyframe,
            })),
        }
    }

    /// Reconfigures the open session's rate, which is what Discord's estimate
    /// moving does to a stream.
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        let mut error = [0 as c_char; ERROR];
        // SAFETY: the session is open for the length of the call.
        let ok =
            unsafe { yapfast_cudaenc_set_bitrate(self.raw, kbps, error.as_mut_ptr(), error.len()) };
        if ok == 0 {
            bail!("{}", said(&error));
        }
        Ok(())
    }

    /// Frames' pixels moved by the processor: this path moves none.
    pub fn copied(&self) -> u64 {
        // SAFETY: the session is open.
        unsafe { yapfast_cudaenc_copied(self.raw) }
    }

    /// What the session was opened with, and what it dropped, in the words the
    /// share's report and the log use.
    pub fn tuning(&self) -> String {
        let mut text = [0 as c_char; 96];
        // SAFETY: the session is open, and the buffer is a buffer it fills.
        unsafe { yapfast_cudaenc_tuning(self.raw, text.as_mut_ptr(), text.len()) };
        // SAFETY: the C side leaves a terminated string.
        unsafe { CStr::from_ptr(text.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    pub fn config(&self) -> String {
        let mut text = [0 as c_char; 320];
        // SAFETY: as [`Self::tuning`].
        unsafe { yapfast_cudaenc_config(self.raw, text.as_mut_ptr(), text.len()) };
        // SAFETY: the C side leaves a terminated string.
        unsafe { CStr::from_ptr(text.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    /// The last refusal, in the driver's words.
    pub fn error(&self) -> String {
        // SAFETY: the session is open; the string belongs to it.
        let said = unsafe { CStr::from_ptr(yapfast_cudaenc_error(self.raw)) };
        said.to_string_lossy().into_owned()
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: made by this module, and let go of once, after the convert's
        // buffer, which the session merely reads.
        unsafe { yapfast_cudaenc_free(self.raw) }
    }
}

/// A session on the card's own memory asks NVENC for what a share is measured
/// with, and not for something else: `native/hwenc.c`'s table is the one answer,
/// and this path's settings are held to it rather than to a copy of it. A tuning
/// changed on one side and not the other would encode a picture nobody measured,
/// and neither a stream nor a comparison would say which it got.
#[cfg(all(test, hwenc))]
mod tests {
    use super::{CStr, c_uint};

    unsafe extern "C" {
        fn yapfast_cudaenc_tuning_settings(
            codec: c_uint,
            fps: c_uint,
            kbps: c_uint,
            out: *mut std::ffi::c_char,
            size: usize,
        );
        fn yapfast_hwenc_nvenc_settings(
            tuning: *const std::ffi::c_char,
            asked: *mut std::ffi::c_char,
            asked_size: usize,
        ) -> std::ffi::c_int;
    }

    fn text(buffer: &[u8]) -> String {
        // SAFETY: both sides end their buffer with a zero.
        let text = unsafe { CStr::from_ptr(buffer.as_ptr().cast()) };
        text.to_string_lossy().into_owned()
    }

    #[test]
    fn the_cards_own_memory_is_encoded_with_the_settings_a_share_is_measured_with() {
        let mut ffmpeg = [0u8; 192];
        let mut cuda = [0u8; 192];
        // SAFETY: no tuning named asks for the shipped row; both buffers are the
        // size given, and both sides end theirs with a zero.
        let status = unsafe {
            yapfast_hwenc_nvenc_settings(
                std::ptr::null(),
                ffmpeg.as_mut_ptr().cast(),
                ffmpeg.len(),
            );
            yapfast_cudaenc_tuning_settings(0, 30, 4_000, cuda.as_mut_ptr().cast(), cuda.len());
            0
        };
        assert_eq!(status, 0);
        let (ours, theirs) = (text(&cuda), text(&ffmpeg));
        assert!(
            ours.starts_with(&theirs),
            "the card's own memory asks NVENC for `{ours}`, and a share is measured at `{theirs}`"
        );
    }
}
