//! NVIDIA's card as the way a captured screen reaches its encoder on Linux:
//! `native/cudaconvert.c`'s import of a compositor's DMA-BUF through EGL and
//! CUDA, and the two kernels `native/cuda_convert.cu` was compiled to by
//! `build.rs`, which write the stream's picture as NV12 into memory that lives
//! on the card.
//!
//! Nothing of the picture crosses to the processor: the descriptor is imported,
//! the driver's own scale and convert run on the card, and an encoder handed
//! [`Convert::nv12`] reads it where it lies. A tiled picture — a compositor on
//! NVIDIA's driver shares one — is an array CUDA can't map, so GL copies it on
//! the card into a texture CUDA can, and the kernels read that through a texture
//! object. What this can't take — a
//! format that isn't 32-bit BGRA, a picture the driver holds in neither rows nor
//! an array, a device that isn't the one the picture came from — it says in a
//! sentence, which is what the share's ladder falls through on to the processor's
//! own copy. A screen showing HDR is no longer one of them: the kernels read the
//! codes of such a picture as the light they stand for and write SDR's, on the
//! way, so the stream is SDR's and no pixel of it comes back for a curve either.
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

use yapfast_discord::{
    rtp::Codec,
    stream_connection::{Encoded, Pool},
};

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
        tone: c_int,
        error: *mut c_char,
        size: usize,
    ) -> c_int;
    fn yapfast_cudaconvert_nv12(convert: *mut c_void) -> *mut c_void;
    fn yapfast_cudaconvert_context(convert: *mut c_void) -> *mut c_void;
    fn yapfast_cudaconvert_pitch(convert: *mut c_void) -> c_uint;
    fn yapfast_cudaconvert_device(convert: *mut c_void) -> *const c_char;
    fn yapfast_cudaconvert_driver(convert: *mut c_void) -> *const c_char;
    fn yapfast_cudaconvert_copied(convert: *mut c_void) -> c_ulonglong;
    fn yapfast_cudaconvert_still(
        convert: *mut c_void,
        y: *mut u8,
        y_stride: c_uint,
        u: *mut u8,
        u_stride: c_uint,
        v: *mut u8,
        v_stride: c_uint,
    ) -> c_int;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_cudaconvert_preview(
        convert: *mut c_void,
        width: c_uint,
        height: c_uint,
        y: *mut u8,
        y_stride: c_uint,
        u: *mut u8,
        u_stride: c_uint,
        v: *mut u8,
        v_stride: c_uint,
    ) -> c_int;
    fn yapfast_cudaconvert_error(convert: *mut c_void) -> *const c_char;
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
        error: *mut c_char,
        size: usize,
    ) -> std::ffi::c_long;
    fn yapfast_cudaenc_drain(
        encoder: *mut c_void,
        out: *mut u8,
        capacity: usize,
    ) -> std::ffi::c_long;
    fn yapfast_cudaenc_set_bitrate(
        encoder: *mut c_void,
        kbps: c_uint,
        error: *mut c_char,
        size: usize,
    ) -> c_int;
    fn yapfast_cudaenc_copied(encoder: *mut c_void) -> c_ulonglong;
    fn yapfast_cudaenc_codecs(encoder: *mut c_void) -> c_uint;
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
// SAFETY: a shared `Handle` does nothing with its pointer but free it, once,
// when the last of the converter and its session lets go; the calls that use
// the pointer are made through those two, which aren't shared between threads.
unsafe impl Sync for Handle {}

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
    ///
    /// A machine with two cards has a node that is not the picture's, and the
    /// reason it gives is as much of the answer as the refusal of the card that
    /// is; every node's is kept, so the share's log names the one that matters.
    pub fn open(width: u32, height: u32) -> Result<Self> {
        let mut nodes = render_nodes()?;
        let mut why = Vec::new();
        while let Some(node) = (!nodes.is_empty()).then(|| nodes.remove(0)) {
            match Self::at(&node, width, height) {
                Ok(mut convert) => {
                    convert.rest = nodes;
                    return Ok(convert);
                }
                Err(error) => why.push(format!("{}: {error:#}", node.display())),
            }
        }
        Err(anyhow::anyhow!(
            "no NVIDIA device took the convert ({})",
            why.join("; ")
        ))
    }

    /// Opens on `node`, which is the device the pictures to come belong to.
    pub fn at(node: &Path, width: u32, height: u32) -> Result<Self> {
        let device = std::fs::File::open(node)
            .with_context(|| format!("couldn't open {}", node.display()))?;
        let mut error = [0 as c_char; ERROR];
        // The driver takes the PTX as a C string and reads it to the null byte at
        // the end, which `include_bytes!` doesn't have: it is the file's own
        // bytes. One copy at the open, where the picture's first frame is still
        // to come, rather than a driver that reads past them.
        let ptx = std::ffi::CString::new(PTX).context("the PTX is a file of text")?;
        // SAFETY: the PTX is this binary's own and outlives the call; the file is
        // open for the length of it.
        let raw = unsafe {
            yapfast_cudaconvert_new(
                device.as_raw_fd(),
                width,
                height,
                ptx.as_ptr().cast(),
                ptx.as_bytes().len(),
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
    ///
    /// `hdr` says the picture's codes are HDR's, which the display's own answer
    /// says and not a guess from the pixels: the kernels then apply the curve as
    /// they scale and convert, so the NV12 the encoder reads is SDR's and the
    /// picture still never crosses to the processor. A screen showing SDR passes
    /// false, and nothing in the kernels changes for it.
    pub fn convert(&mut self, picture: &Descriptor<'_>, hdr: bool) -> Result<Nv12> {
        loop {
            match self.import(picture, hdr) {
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
                            "a session already reads this converter's memory, so no other device \
                             is tried",
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

    fn import(&mut self, picture: &Descriptor<'_>, hdr: bool) -> Result<Nv12> {
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
                c_int::from(hdr),
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

    /// The stream's size, which every picture is scaled to.
    pub fn size(&self) -> (u32, u32) {
        (self.width & !1, self.height & !1)
    }

    /// The picture the last convert made, at [`Self::size`], copied back into
    /// `y`, `u` and `v` as I420 at the strides given: what Discord's still is
    /// made from every few minutes. The one copy of a picture this path makes,
    /// and only when it is asked for.
    pub fn still(
        &self,
        planes: (&mut [u8], &mut [u8], &mut [u8]),
        strides: [u32; 3],
    ) -> Result<()> {
        self.read_back(None, planes, strides)
    }

    /// The last picture that came through GL's door — a tiled screen's —
    /// scaled and converted on the card to `size` and copied back as I420: the
    /// preview of a window that draws on the processor, which can't sample the
    /// picture where it lies. Only the preview's pixels cross the bus. A picture
    /// the driver held as rows left no copy on the card to make one from, and
    /// is refused: the processor can read its rows itself.
    pub fn preview(
        &self,
        size: (u32, u32),
        planes: (&mut [u8], &mut [u8], &mut [u8]),
        strides: [u32; 3],
    ) -> Result<()> {
        self.read_back(Some((size.0 & !1, size.1 & !1)), planes, strides)
    }

    /// The still at the stream's size, or the preview at `preview`'s.
    fn read_back(
        &self,
        preview: Option<(u32, u32)>,
        (y, u, v): (&mut [u8], &mut [u8], &mut [u8]),
        strides: [u32; 3],
    ) -> Result<()> {
        let (width, height) = preview.unwrap_or_else(|| self.size());
        let needs = |stride: u32, columns: u32, rows: u32, plane: &[u8]| {
            stride >= columns && plane.len() >= stride as usize * rows as usize
        };
        if !(needs(strides[0], width, height, y)
            && needs(strides[1], width / 2, height / 2, u)
            && needs(strides[2], width / 2, height / 2, v))
        {
            bail!("the planes don't hold a {width}x{height} picture");
        }
        // SAFETY: each plane holds its rows at its stride, checked above, and the
        // handle is this module's.
        let ok = unsafe {
            match preview {
                None => yapfast_cudaconvert_still(
                    self.handle.0,
                    y.as_mut_ptr(),
                    strides[0],
                    u.as_mut_ptr(),
                    strides[1],
                    v.as_mut_ptr(),
                    strides[2],
                ),
                Some((width, height)) => yapfast_cudaconvert_preview(
                    self.handle.0,
                    width,
                    height,
                    y.as_mut_ptr(),
                    strides[0],
                    u.as_mut_ptr(),
                    strides[1],
                    v.as_mut_ptr(),
                    strides[2],
                ),
            }
        };
        if ok == 0 {
            // SAFETY: the string belongs to the handle, which is alive.
            let why = unsafe { CStr::from_ptr(yapfast_cudaconvert_error(self.handle.0)) };
            bail!("{}", why.to_string_lossy());
        }
        Ok(())
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
    /// The buffers access units are copied into and sent from, reused as their
    /// frames go out: the one copy of a frame's bytes between the driver and the
    /// packetizer, as the encoders FFmpeg opens make theirs.
    out: Arc<Pool>,
}

// SAFETY: the session is the driver's and is used by the thread that made the
// share, as [`Convert`] is.
unsafe impl Send for Encoder {}

/// One encoded picture, as it goes out: the access unit and whether it stands on
/// its own.
pub struct Packet {
    pub data: Encoded,
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
            // The unit's size is known before it is copied, so the pool grows to
            // hold it: up to two bytes a pixel, a keyframe of noise, beyond which
            // no unit of this size goes.
            out: Arc::new(Pool::new(width as usize * height as usize * 2 + 4096)),
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
                error.as_mut_ptr(),
                error.len(),
            )
        };
        let size = match written {
            ..0 => bail!("{:?} encoding failed: {}", self.codec, said(&error)),
            0 => return Ok(None),
            size => size as usize,
        };
        // A unit too large for the pool still goes to the drain, which lets the
        // driver's buffer go and says so.
        while self.out.capacity() < size && self.out.grow() {}
        let mut data = self.out.checkout();
        let spare = data.spare();
        // SAFETY: the session is open and holds the unit the encode above
        // produced; the buffer is `spare.len()` bytes and stays put meanwhile.
        let copied = unsafe { yapfast_cudaenc_drain(self.raw, spare.as_mut_ptr(), spare.len()) };
        if copied <= 0 {
            bail!("{:?} encoding lost its frame: {}", self.codec, self.error());
        }
        data.finish(copied as usize);
        Ok(Some(Packet { data, keyframe }))
    }

    /// Reconfigures the open session's rate, which is what Discord's estimate
    /// moving does to a stream.
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        let mut error = [0 as c_char; ERROR];
        // The C side answers 0 for a session reconfigured and -1 for a refusal.
        // SAFETY: the session is open for the length of the call.
        let status =
            unsafe { yapfast_cudaenc_set_bitrate(self.raw, kbps, error.as_mut_ptr(), error.len()) };
        if status != 0 {
            bail!("{}", said(&error));
        }
        Ok(())
    }

    /// Frames' pixels moved by the processor: this path moves none.
    pub fn copied(&self) -> u64 {
        // SAFETY: the session is open.
        unsafe { yapfast_cudaenc_copied(self.raw) }
    }

    /// The codecs this session's driver encodes, most wanted first, or `None`
    /// when the driver wouldn't say which: what a share offers Discord, asked of
    /// the session it keeps rather than of one opened for each codec and given up
    /// beside it. The driver answers it from one session whichever codec that
    /// session encodes.
    pub fn driver_codecs(&self) -> Option<Vec<Codec>> {
        // SAFETY: the session is open.
        let supported = unsafe { yapfast_cudaenc_codecs(self.raw) };
        (supported != 0).then(|| {
            [Codec::Av1, Codec::H265, Codec::H264]
                .into_iter()
                .filter(|codec| {
                    codec_number(*codec).is_ok_and(|number| supported & (1 << number) != 0)
                })
                .collect()
        })
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

/// A converter let go of leaves the others on the same card working: EGL hands
/// every converter on one device the same display, and a share that switches
/// codec lets one converter go while it opens another. Needs NVIDIA's card and
/// driver, and says so rather than failing where they aren't.
#[cfg(all(test, nvcuda))]
mod displays {
    use super::Convert;
    use crate::dmabuf::{Descriptor, Plane};
    use std::os::fd::AsRawFd;

    #[test]
    fn a_converter_let_go_of_leaves_the_others_on_its_card_importing() {
        let (first, mut kept) = match (Convert::open(64, 64), Convert::open(64, 64)) {
            (Ok(first), Ok(kept)) => (first, kept),
            (Err(error), _) | (_, Err(error)) => {
                eprintln!("no NVIDIA card path here: {error:#}");
                return;
            }
        };
        drop(first);
        // Memory that is no picture: the import refuses it, and what matters is
        // that it got as far as asking, with its display still initialised.
        let null = std::fs::File::open("/dev/null").expect("/dev/null opens");
        let planes = [Plane {
            offset: 0,
            stride: 256,
        }];
        let fds = [null.as_raw_fd()];
        let picture = Descriptor {
            width: 64,
            height: 64,
            fourcc: 0x3432_5258,
            modifier: 0,
            planes: &planes,
            colour: crate::dmabuf::Colour::SDR,
            fds: &fds,
        };
        let error = format!(
            "{:#}",
            kept.import(&picture).expect_err("/dev/null is no picture")
        );
        assert!(
            !error.contains("wouldn't enter"),
            "the converter kept lost its display with the one let go of: {error}"
        );
    }

    /// Opening a converter leaves no CUDA context current on the thread that
    /// opened it. `cuCtxCreate` makes the context it creates current, and one left
    /// there outlived its converter: a codec switch destroys the context, and the
    /// thread went on with a destroyed context under every push and pop after.
    #[test]
    fn opening_a_converter_leaves_no_cuda_context_current() {
        let convert = match Convert::open(64, 64) {
            Ok(convert) => convert,
            Err(error) => {
                eprintln!("no NVIDIA card path here: {error:#}");
                return;
            }
        };
        // SAFETY: the converter has libcuda loaded, so RTLD_NOLOAD finds it, and
        // cuCtxGetCurrent's signature is the driver API's own.
        let current = unsafe {
            let library =
                libc::dlopen(c"libcuda.so.1".as_ptr(), libc::RTLD_NOW | libc::RTLD_NOLOAD);
            assert!(!library.is_null(), "the converter loaded libcuda");
            let symbol = libc::dlsym(library, c"cuCtxGetCurrent".as_ptr());
            assert!(!symbol.is_null(), "libcuda has cuCtxGetCurrent");
            let get: unsafe extern "C" fn(*mut *mut std::ffi::c_void) -> i32 =
                std::mem::transmute(symbol);
            let mut context = std::ptr::null_mut();
            assert_eq!(get(&mut context), 0, "cuCtxGetCurrent answers");
            libc::dlclose(library);
            context
        };
        drop(convert);
        assert!(
            current.is_null(),
            "the converter's context is still current on the thread that opened it"
        );
    }

    /// A session's driver names every codec it encodes, whatever codec that
    /// session encodes: what a share offers Discord, read from the session it
    /// keeps rather than from a session opened for each codec and given up
    /// beside it.
    #[test]
    fn a_session_names_the_codecs_its_driver_encodes() {
        let Ok(convert) = Convert::open(320, 180) else {
            eprintln!("no NVIDIA card path here");
            return;
        };
        let Ok(h264) = super::Encoder::open(super::Codec::H264, 320, 180, 30, 2_000, &convert)
        else {
            eprintln!("no NVENC on this card here");
            return;
        };
        let named = h264
            .driver_codecs()
            .expect("the driver says what it encodes");
        assert!(
            named.contains(&super::Codec::H264),
            "every card here encodes H264: {named:?}"
        );
        // Most wanted first, as Discord's client orders them.
        let mut order = [super::Codec::Av1, super::Codec::H265, super::Codec::H264].into_iter();
        assert!(
            named.iter().all(|codec| order.any(|next| next == *codec)),
            "in order: {named:?}"
        );
        // The session that encodes one codec names the others too, which is why
        // a share opens no second one to ask. The first session goes before the
        // next opens, as a share's does.
        drop(h264);
        let Ok(av1) = super::Encoder::open(super::Codec::Av1, 320, 180, 30, 2_000, &convert) else {
            eprintln!("this card has no AV1 encoder");
            return;
        };
        assert_eq!(
            av1.driver_codecs()
                .expect("the driver says what it encodes"),
            named,
            "one list of codecs, whichever codec the session encodes"
        );
    }
}

/// The card's path end to end, on the picture a compositor on NVIDIA's driver
/// shares: a tiled buffer (KWin's `0x0300000000e08014`) made by the driver's own
/// gbm, imported, scaled and converted on the card, encoded by NVENC, and
/// decoded again, whose colours are the ones drawn. Converters are opened for
/// every codec first and all but one let go of, as a share does. Needs NVIDIA's
/// card, driver and gbm backend, and says so rather than failing where they
/// aren't.
#[cfg(all(test, nvcuda))]
mod card_path {
    use super::{Convert, Encoder};
    use crate::decode::Decoder;
    use crate::dmabuf::{Descriptor, Plane};
    use std::ffi::{CStr, c_int, c_void};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use yapfast_discord::rtp::Codec;

    const RED: [f32; 3] = [1.0, 0.0, 0.0];
    const GREEN: [f32; 3] = [0.0, 1.0, 0.0];

    /// The modifier KWin's screencast hands over on NVIDIA's driver.
    const KWIN_TILED: u64 = 0x0300_0000_00e0_8014;
    const XRGB8888: u32 = 0x3432_5258;
    const ARGB8888: u32 = 0x3432_5241;
    const GBM_BO_USE_RENDERING: u32 = 1 << 2;

    /// libgbm's few entry points, dlopened as the converter opens its own.
    struct Gbm {
        library: *mut c_void,
        device: *mut c_void,
        _node: std::fs::File,
    }

    impl Gbm {
        fn function<T: Copy>(&self, name: &str) -> T {
            let name = std::ffi::CString::new(name).unwrap();
            // SAFETY: the library is open; the name is a C string.
            let found = unsafe { libc::dlsym(self.library, name.as_ptr()) };
            assert!(!found.is_null(), "libgbm has no {name:?}");
            // SAFETY: T is the function's type as gbm.h declares it.
            unsafe { std::mem::transmute_copy(&found) }
        }

        fn open() -> Result<Self, String> {
            let node = (128..136)
                .map(|n| format!("renderD{n}"))
                .find(|name| {
                    std::fs::read_link(format!("/sys/class/drm/{name}/device/driver"))
                        .is_ok_and(|driver| driver.ends_with("nvidia"))
                })
                .ok_or("no render node of NVIDIA's driver")?;
            let node = std::fs::File::options()
                .read(true)
                .write(true)
                .open(format!("/dev/dri/{node}"))
                .map_err(|error| error.to_string())?;
            // SAFETY: a library name; null is an answer.
            let library = unsafe { libc::dlopen(c"libgbm.so.1".as_ptr(), libc::RTLD_NOW) };
            if library.is_null() {
                // SAFETY: dlerror's string is valid until the next dl call.
                return Err(unsafe { CStr::from_ptr(libc::dlerror()) }
                    .to_string_lossy()
                    .into_owned());
            }
            let mut gbm = Self {
                library,
                device: std::ptr::null_mut(),
                _node: node,
            };
            let create: unsafe extern "C" fn(c_int) -> *mut c_void =
                gbm.function("gbm_create_device");
            // SAFETY: the node is open for as long as the device is.
            gbm.device = unsafe { create(gbm._node.as_raw_fd()) };
            if gbm.device.is_null() {
                return Err("gbm made no device on NVIDIA's node".into());
            }
            Ok(gbm)
        }

        /// A buffer of `width` by `height` the driver tiles, filled with red on
        /// the left half and green on the right: its descriptor, stride, offset
        /// and the modifier it was laid out with.
        fn tiled(&self, width: u32, height: u32) -> Result<(OwnedFd, u32, u32, u64), String> {
            let with_modifiers: unsafe extern "C" fn(
                *mut c_void,
                u32,
                u32,
                u32,
                *const u64,
                u32,
            ) -> *mut c_void = self.function("gbm_bo_create_with_modifiers");
            let plain: unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32) -> *mut c_void =
                self.function("gbm_bo_create");
            // SAFETY: the device is open; the list is one modifier long.
            let mut bo =
                unsafe { with_modifiers(self.device, width, height, XRGB8888, &KWIN_TILED, 1) };
            if bo.is_null() {
                // SAFETY: as above.
                bo = unsafe { plain(self.device, width, height, XRGB8888, GBM_BO_USE_RENDERING) };
            }
            if bo.is_null() {
                return Err("gbm made no buffer".into());
            }
            let modifier: unsafe extern "C" fn(*mut c_void) -> u64 =
                self.function("gbm_bo_get_modifier");
            let stride: unsafe extern "C" fn(*mut c_void) -> u32 =
                self.function("gbm_bo_get_stride");
            let offset: unsafe extern "C" fn(*mut c_void, c_int) -> u32 =
                self.function("gbm_bo_get_offset");
            let fd: unsafe extern "C" fn(*mut c_void) -> c_int = self.function("gbm_bo_get_fd");
            let destroy: unsafe extern "C" fn(*mut c_void) = self.function("gbm_bo_destroy");
            // SAFETY: the buffer is alive; each call reads it.
            let (raw, stride, offset, modifier) =
                unsafe { (fd(bo), stride(bo), offset(bo, 0), modifier(bo)) };
            // The descriptor holds the memory; the buffer object is let go of.
            // SAFETY: made above, let go of once.
            unsafe { destroy(bo) };
            if raw < 0 {
                return Err("the buffer exported no descriptor".into());
            }
            // SAFETY: a descriptor gbm made for this caller.
            let fd = unsafe { OwnedFd::from_raw_fd(raw) };
            self.draw(&fd, (stride, offset, modifier), (width, height), RED, GREEN)?;
            Ok((fd, stride, offset, modifier))
        }

        /// Red on the left half and green on the right, drawn into the buffer by
        /// GL on the card, as a compositor draws its screen: the driver won't map
        /// a tiled buffer for the processor to write.
        fn draw(
            &self,
            fd: &OwnedFd,
            (stride, offset, modifier): (u32, u32, u64),
            (width, height): (u32, u32),
            left: [f32; 3],
            right: [f32; 3],
        ) -> Result<(), String> {
            // SAFETY: a library name; null is an answer.
            let egl = unsafe { libc::dlopen(c"libEGL.so.1".as_ptr(), libc::RTLD_NOW) };
            if egl.is_null() {
                return Err("no libEGL".into());
            }
            // SAFETY: the library is open, and the name a C string.
            let get_proc: unsafe extern "C" fn(*const libc::c_char) -> *mut c_void =
                unsafe { std::mem::transmute(libc::dlsym(egl, c"eglGetProcAddress".as_ptr())) };
            macro_rules! f {
                ($name:literal: $($ty:tt)*) => {{
                    // SAFETY: the name is a C string; null is checked.
                    let found = unsafe { get_proc(concat!($name, "\0").as_ptr().cast()) };
                    if found.is_null() {
                        return Err(format!("EGL has no {}", $name));
                    }
                    // SAFETY: the type is the one the headers declare.
                    unsafe { std::mem::transmute::<*mut c_void, $($ty)*>(found) }
                }};
            }
            type P = *mut c_void;
            let get_display =
                f!("eglGetPlatformDisplayEXT": unsafe extern "C" fn(u32, P, *const i32) -> P);
            let initialize =
                f!("eglInitialize": unsafe extern "C" fn(P, *mut i32, *mut i32) -> u32);
            let bind_api = f!("eglBindAPI": unsafe extern "C" fn(u32) -> u32);
            let choose = f!("eglChooseConfig": unsafe extern "C" fn(P, *const i32, *mut P, i32, *mut i32) -> u32);
            let create_context =
                f!("eglCreateContext": unsafe extern "C" fn(P, P, P, *const i32) -> P);
            let make_current = f!("eglMakeCurrent": unsafe extern "C" fn(P, P, P, P) -> u32);
            let create_image =
                f!("eglCreateImageKHR": unsafe extern "C" fn(P, P, u32, P, *const i32) -> P);
            let destroy_image = f!("eglDestroyImageKHR": unsafe extern "C" fn(P, P) -> u32);
            let destroy_context = f!("eglDestroyContext": unsafe extern "C" fn(P, P) -> u32);
            let terminate = f!("eglTerminate": unsafe extern "C" fn(P) -> u32);
            let get_error = f!("eglGetError": unsafe extern "C" fn() -> i32);
            let gen_textures = f!("glGenTextures": unsafe extern "C" fn(i32, *mut u32));
            let bind_texture = f!("glBindTexture": unsafe extern "C" fn(u32, u32));
            let target = f!("glEGLImageTargetTexture2DOES": unsafe extern "C" fn(u32, P));
            let gen_framebuffers = f!("glGenFramebuffers": unsafe extern "C" fn(i32, *mut u32));
            let bind_framebuffer = f!("glBindFramebuffer": unsafe extern "C" fn(u32, u32));
            let attach =
                f!("glFramebufferTexture2D": unsafe extern "C" fn(u32, u32, u32, u32, i32));
            let status = f!("glCheckFramebufferStatus": unsafe extern "C" fn(u32) -> u32);
            let enable = f!("glEnable": unsafe extern "C" fn(u32));
            let scissor = f!("glScissor": unsafe extern "C" fn(i32, i32, i32, i32));
            let clear_color = f!("glClearColor": unsafe extern "C" fn(f32, f32, f32, f32));
            let clear = f!("glClear": unsafe extern "C" fn(u32));
            let finish = f!("glFinish": unsafe extern "C" fn());
            let delete_framebuffers =
                f!("glDeleteFramebuffers": unsafe extern "C" fn(i32, *const u32));
            let delete_textures = f!("glDeleteTextures": unsafe extern "C" fn(i32, *const u32));
            const NONE: i32 = 0x3038;
            let null: P = std::ptr::null_mut();
            // SAFETY: every call is EGL's and GL's as their headers declare
            // them, on the display, context and image made here, let go of
            // before the function returns.
            unsafe {
                let display = get_display(0x31D7, self.device, std::ptr::null());
                if display.is_null() || initialize(display, null.cast(), null.cast()) == 0 {
                    return Err("EGL wouldn't open gbm's display".into());
                }
                bind_api(0x30A2);
                let mut config = null;
                let mut found = 0;
                choose(
                    display,
                    [0x3040, 0x8, NONE].as_ptr(),
                    &mut config,
                    1,
                    &mut found,
                );
                let context = create_context(
                    display,
                    if found > 0 { config } else { null },
                    null,
                    [NONE].as_ptr(),
                );
                if context.is_null() || make_current(display, null, null, context) == 0 {
                    return Err(format!(
                        "no GL context on gbm's display (0x{:x})",
                        get_error()
                    ));
                }
                let attribs = [
                    0x3057,
                    width as i32,
                    0x3056,
                    height as i32,
                    0x3271,
                    ARGB8888 as i32,
                    0x3272,
                    fd.as_raw_fd(),
                    0x3273,
                    offset as i32,
                    0x3274,
                    stride as i32,
                    0x3443,
                    (modifier & 0xffff_ffff) as u32 as i32,
                    0x3444,
                    (modifier >> 32) as i32,
                    NONE,
                ];
                let image = create_image(display, null, 0x3270, null, attribs.as_ptr());
                if image.is_null() {
                    return Err(format!(
                        "GL wouldn't import the buffer (0x{:x})",
                        get_error()
                    ));
                }
                let (mut texture, mut framebuffer) = (0, 0);
                gen_textures(1, &mut texture);
                bind_texture(0x0DE1, texture);
                target(0x0DE1, image);
                gen_framebuffers(1, &mut framebuffer);
                bind_framebuffer(0x8D40, framebuffer);
                attach(0x8D40, 0x8CE0, 0x0DE1, texture, 0);
                let complete = status(0x8D40) == 0x8CD5;
                if complete {
                    enable(0x0C11);
                    let half = (width / 2) as i32;
                    scissor(0, 0, half, height as i32);
                    clear_color(left[0], left[1], left[2], 1.0);
                    clear(0x4000);
                    scissor(half, 0, width as i32 - half, height as i32);
                    clear_color(right[0], right[1], right[2], 1.0);
                    clear(0x4000);
                    finish();
                }
                delete_framebuffers(1, &framebuffer);
                delete_textures(1, &texture);
                destroy_image(display, image);
                make_current(display, null, null, null);
                destroy_context(display, context);
                terminate(display);
                if !complete {
                    return Err("GL can't draw into the buffer".into());
                }
            }
            Ok(())
        }
    }

    impl Drop for Gbm {
        fn drop(&mut self) {
            // A device that didn't open is nothing to destroy.
            if self.device.is_null() {
                return;
            }
            let destroy: unsafe extern "C" fn(*mut c_void) = self.function("gbm_device_destroy");
            // SAFETY: made by `open`, let go of once.
            unsafe { destroy(self.device) };
        }
    }

    /// The decoded picture's luma and chroma a quarter of the way in from each
    /// side, halfway down: the red half's and the green half's.
    /// Luma, then each chroma plane, at the left and right samples, and the size.
    type Sampled = ([u8; 2], [u8; 2], [u8; 2], (u32, u32));

    fn sampled(codec: Codec, units: &[Vec<u8>]) -> Option<Sampled> {
        let mut decoder = Decoder::with_threads(codec, 1).ok()?;
        let mut last = None;
        for unit in units {
            if let Ok(Some(picture)) = decoder.decode(unit) {
                let planes = picture.i420();
                let (w, h) = (planes.width as usize, planes.height as usize);
                let at =
                    |plane: &[u8], stride: u32, x: usize, y: usize| plane[y * stride as usize + x];
                let [ys, us, vs] = planes.strides;
                let (left, right) = (w / 4, w * 3 / 4);
                last = Some((
                    [
                        at(planes.y, ys, left, h / 2),
                        at(planes.y, ys, right, h / 2),
                    ],
                    [
                        at(planes.u, us, left / 2, h / 4),
                        at(planes.u, us, right / 2, h / 4),
                    ],
                    [
                        at(planes.v, vs, left / 2, h / 4),
                        at(planes.v, vs, right / 2, h / 4),
                    ],
                    (planes.width, planes.height),
                ));
            }
        }
        last
    }

    fn near(value: u8, expected: u8) -> bool {
        value.abs_diff(expected) <= 8
    }

    /// Every codec a share could send, each opened with its own converter and
    /// session as the share's own encoders are; `kept` is the one sent.
    fn opened(width: u32, height: u32) -> Vec<(Codec, Convert, Encoder)> {
        [Codec::Av1, Codec::H265, Codec::H264]
            .into_iter()
            .filter_map(|codec| {
                let convert = Convert::open(width, height)
                    .map_err(|error| eprintln!("{codec:?}: no converter: {error:#}"))
                    .ok()?;
                let encoder = Encoder::open(codec, width, height, 60, 8_000, &convert)
                    .map_err(|error| eprintln!("{codec:?}: no session: {error:#}"))
                    .ok()?;
                Some((codec, convert, encoder))
            })
            .collect()
    }

    /// The pictures a capture cycles through are imported once each and kept,
    /// so a buffer drawn again since its last frame has to come out as it is
    /// now: the compositor redraws the same few buffers, and a kept import that
    /// showed the old picture would freeze the stream.
    #[test]
    fn a_kept_picture_shows_what_was_drawn_into_it_since() {
        let Ok(gbm) = Gbm::open() else {
            eprintln!("no NVIDIA gbm here");
            return;
        };
        let (width, height) = (640, 360);
        let (fd, stride, offset, modifier) = gbm.tiled(width, height).expect("a tiled buffer");
        let planes = [Plane { offset, stride }];
        let fds = [fd.as_raw_fd()];
        let picture = Descriptor {
            width,
            height,
            fourcc: XRGB8888,
            modifier,
            planes: &planes,
            colour: crate::dmabuf::Colour::SDR,
            fds: &fds,
        };
        let Ok(mut convert) = Convert::open(width, height) else {
            eprintln!("no NVIDIA card path here");
            return;
        };
        let Ok(mut encoder) = Encoder::open(Codec::H264, width, height, 60, 8_000, &convert) else {
            eprintln!("no H264 session here");
            return;
        };
        let shown = |convert: &mut Convert, encoder: &mut Encoder, frame: u64| {
            convert.convert(&picture, false).expect("a convert");
            let unit = encoder
                .encode(frame, true)
                .expect("an encode")
                .expect("a keyframe at once");
            sampled(Codec::H264, &[unit.data.to_vec()]).expect("a picture")
        };
        // Twice, so the second frame is the kept import's.
        shown(&mut convert, &mut encoder, 0);
        let (y, ..) = shown(&mut convert, &mut encoder, 1);
        assert!(near(y[0], 82) && near(y[1], 145), "red then green: {y:?}");
        // Discord's still is the picture the card made last, copied back: red on
        // the left, green on the right, at the stream's size.
        let (w, h) = convert.size();
        let (mut y_plane, mut u_plane, mut v_plane) = (
            vec![0u8; (w * h) as usize],
            vec![0u8; (w * h / 4) as usize],
            vec![0u8; (w * h / 4) as usize],
        );
        convert
            .still(
                (&mut y_plane, &mut u_plane, &mut v_plane),
                [w, w / 2, w / 2],
            )
            .expect("a still");
        let row = (h / 2 * w) as usize;
        let (left, right) = (
            y_plane[row + w as usize / 4],
            y_plane[row + w as usize * 3 / 4],
        );
        assert!(
            near(left, 82) && near(right, 145),
            "the still is {left} and {right}"
        );
        // A software-drawn window's preview is the same picture scaled on the card,
        // and only it comes back.
        let (pw, ph) = (320u32, 180u32);
        let (mut y_small, mut u_small, mut v_small) = (
            vec![0u8; (pw * ph) as usize],
            vec![0u8; (pw * ph / 4) as usize],
            vec![0u8; (pw * ph / 4) as usize],
        );
        convert
            .preview(
                (pw, ph),
                (&mut y_small, &mut u_small, &mut v_small),
                [pw, pw / 2, pw / 2],
            )
            .expect("a preview");
        let row = (ph / 2 * pw) as usize;
        let (left, right) = (
            y_small[row + pw as usize / 4],
            y_small[row + pw as usize * 3 / 4],
        );
        assert!(
            near(left, 82) && near(right, 145),
            "the preview is {left} and {right}"
        );
        let chroma = (ph / 4 * pw / 2) as usize + pw as usize / 8;
        assert!(
            near(u_small[chroma], 90) && near(v_small[chroma], 240),
            "the preview's red keeps its colour"
        );
        gbm.draw(&fd, (stride, offset, modifier), (width, height), GREEN, RED)
            .expect("drawn again");
        let (y, ..) = shown(&mut convert, &mut encoder, 2);
        assert!(
            near(y[0], 145) && near(y[1], 82),
            "the buffer was drawn green then red, and the stream shows {y:?}"
        );
    }

    /// Measures a tiled 3440x1440 screen's convert on the card, as a share pays
    /// it every frame: the import, the kernels and the wait for them.
    #[test]
    #[ignore = "measures; run with --ignored --nocapture"]
    fn measures_a_tiled_convert() {
        let Ok(gbm) = Gbm::open() else {
            eprintln!("no NVIDIA gbm here");
            return;
        };
        let (width, height) = (3440, 1440);
        let (fd, stride, offset, modifier) = gbm.tiled(width, height).expect("a tiled buffer");
        let planes = [Plane { offset, stride }];
        let fds = [fd.as_raw_fd()];
        let picture = Descriptor {
            width,
            height,
            fourcc: XRGB8888,
            modifier,
            planes: &planes,
            colour: crate::dmabuf::Colour::SDR,
            fds: &fds,
        };
        let mut convert = Convert::open(width, height).expect("a converter");
        // The first picture finds the door; the rest are what a stream pays.
        convert.convert(&picture, false).expect("a convert");
        let started = std::time::Instant::now();
        for _ in 0..200 {
            convert.convert(&picture, false).expect("a convert");
        }
        eprintln!(
            "a tiled 3440x1440 convert: {:?} on average",
            started.elapsed() / 200
        );
    }

    #[test]
    fn a_tiled_screen_is_encoded_on_the_card_in_every_codec_as_it_was_drawn() {
        let gbm = match Gbm::open() {
            Ok(gbm) => gbm,
            Err(why) => {
                eprintln!("no NVIDIA gbm here: {why}");
                return;
            }
        };
        // The screen of the share this was found on.
        let (width, height) = (3440, 1440);
        let (fd, stride, offset, modifier) = gbm.tiled(width, height).expect("a tiled buffer");
        eprintln!("the driver laid the buffer out as {modifier:#018x}, {stride} bytes a row");
        assert_ne!(
            modifier, 0,
            "a linear buffer tests nothing a compositor shares"
        );
        let planes = [Plane { offset, stride }];
        let fds = [fd.as_raw_fd()];
        // The same memory as a compositor names it with alpha and without: a
        // screencast is usually the second.
        for fourcc in [XRGB8888, ARGB8888] {
            let picture = Descriptor {
                width,
                height,
                fourcc,
                modifier,
                planes: &planes,
                colour: crate::dmabuf::Colour::SDR,
                fds: &fds,
            };
            // Sent at the screen's own size, and scaled down to a share's.
            for (sent_width, sent_height) in [(width, height), (1920, 804)] {
                for kept in [Codec::Av1, Codec::H265, Codec::H264] {
                    let mut all = opened(sent_width, sent_height);
                    if all.is_empty() {
                        eprintln!("no NVIDIA card path here");
                        return;
                    }
                    let Some(at) = all.iter().position(|(codec, ..)| *codec == kept) else {
                        eprintln!("{kept:?}: this card doesn't encode it");
                        continue;
                    };
                    let (codec, mut convert, mut encoder) = all.remove(at);
                    let what = format!("{codec:?} of {fourcc:#010x} at {sent_width}x{sent_height}");
                    let mut units = Vec::new();
                    for frame in 0..10u64 {
                        convert
                            .convert(&picture, false)
                            .unwrap_or_else(|error| panic!("{what}: {error:#}"));
                        if let Some(packet) = encoder.encode(frame, frame == 0).expect("an encode")
                        {
                            units.push(packet.data.to_vec());
                        }
                        // The sessions not sent go once the first picture is in,
                        // as a share's do: the one kept goes on.
                        all.clear();
                        // Discord's estimate moves the rate mid-stream, and the
                        // session takes it without a keyframe.
                        if frame == 4 {
                            encoder
                                .set_bitrate(4_000)
                                .unwrap_or_else(|error| panic!("{what}: a new rate: {error:#}"));
                        }
                    }
                    assert_eq!(encoder.copied(), 0, "{what}: the processor moved a picture");
                    assert!(!units.is_empty(), "{what}: the session gave nothing back");
                    let Some((y, u, v, size)) = sampled(codec, &units) else {
                        eprintln!(
                            "{what}: this build doesn't decode it; {} units",
                            units.len()
                        );
                        continue;
                    };
                    eprintln!("{what}: decoded {size:?}, Y {y:?} U {u:?} V {v:?}");
                    assert_eq!(size, (sent_width, sent_height), "{what}: the stream's size");
                    // BT.601, limited range: red is Y 82 U 90 V 240, green Y 145
                    // U 54 V 34. Blue on the left would be red and blue swapped.
                    assert!(
                        near(y[0], 82) && near(u[0], 90) && near(v[0], 240),
                        "{what}: the left half isn't red: Y {} U {} V {}",
                        y[0],
                        u[0],
                        v[0]
                    );
                    assert!(
                        near(y[1], 145) && near(u[1], 54) && near(v[1], 34),
                        "{what}: the right half isn't green: Y {} U {} V {}",
                        y[1],
                        u[1],
                        v[1]
                    );
                }
            }
        }
    }
}
