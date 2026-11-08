//! Decoding a watched Go Live stream: VP8 with libvpx and H264 with FFmpeg,
//! both inside libwebrtc, through native/decode.c; AV1 with libwebrtc's
//! dav1d, through native/av1.c; and H265 with the HEVC decoder of
//! vendor/ffmpeg-src's FFmpeg, through native/hevc.c, where it's built.
//!
//! A watched stream first asks the card's decoder, which leaves every picture
//! in a graphics card texture and scales it to the tile on the card through
//! native/d3d11dec.cc. Two decoders can be that one, and [`BACKENDS`] is the
//! order [`Hardware::new`] tries them in: FFmpeg's own Direct3D 11 decoder
//! (native/hwdec.c), which handles H264, HEVC, AV1 and VP9 on any vendor's
//! card, and Media Foundation's MFTs, kept beside it until a measured
//! comparison says which wins. The software decoders above are what reads a
//! stream the card's can't, so the picture a viewer sees is made either way.
//!
//! Which decoder a stream gets is this module's to say, not the native
//! shim's: [`Hardware::new`] opens the first backend that takes the codec,
//! [`Live`] falls back to the software decoder when none does, and the
//! keyframe wait and the software fallback after a card's failure are the same
//! whichever backend ran.

use std::{
    ffi::{c_int, c_uchar, c_uint, c_void},
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::{Result, bail};
use livekit::webrtc::native::yuv_helper;
use yapfast_discord::rtp::Codec;

#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

#[repr(C)]
struct RawPicture {
    width: c_int,
    height: c_int,
    y: *const c_uchar,
    u: *const c_uchar,
    v: *const c_uchar,
    y_stride: c_int,
    u_stride: c_int,
    v_stride: c_int,
}

unsafe extern "C" {
    fn yapfast_decoder_new(codec: c_int, threads: c_uint) -> *mut Raw;
    fn yapfast_decoder_decode(
        decoder: *mut Raw,
        data: *const c_uchar,
        size: usize,
        picture: *mut RawPicture,
    ) -> c_int;
    fn yapfast_decoder_free(decoder: *mut Raw);
    fn yapfast_av1_new(threads: c_uint) -> *mut Raw;
    fn yapfast_av1_decode(
        decoder: *mut Raw,
        data: *const c_uchar,
        size: usize,
        picture: *mut RawPicture,
    ) -> c_int;
    fn yapfast_av1_free(decoder: *mut Raw);
}

#[cfg(hevcdec)]
unsafe extern "C" {
    fn yapfast_hevc_new(threads: c_uint) -> *mut Raw;
    fn yapfast_hevc_decode(
        decoder: *mut Raw,
        data: *const c_uchar,
        size: usize,
        picture: *mut RawPicture,
    ) -> c_int;
    fn yapfast_hevc_free(decoder: *mut Raw);
}

type DecodeFn = unsafe extern "C" fn(*mut Raw, *const c_uchar, usize, *mut RawPicture) -> c_int;
type FreeFn = unsafe extern "C" fn(*mut Raw);

/// The codecs a viewer tells Discord it decodes, most wanted first: AV1
/// everywhere, H265 only where FFmpeg's HEVC decoder is built in, on Linux
/// and Windows.
pub const DECODES: &[Codec] = if cfg!(hevcdec) {
    &[Codec::Av1, Codec::H265, Codec::H264, Codec::Vp8]
} else {
    &[Codec::Av1, Codec::H264, Codec::Vp8]
};

/// The most threads one decoder asks for. A stream shown large comes off
/// four threads close to real time, while each of them is a reference picture
/// the codec keeps, so past four the work is heavier for little gain.
const MOST_THREADS: usize = 4;

/// One stream's share of a machine with `cores` that is decoding `streams` of
/// them at once: the cores divided among the streams, up to the four one
/// stream alone takes, and never less than the one thread a decoder can't do
/// without. A stream past the cores oversubscribes by that one thread rather
/// than waiting for another stream to finish.
pub fn share(cores: usize, streams: usize) -> usize {
    (cores / streams.max(1)).clamp(1, MOST_THREADS)
}

/// The decoder threads everything decoding in this process shares.
///
/// Each decoder asking the machine what it has, as if it were alone on it,
/// leaves the sum of them to chance: a watch that turns to another codec, a
/// second decoder beside the first, and two streams on a four-core machine
/// would each take four threads.
static THREADS: Budget = Budget::new();

/// The threads this process's decoders may use between them: what the Yapfast
/// that started this worker worked out for it, or what one decoder asks for
/// when a worker was started by hand.
static ALLOWED: AtomicUsize = AtomicUsize::new(MOST_THREADS);

/// Sets the threads this process's decoders may use between them, which the
/// Yapfast that started a stream worker works out from the machine's cores and
/// how many streams it's decoding at once. One thread is the floor, and four
/// the most one decoder asks for; anything else is ignored.
pub fn allow(threads: usize) {
    ALLOWED.store(threads.clamp(1, MOST_THREADS), Ordering::Relaxed);
}

/// The threads decoders in one process may use between them, handed out a
/// decoder at a time and given back when it goes.
pub struct Budget {
    claimed: AtomicUsize,
}

impl Budget {
    const fn new() -> Self {
        Self {
            claimed: AtomicUsize::new(0),
        }
    }

    /// Claims threads for one decoder out of `allowed`: up to four, as many
    /// as the budget has left, and at least one. A decoder on no threads
    /// decodes nothing, so a stream past the budget oversubscribes by the one
    /// thread it can't do without rather than waiting for another stream to
    /// finish.
    fn claim(&self, allowed: usize) -> Threads<'_> {
        let want = allowed.min(MOST_THREADS);
        let mut claimed = self.claimed.load(Ordering::Relaxed);
        loop {
            let threads = want.min(allowed.saturating_sub(claimed)).max(1);
            match self.claimed.compare_exchange_weak(
                claimed,
                claimed + threads,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    return Threads {
                        budget: self,
                        threads: threads as u32,
                    };
                }
                Err(moved) => claimed = moved,
            }
        }
    }

    /// How many threads this process's decoders hold now.
    pub fn claimed(&self) -> usize {
        self.claimed.load(Ordering::Relaxed)
    }
}

/// One decoder's share of a [`Budget`], given back when the decoder goes.
pub struct Threads<'a> {
    budget: &'a Budget,
    threads: u32,
}

impl Threads<'_> {
    pub fn count(&self) -> u32 {
        self.threads
    }
}

impl Drop for Threads<'_> {
    fn drop(&mut self) {
        self.budget
            .claimed
            .fetch_sub(self.threads as usize, Ordering::AcqRel);
    }
}

pub struct Decoder {
    raw: *mut Raw,
    codec: Codec,
    decode: DecodeFn,
    free: FreeFn,
    /// The share of this process's thread budget the decoder holds, if it took
    /// one from there: given back with the reference frames it keeps when the
    /// decoder goes.
    claimed: Option<Threads<'static>>,
}

// SAFETY: no library keeps thread-local state; one thread uses the decoder at a time.
unsafe impl Send for Decoder {}

/// A decoded picture, valid until the decoder's next frame.
pub struct Picture<'a> {
    pub width: u32,
    pub height: u32,
    y: &'a [u8],
    u: &'a [u8],
    v: &'a [u8],
    strides: [u32; 3],
}

impl Decoder {
    /// A decoder on up to four threads, out of the threads this process may
    /// decode on, for a stream shown large.
    pub fn new(codec: Codec) -> Result<Self> {
        let claimed = THREADS.claim(ALLOWED.load(Ordering::Relaxed));
        let mut decoder = Self::with_threads(codec, claimed.count())?;
        decoder.claimed = Some(claimed);
        Ok(decoder)
    }

    /// A decoder on `threads` threads; each keeps frames of its own. The
    /// caller has those threads to give, so none are claimed from the budget.
    pub fn with_threads(codec: Codec, threads: u32) -> Result<Self> {
        let threads = threads.max(1) as c_uint;
        // SAFETY: plain values; null means the codec couldn't start.
        let (raw, decode, free): (_, DecodeFn, FreeFn) = match codec {
            Codec::Vp8 => (
                unsafe { yapfast_decoder_new(0, threads) },
                yapfast_decoder_decode,
                yapfast_decoder_free,
            ),
            Codec::H264 => (
                unsafe { yapfast_decoder_new(1, threads) },
                yapfast_decoder_decode,
                yapfast_decoder_free,
            ),
            #[cfg(hevcdec)]
            Codec::H265 => (
                unsafe { yapfast_hevc_new(threads) },
                yapfast_hevc_decode,
                yapfast_hevc_free,
            ),
            // Not in DECODES, so Discord sends this viewer H264.
            #[cfg(not(hevcdec))]
            Codec::H265 => bail!("this build of Yapfast can't decode H265"),
            Codec::Av1 => (
                unsafe { yapfast_av1_new(threads) },
                yapfast_av1_decode,
                yapfast_av1_free,
            ),
        };
        if raw.is_null() {
            bail!("could not start the {codec:?} decoder");
        }
        Ok(Self {
            raw,
            codec,
            decode,
            free,
            claimed: None,
        })
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    /// Decodes a whole frame. None while the decoder holds it back.
    pub fn decode(&mut self, frame: &[u8]) -> Result<Option<Picture<'_>>> {
        let mut raw = RawPicture {
            width: 0,
            height: 0,
            y: std::ptr::null(),
            u: std::ptr::null(),
            v: std::ptr::null(),
            y_stride: 0,
            u_stride: 0,
            v_stride: 0,
        };
        // SAFETY: the frame outlives the call; the picture points into the
        // decoder, which `Picture` borrows.
        let decoded = unsafe { (self.decode)(self.raw, frame.as_ptr(), frame.len(), &mut raw) };
        match decoded {
            ..0 => bail!("could not decode a {:?} frame ({decoded})", self.codec),
            0 => Ok(None),
            _ => {
                if raw.width <= 0
                    || raw.height <= 0
                    || raw.y_stride <= 0
                    || raw.u_stride <= 0
                    || raw.v_stride <= 0
                {
                    bail!("the {:?} decoder gave an empty picture", self.codec);
                }
                let (width, height) = (raw.width as u32, raw.height as u32);
                let chroma = height.div_ceil(2) as usize;
                // SAFETY: the decoder's planes hold `height` rows of luma and
                // half as many of each chroma plane, at these strides.
                let plane = |data: *const c_uchar, stride: c_int, rows: usize| unsafe {
                    std::slice::from_raw_parts(data, stride as usize * rows)
                };
                Ok(Some(Picture {
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
                }))
            }
        }
    }
}

impl Picture<'_> {
    /// The picture's planes, as a feed's sink takes them.
    pub fn i420(&self) -> crate::picture::I420<'_> {
        crate::picture::I420 {
            width: self.width,
            height: self.height,
            y: self.y,
            u: self.u,
            v: self.v,
            strides: self.strides,
        }
    }

    /// Writes the picture as RGBA, `width * 4` bytes a row.
    pub fn to_rgba(&self, out: &mut [u8]) {
        // libyuv's ABGR is R, G, B, A in memory.
        yuv_helper::i420_to_abgr(
            self.y,
            self.strides[0],
            self.u,
            self.strides[1],
            self.v,
            self.strides[2],
            out,
            self.width * 4,
            self.width as i32,
            self.height as i32,
        );
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: raw came from the constructor `free` belongs with, and
        // is freed once.
        unsafe { (self.free)(self.raw) };
    }
}

/// The card's decoders, in the order [`Hardware::new`] tries them: FFmpeg's
/// own Direct3D 11 decoder first, then Media Foundation's MFTs.
///
/// FFmpeg's comes first because it is the one path that decodes all four of
/// H264, HEVC, AV1 and VP9 on any vendor's card, and because AV1 hardware
/// decode through it needs no Microsoft AV1 Video Extension, which Media
/// Foundation does and Windows 10 often lacks. Media Foundation stays beside
/// it until a measured comparison at 1080p and 4K, a time to first picture
/// and a driver reset say which is the winner; flipping the two is this line.
#[cfg(d3d11dec)]
const BACKENDS: [c_int; 2] = [1, 0];

/// The card's decoder for a watched stream, which leaves every picture
/// in a texture on the graphics card and scales it to the tile there, through
/// native/d3d11dec.cc.
#[cfg(d3d11dec)]
pub struct Hardware {
    raw: *mut Raw,
    size: (u32, u32),
}

#[cfg(d3d11dec)]
unsafe extern "C" {
    /// A decoder from `backend` — 0 Media Foundation, 1 FFmpeg's d3d11va — for
    /// `codec` (0 H264, 1 HEVC, 2 AV1, 3 VP9), which opens around the size
    /// given and settles the stream's own on its first frame; null when this
    /// build or this machine has no such decoder. `device` is the
    /// `ID3D11Device` the pictures should live on, which the caller keeps
    /// alive, or null for one made here.
    fn yapfast_hwdec_new(
        backend: c_int,
        codec: c_int,
        hint_width: c_int,
        hint_height: c_int,
        device: *mut c_void,
    ) -> *mut Raw;
    fn yapfast_hwdec_decode(
        decoder: *mut Raw,
        data: *const c_uchar,
        size: usize,
        keyframe: c_int,
    ) -> c_int;
    fn yapfast_hwdec_size(decoder: *const Raw, width: *mut c_int, height: *mut c_int) -> c_int;
    fn yapfast_hwdec_rgba(
        decoder: *mut Raw,
        rgba: *mut c_uchar,
        width: c_int,
        height: c_int,
    ) -> c_int;
    fn yapfast_hwdec_error(decoder: *const Raw) -> *const c_uchar;
    /// The newest picture's 8-bit planes, which only a test asks for: it
    /// converts the decoder's own samples both ways and compares the pictures.
    #[cfg(test)]
    fn yapfast_hwdec_nv12(
        decoder: *mut Raw,
        y: *mut c_uchar,
        y_stride: c_int,
        u: *mut c_uchar,
        u_stride: c_int,
        v: *mut c_uchar,
        v_stride: c_int,
    ) -> c_int;
    fn yapfast_hwdec_free(decoder: *mut Raw);
}

#[cfg(d3d11dec)]
impl Hardware {
    /// The card's decoder for a stream of `codec`: the first backend in
    /// [`BACKENDS`] that opens one, on any vendor's card. VP8 never has one —
    /// no hardware decoder takes it — and the software decoder reads the rest.
    pub fn new(codec: Codec) -> Result<Self> {
        Self::new_on(std::ptr::null_mut(), codec)
    }

    /// The card's decoder for a stream of `codec`, on `device` when the caller
    /// holds the `ID3D11Device` the encoder and the window already share, so a
    /// decoded picture is never copied between devices. Null asks for a device
    /// of the decoder's own.
    pub fn new_on(device: *mut c_void, codec: Codec) -> Result<Self> {
        let (which, what) = match codec {
            Codec::H264 => (0, "H264"),
            Codec::H265 => (1, "H265"),
            Codec::Av1 => (2, "AV1"),
            Codec::Vp8 => bail!("the graphics card has no VP8 decoder"),
        };
        Self::open(device, which, what)
    }

    /// The card's decoder for VP9, which the played-video path shows: FFmpeg's
    /// d3d11va decodes it on any vendor's card, and Media Foundation never
    /// does. `device` is as [`Self::new_on`] has it.
    pub fn vp9_on(device: *mut c_void) -> Result<Self> {
        Self::open(device, 3, "VP9")
    }

    /// The first backend that opens a decoder for `which` (0 H264, 1 HEVC,
    /// 2 AV1, 3 VP9), all of them decoding into textures on `device`. `what`
    /// names the codec in the error.
    fn open(device: *mut c_void, which: c_int, what: &str) -> Result<Self> {
        let mut last = None;
        for backend in BACKENDS {
            // SAFETY: plain values; null means this backend or this machine
            // has no decoder for the codec, and the reason is the shim's last
            // error.
            let raw = unsafe { yapfast_hwdec_new(backend, which, 0, 0, device) };
            if !raw.is_null() {
                return Ok(Self { raw, size: (0, 0) });
            }
            // SAFETY: the null decoder is what `error` reads the reason off.
            last = Some(unsafe { message(yapfast_hwdec_error(std::ptr::null())) });
        }
        let reason = last.unwrap_or_else(|| "the machine gave no reason".into());
        bail!("no {what} hardware decoder: {reason}")
    }

    /// The size of the newest picture, as the decoder made it.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Feeds one whole frame. True says a picture is ready to be shown, false
    /// that the decoder kept the frame for the pictures after it.
    pub fn decode(&mut self, frame: &[u8], keyframe: bool) -> Result<bool> {
        // SAFETY: the frame outlives the call, and one thread uses the decoder
        // at a time.
        let decoded = unsafe {
            yapfast_hwdec_decode(self.raw, frame.as_ptr(), frame.len(), keyframe as c_int)
        };
        if decoded < 0 {
            // SAFETY: the decoder is this one's, alive for as long as it is.
            bail!("the graphics card wouldn't decode a frame: {}", unsafe {
                message(yapfast_hwdec_error(self.raw))
            });
        }
        if decoded == 0 {
            return Ok(false);
        }
        let (mut width, mut height) = (0, 0);
        // SAFETY: both are plain integers for the call to fill in.
        let sized = unsafe { yapfast_hwdec_size(self.raw, &mut width, &mut height) };
        if sized == 0 || width <= 0 || height <= 0 {
            bail!("the graphics card gave a picture with no size");
        }
        self.size = (width as u32, height as u32);
        Ok(true)
    }

    /// The newest picture's planes as the decoder made them, the visible part
    /// of them, for a test that converts one picture both ways. The watch
    /// never asks: its pictures stay on the card.
    #[cfg(test)]
    pub fn nv12(&mut self) -> Result<Planes> {
        let (width, height) = self.size;
        let (luma_stride, chroma_stride) = (width as usize, width.div_ceil(2) as usize);
        let mut y = vec![0; luma_stride * height as usize];
        let mut u = vec![0; chroma_stride * height.div_ceil(2) as usize];
        let mut v = vec![0; u.len()];
        // SAFETY: each plane holds `height` (or half of them) rows of its
        // stride, and one thread uses the decoder at a time.
        let written = unsafe {
            yapfast_hwdec_nv12(
                self.raw,
                y.as_mut_ptr(),
                luma_stride as c_int,
                u.as_mut_ptr(),
                chroma_stride as c_int,
                v.as_mut_ptr(),
                chroma_stride as c_int,
            )
        };
        if written < 0 {
            // SAFETY: the decoder is this one's, alive for as long as it is.
            bail!("the picture's planes weren't readable: {}", unsafe {
                message(yapfast_hwdec_error(self.raw))
            });
        }
        Ok(Planes {
            width,
            height,
            y,
            u,
            v,
        })
    }

    /// Writes the picture, in the stream's colour and scaled to `width` by
    /// `height`, as RGBA of `width * 4` bytes a row.
    ///
    /// The tile written is the one the card was asked for on the call before:
    /// reading a tile back the moment its picture was converted is a wait for
    /// the card to finish decoding, which costs more processor time than the
    /// whole conversion, so a tile is copied back one frame later, when the
    /// card has long had it ready. A stream shows it a frame later than it
    /// decodes, which no viewer sees.
    pub fn rgba(&mut self, out: &mut [u8], width: u32, height: u32) -> Result<()> {
        assert!(out.len() >= width as usize * height as usize * 4);
        // SAFETY: `out` holds the picture at that size, and one thread uses
        // the decoder at a time.
        let written = unsafe {
            yapfast_hwdec_rgba(self.raw, out.as_mut_ptr(), width as c_int, height as c_int)
        };
        if written < 0 {
            // SAFETY: the decoder is this one's, alive for as long as it is.
            bail!("the tile wasn't converted: {}", unsafe {
                message(yapfast_hwdec_error(self.raw))
            });
        }
        Ok(())
    }
}

/// A message the shim wrote: a C string that lives until its next call.
///
/// # Safety
/// `text` must be null or a C string the shim keeps alive for the call that
/// asked for it, as `yapfast_hwdec_error`'s is.
#[cfg(d3d11dec)]
unsafe fn message(text: *const c_uchar) -> String {
    if text.is_null() {
        return "the machine gave no reason".into();
    }
    // SAFETY: as the caller promised; CStr stops at the shim's terminator.
    unsafe { std::ffi::CStr::from_ptr(text.cast()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(d3d11dec)]
impl Drop for Hardware {
    fn drop(&mut self) {
        // SAFETY: raw came from the constructor `free` belongs with, and is
        // freed once.
        unsafe { yapfast_hwdec_free(self.raw) };
    }
}

// SAFETY: no library keeps thread-local state; one thread uses the decoder at a
// time.
#[cfg(d3d11dec)]
unsafe impl Send for Hardware {}

/// One picture's 8-bit planes, the visible part of them: a test's way of
/// converting a decoder's own samples the way the software path does.
#[cfg(all(d3d11dec, test))]
pub struct Planes {
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

/// A picture decoded for a watched stream, and where it lies: in the software
/// decoder's planes, which the processor reads and converts, or in the texture
/// the machine's decoder left it in, which the graphics card scales into the
/// tile.
pub enum Frame<'a> {
    Software(Picture<'a>),
    #[cfg(d3d11dec)]
    Hardware(&'a mut Hardware),
}

impl Frame<'_> {
    /// The picture's size, as the decoder made it.
    pub fn size(&self) -> (u32, u32) {
        match self {
            Frame::Software(picture) => (picture.width, picture.height),
            #[cfg(d3d11dec)]
            Frame::Hardware(hardware) => hardware.size(),
        }
    }
}

/// A watched stream's decoder: the machine's own while it can take the stream,
/// and the software decoder where it can't — VP8, a codec this machine has no
/// decoder for, a 10-bit picture the card's pass has no curve for, or a
/// session the card refused. Which one a stream gets is the machine's to say,
/// and the stream is shown the same way either way.
pub struct Live {
    codec: Codec,
    #[cfg(d3d11dec)]
    hardware: Option<Hardware>,
    software: Option<Decoder>,
}

impl Live {
    /// A decoder for a newly watched stream, on the graphics card where this
    /// machine has one that takes the codec and this build's fallback where it
    /// hasn't.
    pub fn new(codec: Codec) -> Result<Self> {
        Self::new_on(std::ptr::null_mut(), codec)
    }

    /// A decoder for a newly watched stream, on `device` when the caller holds
    /// the `ID3D11Device` the encoder and the window already share: the same
    /// pictures the watch shows, decoded where they are shown. The first
    /// backend in [`BACKENDS`] that takes the codec is the one that reads the
    /// stream, and the software decoder is the fallback when none does.
    pub fn new_on(device: *mut c_void, codec: Codec) -> Result<Self> {
        // Only the card's path reads the device; without one this is the
        // software decoder alone.
        let _ = device;
        #[cfg(d3d11dec)]
        match Hardware::new_on(device, codec) {
            Ok(hardware) => {
                return Ok(Self {
                    codec,
                    hardware: Some(hardware),
                    software: None,
                });
            }
            Err(error) => tracing::info!(
                error = format!("{error:#}"),
                codec = ?codec,
                "watching this stream with the software decoder"
            ),
        }
        Self::software(codec)
    }

    /// The software decoder alone, where the machine's has given up on a
    /// stream or was never tried.
    pub fn software(codec: Codec) -> Result<Self> {
        Ok(Self {
            codec,
            #[cfg(d3d11dec)]
            hardware: None,
            software: Some(Decoder::new(codec)?),
        })
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    /// Whether the machine's decoder is the one reading this stream, rather
    /// than the software one.
    #[cfg(d3d11dec)]
    pub fn on_the_card(&self) -> bool {
        self.hardware.is_some()
    }

    /// Whether the machine's decoder is the one reading this stream, rather
    /// than the software one.
    #[cfg(not(d3d11dec))]
    pub fn on_the_card(&self) -> bool {
        false
    }

    /// Decodes a whole frame: the picture where it lies, or none while the
    /// decoder holds it back.
    ///
    /// A machine decoder that fails stops being used, and the error it leaves
    /// says so: the caller waits for a keyframe, and the software decoder takes
    /// the stream from there.
    // `keyframe` is what a machine decoder is told; software doesn't need it.
    #[cfg_attr(not(d3d11dec), allow(unused_variables))]
    pub fn decode(&mut self, frame: &[u8], keyframe: bool) -> Result<Option<Frame<'_>>> {
        #[cfg(d3d11dec)]
        if let Some(mut hardware) = self.hardware.take() {
            match hardware.decode(frame, keyframe) {
                Ok(true) => {
                    self.hardware = Some(hardware);
                    return Ok(self.hardware.as_mut().map(Frame::Hardware));
                }
                Ok(false) => {
                    self.hardware = Some(hardware);
                    return Ok(None);
                }
                Err(error) => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        codec = ?self.codec,
                        "the graphics card gave up on a stream"
                    );
                    self.hardware = None;
                    return Err(error);
                }
            }
        }
        if self.software.is_none() {
            self.software = Some(Decoder::new(self.codec)?);
        }
        let decoder = self
            .software
            .as_mut()
            .expect("a software decoder was just made");
        Ok(decoder.decode(frame)?.map(Frame::Software))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../testing/fixtures")
                .join(name),
        )
        .unwrap()
    }

    /// A stream's share of the machine's decoder threads, as the Yapfast that
    /// starts a worker works it out: a stream alone takes the four one decoder
    /// asks for however many cores there are, four streams on eight cores take
    /// two each, and a stream past the cores still takes the one thread a
    /// decoder can't do without.
    #[test]
    fn a_streams_share_of_the_decoder_threads() {
        assert_eq!(share(4, 1), 4, "a stream alone takes four threads");
        assert_eq!(share(32, 1), 4, "cores to spare don't buy one stream more");
        assert_eq!(share(8, 4), 2, "four streams on eight cores take two each");
        assert_eq!(share(4, 4), 1, "four streams on four cores take one each");
        assert_eq!(share(4, 8), 1, "past the cores, a stream still takes one");
        assert_eq!(share(1, 1), 1, "a one-core machine has one to give");
        assert_eq!(share(2, 2), 1, "a small machine gives what it has");
    }

    /// Watches that decode at once share one budget of threads: the first on
    /// a four-core machine gets the four threads a lone stream always got, the
    /// next gets the one a decoder can't do without rather than four more, and
    /// what one gives back another may claim.
    #[test]
    fn decoders_share_one_budget_of_threads() {
        let budget = Budget::new();
        let first = budget.claim(4);
        assert_eq!(
            first.count(),
            4,
            "a stream on a four-core machine gets four"
        );
        let second = budget.claim(4);
        assert_eq!(second.count(), 1, "a stream past the budget gets one");
        assert_eq!(budget.claimed(), 5, "five threads on four cores, not eight");
        // Cores to spare don't buy one stream more than four of them.
        assert_eq!(Budget::new().claim(32).count(), 4);
        // A small machine gives what it has, not four.
        assert_eq!(Budget::new().claim(2).count(), 2);
        // A decoder that goes gives its threads back, and the next claims as
        // many as the budget has left then.
        drop(first);
        assert_eq!(budget.claimed(), 1);
        let third = budget.claim(4);
        assert_eq!(third.count(), 3);
        assert_eq!(budget.claimed(), 4);
        drop((second, third));
        assert_eq!(budget.claimed(), 0, "every thread went back");
    }

    fn check(codec: Codec, frame: &[u8]) {
        let mut decoder = Decoder::new(codec).unwrap();
        let picture = decoder
            .decode(frame)
            .unwrap()
            .expect("a picture from a keyframe");
        assert_eq!((picture.width, picture.height), (320, 180), "{codec:?}");
        let mut rgba = vec![0; 320 * 180 * 4];
        picture.to_rgba(&mut rgba);
        assert!(rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
        let colours: std::collections::HashSet<_> =
            rgba.chunks_exact(4).map(|pixel| pixel[0] / 32).collect();
        assert!(
            colours.len() > 3,
            "{codec:?} decoded to a picture, not a flat colour"
        );
        assert!(decoder.decode(b"not video").is_err() || codec != Codec::Vp8);
    }

    /// The first access unit of an Annex B recording: up to its second
    /// delimiter, whose NAL header starts with `delimiter`.
    fn first_unit(file: &[u8], delimiter: u8) -> &[u8] {
        let second = (1..file.len() - 4)
            .find(|&at| file[at..at + 5] == [0, 0, 0, 1, delimiter])
            .unwrap();
        &file[..second]
    }

    /// The first temporal unit of an AV1 recording: its OBUs, each giving
    /// its size, up to the second temporal delimiter (type 2).
    fn first_temporal_unit(file: &[u8]) -> Vec<u8> {
        let mut at = 0;
        loop {
            let (mut size, mut used) = (0, 0);
            while file[at + 1 + used] & 0x80 != 0 {
                size |= usize::from(file[at + 1 + used] & 0x7f) << (7 * used);
                used += 1;
            }
            size |= usize::from(file[at + 1 + used]) << (7 * used);
            let next = at + 2 + used + size;
            if at > 0 && (file[at] >> 3) & 0x0f == 2 {
                return file[..at].to_vec();
            }
            at = next;
        }
    }

    #[test]
    fn decodes_the_recorded_streams() {
        let vp8 = fixture("stream-vp8.ivf");
        let header = usize::from(u16::from_le_bytes([vp8[6], vp8[7]]));
        let size = u32::from_le_bytes(vp8[header..header + 4].try_into().unwrap()) as usize;
        check(Codec::Vp8, &vp8[header + 12..header + 12 + size]);
        check(Codec::H264, first_unit(&fixture("stream-h264.h264"), 9));
        // AV1 as it comes, and as DAVE leaves it, the last OBU without its
        // size.
        let unit = first_temporal_unit(&fixture("stream-av1.obu"));
        check(Codec::Av1, &unit);
        let elements = yapfast_discord::rtp::obu_elements(&unit).unwrap();
        let last = elements.last().unwrap();
        // Its header, then a size of one LEB128 byte, or two from 128 on.
        let sized_last = 1 + last.len() + usize::from(last.len() > 128);
        check(
            Codec::Av1,
            &[&unit[..unit.len() - sized_last], last.as_slice()].concat(),
        );
        // H265's delimiter is NAL type 35.
        if cfg!(hevcdec) {
            check(
                Codec::H265,
                first_unit(&fixture("stream-h265.h265"), 35 << 1),
            );
        }
    }

    /// A stream the card can decode is watched on the card: the decoder the
    /// watch asks for hands back a picture that never came to the processor,
    /// which is the path a viewer's frame takes when nothing goes wrong.
    #[cfg(d3d11dec)]
    #[test]
    fn a_stream_the_card_can_decode_is_watched_on_the_card() {
        if Hardware::new(Codec::H264).is_err() {
            eprintln!("this machine's card has no H264 decoder");
            return;
        }
        let stream = fixture("stream-h264.h264");
        let unit = first_unit(&stream, 9);
        let mut live = Live::new(Codec::H264).unwrap();
        assert!(
            live.on_the_card(),
            "a card decoder was found and should be used"
        );
        match live.decode(unit, true).unwrap() {
            Some(Frame::Hardware(hardware)) => {
                let (width, height) = hardware.size();
                assert_eq!((width, height), (320, 180));
                let mut pixels = vec![0; width as usize * height as usize * 4];
                hardware.rgba(&mut pixels, width, height).unwrap();
                let lit = pixels.chunks_exact(4).filter(|pixel| pixel[..3] != [0; 3]);
                assert!(lit.count() > 1000, "the picture has a picture in it");
            }
            _ => panic!("the card decoded a keyframe and gave no picture"),
        }
    }

    /// The process's own kernel and user time, in milliseconds, as the
    /// measurement method counts it: Windows charges it in 15.6 ms ticks, so a
    /// measurement has to run long enough for the ticks to add up.
    #[cfg(d3d11dec)]
    fn cpu_millis() -> f64 {
        #[repr(C)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn GetProcessTimes(
                process: *mut std::ffi::c_void,
                created: *mut FileTime,
                exited: *mut FileTime,
                kernel: *mut FileTime,
                user: *mut FileTime,
            ) -> i32;
        }
        let blank = || FileTime { low: 0, high: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (blank(), blank(), blank(), blank());
        // SAFETY: plain out-parameters for the call, and the process is this one.
        unsafe {
            GetProcessTimes(
                GetCurrentProcess(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        };
        let ticks = |time: FileTime| f64::from(time.high) * 4_294_967_296.0 + f64::from(time.low);
        (ticks(kernel) + ticks(user)) / 10_000.0
    }

    /// This process's working set, in megabytes.
    #[cfg(d3d11dec)]
    fn working_set_megabytes() -> f64 {
        #[repr(C)]
        struct Counters {
            size: u32,
            page_faults: u32,
            peak_working_set: usize,
            working_set: usize,
            quota_peak_paged: usize,
            quota_paged: usize,
            quota_peak_unpaged: usize,
            quota_unpaged: usize,
            pagefile: usize,
            peak_pagefile: usize,
        }
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn K32GetProcessMemoryInfo(
                process: *mut std::ffi::c_void,
                counters: *mut Counters,
                size: u32,
            ) -> i32;
        }
        let mut counters = Counters {
            size: size_of::<Counters>() as u32,
            page_faults: 0,
            peak_working_set: 0,
            working_set: 0,
            quota_peak_paged: 0,
            quota_paged: 0,
            quota_peak_unpaged: 0,
            quota_unpaged: 0,
            pagefile: 0,
            peak_pagefile: 0,
        };
        // SAFETY: the counters are this call's own out-parameter, sized as the
        // call expects, for this process.
        unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.size) };
        counters.working_set as f64 / (1024.0 * 1024.0)
    }

    /// What a 1080p stream costs the processor a frame, before and after: the
    /// same recording decoded and shown at a 480×270 tile through both paths —
    /// software decode, then libyuv's conversion and scale on the processor;
    /// the card's decode, then its video processor scaling the picture into
    /// the tile, of which only the tile comes back.
    ///
    /// The recording is named by `YAPFAST_MEASURE_STREAM`, an Annex B H264
    /// file, because the repository's fixtures are 320×180 and a tile's worth
    /// of that measures nothing. CPU is the process's own, and four decoders
    /// opened at the end say what a stream's session costs the process on top.
    #[cfg(d3d11dec)]
    #[test]
    #[ignore = "measures a 1080p stream's cost; set YAPFAST_MEASURE_STREAM to an Annex B H264 file"]
    fn measures_a_1080p_stream_at_a_tile() {
        use std::time::Instant;
        let path =
            std::env::var("YAPFAST_MEASURE_STREAM").expect("an Annex B H264 file to measure");
        let file = std::fs::read(&path).expect("the stream to measure");
        // Every access unit: a new one starts at each parameter set and each
        // slice, so that a keyframe's SPS and PPS are read with its IDR, the
        // way a stream's first frame carries them.
        let mut starts = vec![];
        let mut slice_before = false;
        let mut at = 0;
        while let Some(found) = file[at..].windows(3).position(|window| window == [0, 0, 1]) {
            let code = at + found;
            let kind = file.get(code + 3).map(|byte| byte & 0x1f).unwrap_or(0);
            let slice = matches!(kind, 1 | 5);
            // A parameter set starts the unit a keyframe's picture comes with;
            // a slice starts one of its own once a slice came before it.
            if kind == 7 || (slice && slice_before) {
                starts.push(code);
            }
            slice_before = slice;
            at = code + 3;
        }
        let units: Vec<(usize, usize)> = starts
            .iter()
            .zip(starts.iter().skip(1).map(Some).chain([None]))
            .map(|(from, to)| (*from, to.copied().unwrap_or(file.len())))
            .collect();
        let passes = 2;
        let mut pixels = vec![0; 480 * 270 * 4];
        let mut scratch = vec![];

        let mut decoder = Decoder::new(Codec::H264).unwrap();
        let mut frames = 0u32;
        let (began, cpu) = (Instant::now(), cpu_millis());
        for _ in 0..passes {
            for &(from, to) in &units {
                if let Some(picture) = decoder.decode(&file[from..to]).unwrap() {
                    picture.i420().to_rgba(&mut scratch, &mut pixels, 480, 270);
                    frames += 1;
                }
            }
        }
        let software = (cpu_millis() - cpu) / f64::from(frames);
        eprintln!(
            "software: {software:.3} ms of CPU a frame ({frames} frames, {:.0} ms wall)",
            began.elapsed().as_secs_f64() * 1e3
        );

        // The card's decoder alone, and the same decode with the video
        // processor's pass and the tile's way back, so that what the pass
        // costs is the difference rather than a guess.
        let mut hardware = Hardware::new(Codec::H264).unwrap();
        let mut frames = 0u32;
        let (began, cpu) = (Instant::now(), cpu_millis());
        for _ in 0..passes {
            for &(from, to) in &units {
                if hardware.decode(&file[from..to], true).unwrap() {
                    frames += 1;
                }
            }
        }
        let decoding = (cpu_millis() - cpu) / f64::from(frames);
        eprintln!(
            "the card, decoding only: {decoding:.3} ms of CPU a frame ({frames} frames, {:.0} ms wall)",
            began.elapsed().as_secs_f64() * 1e3
        );

        let mut hardware = Hardware::new(Codec::H264).unwrap();
        let mut frames = 0u32;
        let (began, cpu) = (Instant::now(), cpu_millis());
        for _ in 0..passes {
            for &(from, to) in &units {
                if hardware.decode(&file[from..to], true).unwrap() {
                    hardware.rgba(&mut pixels, 480, 270).unwrap();
                    frames += 1;
                }
            }
        }
        let on_the_card = (cpu_millis() - cpu) / f64::from(frames);
        eprintln!(
            "the card, shown at 480x270: {on_the_card:.3} ms of CPU a frame ({frames} frames, {:.0} ms wall)",
            began.elapsed().as_secs_f64() * 1e3
        );

        // The pass's own cost with the tile it has to bring back cut to one
        // pixel, and with only every fourth picture brought back at all: what
        // is left is the wait for the card's pipeline, not the copy.
        let mut hardware = Hardware::new(Codec::H264).unwrap();
        let mut frames = 0u32;
        let mut tiny = [0u8; 4];
        let (began, cpu) = (Instant::now(), cpu_millis());
        for _ in 0..passes {
            for &(from, to) in &units {
                if hardware.decode(&file[from..to], true).unwrap() {
                    hardware.rgba(&mut tiny, 1, 1).unwrap();
                    frames += 1;
                }
            }
        }
        let one_pixel = (cpu_millis() - cpu) / f64::from(frames);
        eprintln!(
            "the card, shown at one pixel: {one_pixel:.3} ms of CPU a frame ({frames} frames, {:.0} ms wall)",
            began.elapsed().as_secs_f64() * 1e3
        );

        let before = working_set_megabytes();
        let mut kept = vec![];
        for _ in 0..4 {
            let mut decoder = Hardware::new(Codec::H264).unwrap();
            // A picture takes the first frames of a stream to come out of the
            // card's low-latency decoder.
            let mut ready = false;
            for (index, &(from, to)) in units.iter().take(8).enumerate() {
                ready |= decoder.decode(&file[from..to], index == 0).unwrap();
            }
            assert!(ready, "a keyframe and seven frames made no picture");
            decoder.rgba(&mut pixels, 480, 270).unwrap();
            kept.push(decoder);
        }
        let after = working_set_megabytes();
        eprintln!(
            "four 1080p decoders hold {:.0} MB of working set (the process had {before:.0} MB)",
            after - before
        );
    }

    /// The machine's decoder's picture in the same colours as the software
    /// path's: one H264, HEVC and AV1 keyframe from `testing/fixtures/` is
    /// decoded on the card, its own 8-bit planes are converted twice — by
    /// libyuv on the processor, as `I420::to_rgba` does for the watch, and by
    /// the video processor on the card, matrix, range, scale and channel order
    /// in its one pass — and the two pictures are compared.
    ///
    /// The fixtures carry no colour metadata, so both read them as BT.601
    /// limited, which is what the software path has always assumed for a
    /// stream that says nothing. What is left between them is the video
    /// processor's own 4:2:0 upsampling: it interpolates chroma with H.264's
    /// siting where libyuv repeats it, so a chroma edge can be tens of codes
    /// apart on a test card full of them, while every flat area matches. The
    /// luma carries the range — a limited/full mix-up moves it by ten codes —
    /// and a matrix or channel order a shader got wrong moves all three
    /// channels, so the mean holds those to five codes a sample, luma to 1.5
    /// with 95% of it within four, half the samples to one code, and the same
    /// comparison with red and blue exchanged is required to be far worse:
    /// the fixtures are saturated enough (a mean of 162 codes between red and
    /// blue) that a swapped channel order could not pass.
    #[cfg(d3d11dec)]
    #[test]
    fn the_card_shows_a_stream_in_the_same_colours_as_software() {
        let units = [
            (
                Codec::H264,
                first_unit(&fixture("stream-h264.h264"), 9).to_vec(),
            ),
            (
                Codec::H265,
                first_unit(&fixture("stream-h265.h265"), 35 << 1).to_vec(),
            ),
            (Codec::Av1, first_temporal_unit(&fixture("stream-av1.obu"))),
        ];
        let mut compared = 0;
        for (codec, unit) in units {
            let mut hardware = match Hardware::new(codec) {
                Ok(hardware) => hardware,
                Err(error) => {
                    eprintln!("{codec:?} isn't decoded on this machine's card: {error:#}");
                    continue;
                }
            };
            let mut software = Decoder::new(codec).unwrap();
            let picture = software
                .decode(&unit)
                .unwrap()
                .expect("a picture from the keyframe");
            // A keyframe's picture is there once the decoder has read the unit;
            // the same unit read again is the same picture, so a decoder that
            // held the first one gives it here.
            let ready =
                hardware.decode(&unit, true).unwrap() || hardware.decode(&unit, true).unwrap();
            assert!(ready, "{codec:?}: the card made no picture");
            assert_eq!(
                hardware.size(),
                (picture.width, picture.height),
                "{codec:?}: the card's picture is the stream's size, without the padding its surface carries"
            );
            let planes = hardware.nv12().unwrap();
            let (width, height) = (planes.width, planes.height);
            let planes = crate::picture::I420 {
                width,
                height,
                y: &planes.y,
                u: &planes.u,
                v: &planes.v,
                strides: [width, width.div_ceil(2), width.div_ceil(2)],
            };
            let mut expected = vec![0; width as usize * height as usize * 4];
            planes.to_rgba(&mut vec![], &mut expected, width, height);
            let mut shown = vec![0; expected.len()];
            hardware.rgba(&mut shown, width, height).unwrap();
            let mut differences = Vec::with_capacity(expected.len() / 4 * 3);
            let mut luma = Vec::with_capacity(expected.len() / 4);
            let mut exchanged = 0u64;
            let light = |pixel: &[u8]| {
                (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                    / 1000
            };
            for (want, got) in expected.chunks_exact(4).zip(shown.chunks_exact(4)) {
                for channel in 0..3 {
                    differences.push(u16::from(want[channel].abs_diff(got[channel])));
                }
                luma.push(light(want).abs_diff(light(got)));
                exchanged +=
                    u64::from(want[0].abs_diff(got[2])) + u64::from(want[2].abs_diff(got[0]));
            }
            differences.sort_unstable();
            luma.sort_unstable();
            let mean = differences.iter().map(|&d| u64::from(d)).sum::<u64>() as f64
                / differences.len() as f64;
            let median = differences[differences.len() / 2];
            let luma_mean =
                luma.iter().map(|&d| u64::from(d)).sum::<u64>() as f64 / luma.len() as f64;
            let luma_95 = luma[luma.len() * 95 / 100];
            let exchanged = exchanged as f64 / (luma.len() as f64 / 3.0 * 2.0);
            eprintln!(
                "{codec:?}: {mean:.2} codes a sample on average, {median} at the median,                  luma {luma_mean:.2} on average and {luma_95} at the 95th, {exchanged:.1} with red and blue exchanged"
            );
            assert!(mean <= 5.0, "{codec:?}: {mean:.2} codes a sample");
            assert!(median <= 1, "{codec:?}: {median} codes at the median");
            assert!(luma_mean <= 1.5, "{codec:?}: {luma_mean:.2} codes of luma");
            assert!(
                luma_95 <= 4,
                "{codec:?}: {luma_95} codes of luma at the 95th"
            );
            assert!(
                exchanged > mean * 4.0,
                "{codec:?}: exchanging red and blue leaves {exchanged:.1} codes, so the comparison can't tell a channel order"
            );
            compared += 1;
        }
        assert!(
            compared > 0,
            "no codec was compared: this machine's card decodes none of them"
        );
    }
}
