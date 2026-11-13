//! `decode.rs`'s decoding.

use super::*;

#[repr(C)]
pub(crate) struct Raw {
    pub(crate) _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct RawPicture {
    pub(crate) width: c_int,
    pub(crate) height: c_int,
    pub(crate) y: *const c_uchar,
    pub(crate) u: *const c_uchar,
    pub(crate) v: *const c_uchar,
    pub(crate) y_stride: c_int,
    pub(crate) u_stride: c_int,
    pub(crate) v_stride: c_int,
}

pub(crate) type DecodeFn =
    unsafe extern "C" fn(*mut Raw, *const c_uchar, usize, *mut RawPicture) -> c_int;
pub(crate) type FreeFn = unsafe extern "C" fn(*mut Raw);

/// The codecs a viewer tells Discord it decodes, most wanted first: AV1
/// everywhere, H265 only where FFmpeg's HEVC decoder is built in, on Linux
/// and Windows.
pub const DECODES: &[Codec] = if cfg!(hevcdec) {
    &[Codec::Av1, Codec::H265, Codec::H264, Codec::Vp8]
} else {
    &[Codec::Av1, Codec::H264, Codec::Vp8]
};

pub struct Decoder {
    pub(crate) raw: *mut Raw,
    pub(crate) codec: Codec,
    pub(crate) decode: DecodeFn,
    pub(crate) free: FreeFn,
    /// The share of this process's thread budget the decoder holds, if it took
    /// one from there: given back with the reference frames it keeps when the
    /// decoder goes.
    pub(crate) claimed: Option<Threads<'static>>,
}

// SAFETY: no library keeps thread-local state; one thread uses the decoder at a time.
unsafe impl Send for Decoder {}

/// A decoded picture, valid until the decoder's next frame.
pub struct Picture<'a> {
    pub width: u32,
    pub height: u32,
    pub(crate) y: &'a [u8],
    pub(crate) u: &'a [u8],
    pub(crate) v: &'a [u8],
    pub(crate) strides: [u32; 3],
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

/// The card's decoder for a watched stream — FFmpeg's own Direct3D 11
/// decoder, the one card decoder this build has — which leaves every picture
/// in a texture on the graphics card and scales it to the tile there, through
/// native/d3d11dec.cc.
#[cfg(d3d11dec)]
pub struct Hardware {
    pub(crate) raw: *mut Raw,
    pub(crate) size: (u32, u32),
}

#[cfg(d3d11dec)]
impl Hardware {
    /// The card's decoder for a stream of `codec`, on any vendor's card. VP8
    /// never has one — no hardware decoder takes it — and the software decoder
    /// reads the rest.
    pub fn new(codec: Codec) -> Result<Self> {
        // SAFETY: a null device asks for one of the decoder's own, which it
        // makes and keeps.
        unsafe { Self::open(std::ptr::null_mut(), codec) }
    }

    /// The card's decoder for a stream of `codec`, on `device` when the caller
    /// holds the `ID3D11Device` the encoder and the window already share, so a
    /// decoded picture is never copied between devices. Null asks for a device
    /// of the decoder's own.
    ///
    /// # Safety
    /// `device`, when it isn't null, must be an `ID3D11Device` that outlives
    /// every decoder opened from it.
    pub unsafe fn new_on(device: *mut c_void, codec: Codec) -> Result<Self> {
        // SAFETY: as the caller promised.
        unsafe { Self::open(device, codec) }
    }

    /// The card's decoder for `codec` (0 H264, 1 HEVC, 2 AV1), decoding into a
    /// texture on `device`. Only a stream the card takes no codec for — or one
    /// its driver refuses, or one whose pictures the tile pass can't show —
    /// comes back as the reason it couldn't, and it is the processor's then.
    ///
    /// # Safety
    /// `device`, when it isn't null, must be an `ID3D11Device` that outlives
    /// this decoder.
    pub(crate) unsafe fn open(device: *mut c_void, codec: Codec) -> Result<Self> {
        let (which, what) = match codec {
            Codec::H264 => (0, "H264"),
            Codec::H265 => (1, "H265"),
            Codec::Av1 => (2, "AV1"),
            Codec::Vp8 => bail!("the graphics card has no VP8 decoder"),
        };
        // SAFETY: plain values; null means this build or this machine has no
        // decoder for the codec, and the reason is the shim's last error.
        let raw = unsafe { yapfast_hwdec_new(which, 0, 0, device) };
        if !raw.is_null() {
            return Ok(Self { raw, size: (0, 0) });
        }
        // SAFETY: the null decoder is what `error` reads the reason off.
        let reason = unsafe { message(yapfast_hwdec_error(std::ptr::null())) };
        // A decoder that failed before it could say why is no reason to quote
        // an empty one.
        if reason.is_empty() {
            bail!("no {what} hardware decoder: the machine gave no reason")
        }
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
/// asked for it, as `yapfast_hwdec_error`'s and `yapfast_vadec_error`'s are.
#[cfg(any(d3d11dec, vaapidec, test))]
pub(crate) unsafe fn message(text: *const c_uchar) -> String {
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

// SAFETY: the shim keeps one thread-local string — the reason a decoder that
// couldn't be made wasn't, which `Hardware::open` reads and the shim writes on
// the thread that calls it — and a decoder's own state is one thread's at a
// time, since `Frame::Hardware` holds it mutably. Moving a decoder between
// threads is sound; using one from two at once is not.
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
/// decoder's planes, which the processor reads and converts, or on the
/// graphics card — in the texture the Direct3D 11 decoder left it in, which the
/// card scales into the tile, or, on Linux, in the DMA-BUF the VA-API decoder
/// exported it as, which the tile pass imports.
pub enum Frame<'a> {
    Software(Picture<'a>),
    #[cfg(any(d3d11dec, vaapidec, nvdec, mfxdec))]
    Hardware(&'a mut Hardware),
}

impl Frame<'_> {
    /// The picture's size, as the decoder made it.
    pub fn size(&self) -> (u32, u32) {
        match self {
            Frame::Software(picture) => (picture.width, picture.height),
            #[cfg(any(d3d11dec, vaapidec, nvdec, mfxdec))]
            Frame::Hardware(hardware) => hardware.size(),
        }
    }
}

/// A watched stream's decoder: the machine's own while it can take the stream,
/// and the software decoder where it can't — VP8, a codec this machine has no
/// decoder for, a 10-bit picture whose colour signal the card's pass has no
/// curve for, or a session the card refused. Which one a stream gets is the
/// machine's to say, and the stream is shown the same way either way.
pub struct Live {
    pub(crate) codec: Codec,
    #[cfg(any(d3d11dec, vaapidec, nvdec, mfxdec))]
    pub(crate) hardware: Option<Hardware>,
    pub(crate) software: Option<Decoder>,
}

impl Live {
    /// A decoder for a newly watched stream, on the graphics card where this
    /// machine has one that takes the codec and this build's fallback where it
    /// hasn't.
    pub fn new(codec: Codec) -> Result<Self> {
        Self::on_codec(std::ptr::null_mut(), codec)
    }

    /// A decoder for a newly watched stream, on `device` when the caller holds
    /// the `ID3D11Device` the encoder and the window already share: the same
    /// pictures the watch shows, decoded where they are shown. The card's
    /// decoder is what reads the stream when it takes the codec, and the
    /// software decoder is the fallback when it doesn't.
    ///
    /// # Safety
    /// `device`, when it isn't null, must be an `ID3D11Device` that outlives
    /// this decoder.
    pub unsafe fn new_on(device: *mut c_void, codec: Codec) -> Result<Self> {
        Self::on_codec(device, codec)
    }

    /// [`Self::new_on`], without the safety the caller carries from here on.
    pub(crate) fn on_codec(device: *mut c_void, codec: Codec) -> Result<Self> {
        #[cfg(not(any(d3d11dec, vaapidec, nvdec, mfxdec)))]
        let _ = device;
        #[cfg(any(d3d11dec, vaapidec, nvdec, mfxdec))]
        {
            // SAFETY: the device outlives every decoder made from it, as
            // `new_on` promises and `Live::new` has none.
            #[cfg(d3d11dec)]
            let opened = unsafe { Hardware::open(device, codec) };
            // Linux's decoder opens the VA-API device itself — the render node
            // the machine has, whose driver `LIBVA_DRIVER_NAME` names if it
            // names one — and exports the surfaces it decodes into, so it takes
            // no device of the caller's: a picture is imported by whoever shows
            // it rather than made on the window's own.
            #[cfg(not(d3d11dec))]
            let opened = {
                let _ = device;
                Hardware::new(codec)
            };
            match opened {
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
        }
        Self::software(codec)
    }

    /// The software decoder alone, where the machine's has given up on a
    /// stream or was never tried.
    pub fn software(codec: Codec) -> Result<Self> {
        Ok(Self {
            codec,
            #[cfg(any(d3d11dec, vaapidec, nvdec, mfxdec))]
            hardware: None,
            software: Some(Decoder::new(codec)?),
        })
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    /// Whether the machine's decoder is the one reading this stream, rather
    /// than the software one.
    #[cfg(any(d3d11dec, vaapidec, nvdec, mfxdec))]
    pub fn on_the_card(&self) -> bool {
        self.hardware.is_some()
    }

    /// Whether the machine's decoder is the one reading this stream, rather
    /// than the software one.
    #[cfg(not(any(d3d11dec, vaapidec, nvdec, mfxdec)))]
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
    #[cfg_attr(not(any(d3d11dec, vaapidec, nvdec, mfxdec)), allow(unused_variables))]
    pub fn decode(&mut self, frame: &[u8], keyframe: bool) -> Result<Option<Frame<'_>>> {
        #[cfg(any(d3d11dec, vaapidec, nvdec, mfxdec))]
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
                    // The card's decoder is dropped rather than asked again:
                    // it read the stream as far as it could, and the error
                    // tells the caller to wait for a keyframe, at which the
                    // software decoder starts on the stream.
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
