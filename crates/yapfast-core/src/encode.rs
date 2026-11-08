//! Encoding for Go Live and the camera, through native/vp8.c with the libvpx
//! inside libwebrtc, native/h264.c with x264 (vendor/x264-src), and
//! native/hwenc.c with FFmpeg's hardware encoders (vendor/ffmpeg-src) where
//! the GPU has one. Frames arrive as RGBA or I420 and leave as VP8 frames,
//! H264 or H265 access units in Annex B form, or AV1 temporal units of OBUs
//! that give their sizes.

use std::{
    ffi::{c_char, c_int, c_long, c_longlong, c_uchar, c_uint, c_ulong},
    sync::Arc,
    time::Duration,
};

use anyhow::{Result, bail};
use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoBuffer},
};
use yapfast_discord::{
    rtp::Codec,
    stream_connection::{Encoded, Pool},
};

use crate::gpu::{self, Candidate, Luid, Reach};
// FFmpeg's and x264's libraries, which native/hwenc.c and native/h264.c
// call: linked only while named.
#[cfg(hwenc)]
use ffmpeg_src as _;
use x264_src as _;

#[repr(C)]
struct Raw {
    _private: [u8; 0],
}

type Encode = unsafe extern "C" fn(
    *mut Raw,
    *const c_uchar,
    c_int,
    *const c_uchar,
    c_int,
    *const c_uchar,
    c_int,
    c_longlong,
    c_ulong,
    c_int,
    *mut c_int,
) -> c_long;
/// Copies the access unit the last encode produced into the buffer given,
/// which must hold it, and lets the encoder give up its copy.
type Drain = unsafe extern "C" fn(*mut Raw, *mut c_uchar, usize) -> c_long;
/// The bytes an encoder keeps for the unit it produces, reused each frame.
type Staging = unsafe extern "C" fn(*mut Raw) -> usize;
type SetBitrate = unsafe extern "C" fn(*mut Raw, c_uint) -> c_int;
type Free = unsafe extern "C" fn(*mut Raw);

unsafe extern "C" {
    fn yapfast_vp8_new(
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        threads: c_uint,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_vp8_encode(
        encoder: *mut Raw,
        y: *const c_uchar,
        y_stride: c_int,
        u: *const c_uchar,
        u_stride: c_int,
        v: *const c_uchar,
        v_stride: c_int,
        pts: c_longlong,
        duration: c_ulong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_vp8_drain(encoder: *mut Raw, out: *mut c_uchar, capacity: usize) -> c_long;
    fn yapfast_vp8_staging(encoder: *mut Raw) -> usize;
    fn yapfast_vp8_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_vp8_free(encoder: *mut Raw);
    fn yapfast_h264_new(
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        threads: c_uint,
        effort: c_uint,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_h264_encode(
        encoder: *mut Raw,
        y: *const c_uchar,
        y_stride: c_int,
        u: *const c_uchar,
        u_stride: c_int,
        v: *const c_uchar,
        v_stride: c_int,
        pts: c_longlong,
        duration: c_ulong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_h264_drain(encoder: *mut Raw, out: *mut c_uchar, capacity: usize) -> c_long;
    fn yapfast_h264_staging(encoder: *mut Raw) -> usize;
    fn yapfast_h264_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_h264_free(encoder: *mut Raw);
}

#[cfg(hwenc)]
unsafe extern "C" {
    /// Whether this FFmpeg build has the encoder `name`, which is how this side
    /// learns what the shipped build was configured with: an encoder the build
    /// hasn't got is one no stream can offer, and one it has may still fail to
    /// open on this machine.
    fn yapfast_hwenc_has(name: *const c_char) -> c_int;
    /// What an encoder is asked for beyond the picture: the frame's shape and
    /// rate, the bitrate to aim at, the tuning, the encoders to try and the
    /// kind of GPU the stream asked for.
    fn yapfast_hwenc_new(
        ask: *const OpenRequest,
        report: *mut c_char,
        report_size: usize,
    ) -> *mut Raw;
    #[allow(clippy::too_many_arguments)]
    fn yapfast_hwenc_encode(
        encoder: *mut Raw,
        y: *const c_uchar,
        y_stride: c_int,
        u: *const c_uchar,
        u_stride: c_int,
        v: *const c_uchar,
        v_stride: c_int,
        pts: c_longlong,
        duration: c_ulong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_hwenc_drain(encoder: *mut Raw, out: *mut c_uchar, capacity: usize) -> c_long;
    fn yapfast_hwenc_staging(encoder: *mut Raw) -> usize;
    fn yapfast_hwenc_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_hwenc_notes(encoder: *mut Raw, notes: *mut c_char, notes_size: usize);
    /// FFmpeg's own name for the encoder that opened, such as "h264_nvenc",
    /// which is what this side matches its choice of backend back to: the
    /// report beside it names VA-API's device too.
    fn yapfast_hwenc_engine(encoder: *mut Raw, name: *mut c_char, name_size: usize);
    fn yapfast_hwenc_config(encoder: *mut Raw, config: *mut c_char, config_size: usize);
    fn yapfast_hwenc_tuning(encoder: *mut Raw, tuning: *mut c_char, tuning_size: usize);
    fn yapfast_hwenc_late(encoder: *mut Raw) -> c_uint;
    fn yapfast_hwenc_free(encoder: *mut Raw);
}

/// What native/hwenc.c is asked for, laid out as its `struct yapfast_hwenc_open`.
#[cfg(hwenc)]
#[repr(C)]
struct OpenRequest {
    codec: c_uint,
    width: c_uint,
    height: c_uint,
    fps: c_uint,
    kbps: c_uint,
    /// The tuning asked for by name, or null for the shipped one.
    tuning: *const c_char,
    /// FFmpeg's encoders to try, best first and separated by commas: how the
    /// adapter preference reaches the device choice. Null for this build's own
    /// order, which is what a platform that can't list its adapters gets.
    encoders: *const c_char,
    /// "dedicated" or "integrated", for the render nodes VA-API chooses
    /// between itself; null when nothing is preferred.
    preference: *const c_char,
}

/// RTP's video clock.
pub const CLOCK: u64 = 90_000;

/// The time of a point in a stream, `elapsed` into it, in [`CLOCK`] units:
/// where the encoder's frames, the camera's and the RTP timestamps on their
/// packets are all timed from, so a frame can't be timed two ways. RTP's
/// timestamp is 32 bits and wraps after 13 hours, and [`Frame::timestamp`]
/// is the low half of this.
pub fn timestamp(elapsed: Duration) -> u64 {
    // Multiplied before it's divided, or most of a millisecond would be lost.
    elapsed.as_micros() as u64 * CLOCK / 1_000_000
}

/// How much work the H264 encoder puts into each frame: x264's superfast
/// preset, or its faster one, for a sharper picture at about twice the CPU.
/// VP8 takes no effort.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    #[default]
    Light,
    High,
}

/// The tuning the stream asks a GPU's encoder for: how hard it works per
/// frame, how its bitrate cap is spent, and how often a viewer waits for a
/// keyframe. The set itself is `NVENC_TUNINGS` in native/hwenc.c, which knows
/// what each setting needs and what it costs; this names one row of it.
///
/// It is not a setting a person chooses. The stream's quality setting is
/// [`Effort`], which picks between x264's presets, and the GPU's encoders have
/// always run one tuning, so there is one row to name: adding a candidate is a
/// row there and a variant here, and nothing else, and it stays out of the
/// settings until a comparison of its picture puts it there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tuning {
    /// p4 at low-latency tuning, capped VBR, no frame delay and an IDR on
    /// demand: where screen recordings and games landed.
    #[default]
    LowLatency,
}

impl Tuning {
    /// The row's name, as native/hwenc.c reads it.
    #[cfg(hwenc)]
    fn name(self) -> &'static std::ffi::CStr {
        match self {
            Self::LowLatency => c"p4",
        }
    }
}

/// What an encoder was opened with: which engine runs, what it was asked to
/// tune for, what its shim says it took, and the optional settings its driver
/// dropped. Rust chooses nothing here but that ask, so this is what the
/// stream got rather than what it wanted, and it holds for as long as the
/// encoder does: the bitrate, which moves with the network, is
/// [`Encoder::bitrate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Configuration {
    /// Which encoder runs: "libvpx", "x264", or FFmpeg's name for a hardware
    /// one, such as "h264_nvenc".
    pub engine: String,
    /// The tuning the stream asked for, in that row's own words: "p4" for
    /// NVENC, or the effort x264 was given.
    pub asked: String,
    /// What the encoder reports it was opened with — its preset, its rate
    /// control and its keyframe interval, such as "p4 preset, vbr rate
    /// control, keyframe every 90 frames" — and empty for the encoders on the
    /// processor, whose shims report nothing back and so claim nothing. A
    /// driver that refused one of these said so through `settings_dropped`
    /// beside it.
    pub tuning: String,
    /// The optional settings the driver didn't keep, as its shim named them
    /// and separated by commas ("preset,tune"); empty when it kept them all.
    /// A handful of names, read once when the encoder opens, so the stream's
    /// report can say them without growing or reading them again.
    pub settings_dropped: String,
}

pub struct Encoder {
    raw: *mut Raw,
    codec: Codec,
    effort: Effort,
    configuration: Configuration,
    encode: Encode,
    /// Copies the unit the last encode produced out of the encoder, once the
    /// buffer it goes in holds it.
    drain: Drain,
    /// The bytes the encoder keeps for the unit it produces.
    staging: Staging,
    set_bitrate: SetBitrate,
    free: Free,
    /// The bitrate aimed for, in kbit/s.
    kbps: u32,
    width: u32,
    height: u32,
    fps: u32,
    /// How many frames the GPU's encoder has answered late so far, as last
    /// read, so that a warning names each new one once.
    late: u32,
    /// The hardware encoder that opened, and what reaching it costs; `None`
    /// for an encoder on the processor.
    chosen: Option<Candidate>,
    /// The adapter the capture filled its frames on, when the caller said:
    /// which encoder is asked first, and which a failure reopens.
    capture: Option<Luid>,
    /// FFmpeg's names for the hardware encoders this encoder has run as, so a
    /// failure reopens one this stream hasn't already seen fail.
    tried: Vec<String>,
    /// How many pictures the native encoder has been asked for: a test holds
    /// it to one a picture, since a picture encoded twice is one referencing
    /// the copy the receiver never got.
    #[cfg(test)]
    submitted: usize,
    /// The frame as I420, for [`Self::encode`]: a full-size luma plane, then
    /// two quarter-size chroma planes. Empty until it's first needed: the
    /// screen and the camera arrive as I420.
    planes: Vec<u8>,
    /// The buffers encoded frames are copied into, reused as frames are sent.
    out: Arc<Pool>,
}

// SAFETY: no library keeps thread-local state past a call; one thread uses
// the encoder at a time.
unsafe impl Send for Encoder {}

/// One encoded frame, with the buffer it was encoded into: the bytes stay its
/// own as long as it lives, and the buffer goes back to the encoder's pool
/// when it's dropped.
pub struct Frame {
    pub data: Encoded,
    pub keyframe: bool,
    /// Its time in the stream, in 90 kHz units, cut to the 32 bits RTP sends
    /// it in. It travels with the bytes rather than being worked out again
    /// for the packet, so a frame is never sent under another frame's time.
    pub timestamp: u32,
}

/// An opened encoder's handle, its functions, and what it was opened with.
struct Opened {
    raw: *mut Raw,
    encode: Encode,
    drain: Drain,
    staging: Staging,
    set_bitrate: SetBitrate,
    free: Free,
    configuration: Configuration,
    /// FFmpeg's own name for it ("h264_nvenc"), where
    /// [`Configuration::engine`] names VA-API's device as well.
    #[cfg_attr(not(hwenc), allow(dead_code))]
    name: String,
    /// The candidate that opened: which backend, on which adapter, and what a
    /// captured frame costs to reach it. `None` for an encoder on the
    /// processor, and for a hardware one opened where this machine's adapters
    /// couldn't be listed.
    #[cfg_attr(not(hwenc), allow(dead_code))]
    candidate: Option<Candidate>,
}

/// What one call of an encoder made of a frame.
#[derive(Clone, Copy)]
enum Written {
    /// Its size, and whether it's a keyframe. The size may be larger than the
    /// buffer the frame will be sent in, which grows to hold it: the encoder
    /// keeps the frame until [`Encoder::drain_produced`] copies it out.
    Frame(usize, bool),
    /// The encoder skipped it.
    Skipped,
}

impl Encoder {
    /// An encoder for `width` by `height` frames, which must be even. With
    /// `hardware`, H264 goes to the GPU's encoder where there is one, and to
    /// x264 otherwise. H265 and AV1 have only the GPU's.
    pub fn new(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        hardware: bool,
    ) -> Result<Self> {
        Self::open(codec, width, height, fps, kbps, effort, hardware, None)
    }

    /// An encoder like [`Self::new`], for a capture whose frames were filled on
    /// `capture`: the adapter is what the preference and the adapter list put
    /// first, and a stream whose capture is on another adapter is told which
    /// encoder crossed to it rather than which one merely opened. `None` is a
    /// frame with no adapter of its own — a camera's, a clip's — and the
    /// preference alone decides.
    #[allow(clippy::too_many_arguments)]
    pub fn on_adapter(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        hardware: bool,
        capture: Option<Luid>,
    ) -> Result<Self> {
        Self::open(codec, width, height, fps, kbps, effort, hardware, capture)
    }

    #[allow(clippy::too_many_arguments)]
    fn open(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        hardware: bool,
        capture: Option<Luid>,
    ) -> Result<Self> {
        if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            bail!("{codec:?} frames need an even size, not {width}x{height}");
        }
        let opened = match hardware.then(|| {
            open_hardware(codec, width, height, fps, kbps, Tuning::default(), capture, &[])
        }) {
            Some(Ok(opened)) => opened,
            refused => {
                // The GPU was asked for and passed this codec over: the reason
                // it gave says hardware was refused, rather than the stream
                // being on the processor for no stated reason.
                if let Some(Err(why)) = refused {
                    tracing::warn!(
                        why = why.as_str(),
                        fallback = true,
                        "the GPU's encoder was refused; encoding on the processor"
                    );
                }
                open_software(codec, width, height, fps, kbps, effort)?
            }
        };
        let pixels = (width * height) as usize;
        Ok(Self {
            raw: opened.raw,
            codec,
            effort,
            configuration: opened.configuration,
            encode: opened.encode,
            drain: opened.drain,
            staging: opened.staging,
            set_bitrate: opened.set_bitrate,
            free: opened.free,
            kbps,
            width,
            height,
            fps,
            late: 0,
            chosen: opened.candidate,
            capture,
            tried: vec![opened.name],
            #[cfg(test)]
            submitted: 0,
            planes: Vec::new(),
            // Starts lean; a keyframe of noise is the most the pool can grow to.
            out: Arc::new(Pool::new(pixels * 2 + 4096)),
        })
    }

    /// Which encoder runs: "libvpx", "x264", or FFmpeg's name for a
    /// hardware one, such as "h264_nvenc".
    pub fn engine(&self) -> &str {
        &self.configuration.engine
    }

    /// What this encoder was opened with, for a log or a measurement to quote
    /// beside its numbers.
    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    fn hardware(&self) -> bool {
        !matches!(self.engine(), "libvpx" | "x264")
    }

    /// The hardware encoder that runs, and what reaching it costs: the
    /// backend, the adapter it is on and whether a captured frame crosses to
    /// it. `None` for an encoder on the processor, whose frames are already
    /// the processor's, and for a hardware one opened where this machine's
    /// adapters couldn't be listed.
    ///
    /// A hardware encoder here is fed from system memory either way: the
    /// zero-copy path is the native D3D11 one in `d3d11.rs`, not this. What
    /// this says is which adapter the upload ends up on.
    pub fn selection(&self) -> Option<Candidate> {
        self.chosen
    }

    /// Puts `opened` in place of the encoder running now.
    fn replace(&mut self, opened: Opened) {
        // SAFETY: raw came from its codec's constructor and is freed once.
        unsafe { (self.free)(self.raw) };
        self.raw = opened.raw;
        self.encode = opened.encode;
        self.drain = opened.drain;
        self.staging = opened.staging;
        self.set_bitrate = opened.set_bitrate;
        self.free = opened.free;
        self.configuration = opened.configuration;
        self.chosen = opened.candidate;
        self.tried.push(opened.name);
        // The encoder that replaced it has dropped none of its own frames: a
        // count left from the one that failed would read 0 until this one's
        // drops overtook it.
        self.late = 0;
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The bitrate aimed for, in kbit/s.
    pub fn bitrate(&self) -> u32 {
        self.kbps
    }

    /// The bytes the encoder's output buffers hold now, and the most they have
    /// held: what a stream's report shows so a pool that grew for one large
    /// frame says what it kept.
    pub fn pool_bytes(&self) -> (u64, u64) {
        let (live, peak) = self.out.bytes();
        (live as u64, peak as u64)
    }

    /// Gives back the output buffers lying idle, for a machine short of memory:
    /// a buffer being encoded into or waiting to be sent is never touched.
    pub fn trim_pool(&self) {
        self.out.trim();
    }

    /// How many buffers the encoder's pool has allocated: the tests hold it
    /// to the pool's bound.
    #[cfg(test)]
    fn created(&self) -> usize {
        self.out.created()
    }

    /// How many pictures the native encoder was asked to encode: more than one
    /// for a picture is one encoded a second time, which a test fails on.
    #[cfg(test)]
    fn submitted(&self) -> usize {
        self.submitted
    }

    /// The bytes the encoder keeps to copy the unit it produced out of, reused
    /// for the next frame: one frame's worth at most, which is what a test
    /// holds it to rather than a growing history.
    #[cfg(test)]
    fn staging(&self) -> usize {
        // SAFETY: raw came from this codec's constructor and is still live.
        unsafe { (self.staging)(self.raw) }
    }

    /// Aims for `kbps` from the next frame on, without a keyframe.
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        // SAFETY: raw came from this codec's constructor and is still live.
        if unsafe { (self.set_bitrate)(self.raw, kbps) } != 0 {
            bail!("the {:?} encoder refused {kbps} kbit/s", self.codec);
        }
        self.kbps = kbps;
        Ok(())
    }

    /// Encodes RGBA pixels of the encoder's size, shown at `pts` in
    /// [`CLOCK`] units. None when the encoder skipped the frame.
    pub fn encode(&mut self, rgba: &[u8], pts: u64, keyframe: bool) -> Result<Option<Frame>> {
        let (width, height) = (self.width, self.height);
        let pixels = (width * height) as usize;
        if rgba.len() < pixels * 4 {
            bail!("a frame smaller than the encoder's");
        }
        let mut planes = std::mem::take(&mut self.planes);
        planes.resize(pixels + pixels / 2, 0);
        let (y, chroma) = planes.split_at_mut(pixels);
        let (u, v) = chroma.split_at_mut(pixels / 4);
        // libyuv names formats by their order in a little-endian word: its
        // ABGR is R, G, B, A in memory. This is the conversion the encoders'
        // colour claim is about: libyuv writes BT.601's matrix over its
        // limited range, so that is what native/h264.c and native/hwenc.c tell
        // each encoder, and neither claim is right apart from the other.
        yuv_helper::abgr_to_i420(
            rgba,
            width * 4,
            y,
            width,
            u,
            width / 2,
            v,
            width / 2,
            width as i32,
            height as i32,
        );
        let encoded =
            self.encode_planes([(y, width), (u, width / 2), (v, width / 2)], pts, keyframe);
        self.planes = planes;
        encoded
    }

    /// Encodes an I420 picture of the encoder's size, like [`Self::encode`].
    pub fn encode_i420(
        &mut self,
        picture: &I420Buffer,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        if (picture.width(), picture.height()) != (self.width, self.height) {
            bail!("a picture of another size than the encoder's");
        }
        let (y, u, v) = picture.data();
        let (y_stride, u_stride, v_stride) = picture.strides();
        self.encode_planes([(y, y_stride), (u, u_stride), (v, v_stride)], pts, keyframe)
    }

    /// Encodes a camera's picture, at least the encoder's size: its top left
    /// corner, when it's larger, as a picture an odd size loses a row or a
    /// column.
    pub fn encode_picture(
        &mut self,
        picture: &crate::picture::I420<'_>,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        if picture.width < self.width || picture.height < self.height {
            bail!("a picture smaller than the encoder's");
        }
        let [y_stride, u_stride, v_stride] = picture.strides;
        self.encode_planes(
            [
                (picture.y, y_stride),
                (picture.u, u_stride),
                (picture.v, v_stride),
            ],
            pts,
            keyframe,
        )
    }

    /// Encodes three planes, each with its stride, and copies the access unit
    /// the encoder produced into a buffer from the encoder's pool: the frame, or
    /// None when the encoder skipped it. A unit too big for the buffers grows
    /// them, doubling until one holds it, and then goes into the grown buffer
    /// from the encoder's own copy of it. The picture is not submitted again: an
    /// encoder that had already encoded it moved on, so a second submission
    /// would reference a copy the receiver never got and cost the work again.
    /// When a hardware H264 encoder fails, x264 takes over from this frame, with
    /// a keyframe, and that submission does encode the picture a second time;
    /// H265 and AV1 have nothing to take over.
    fn encode_planes(
        &mut self,
        planes: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        let written = self.encode_once_fallback(planes, pts, keyframe)?;
        self.note_late();
        let (size, keyframe) = match written {
            Written::Skipped => return Ok(None),
            Written::Frame(size, keyframe) => (size, keyframe),
        };
        // The buffer that holds a frame is the one the pool hands out: a unit
        // that outgrew it grows the pool first, so the copy below fits first
        // time and the frame stays the one the encoder made.
        while self.out.capacity() < size {
            if !self.out.grow() {
                bail!(
                    "the {:?} frame doesn't fit {} bytes",
                    self.codec,
                    self.out.capacity()
                );
            }
        }
        let mut out = self.out.checkout();
        let copied = self.drain_produced(&mut out)?;
        debug_assert_eq!(copied, size, "the encoder gave back another frame");
        out.finish(copied);
        Ok(Some(Frame {
            data: out,
            keyframe,
            // The frame was encoded at this time, whatever the pool had to do
            // to hold it: one picture, one submission.
            timestamp: pts as u32,
        }))
    }

    /// Copies the access unit the encoder produced into `out`, whose buffer the
    /// pool has already grown to hold it, and lets the encoder give up its copy:
    /// this is the drain half of encode-then-copy, and the only copy of the
    /// frame's bytes between the encoder and the packetizer.
    fn drain_produced(&mut self, out: &mut Encoded) -> Result<usize> {
        let spare = out.spare();
        // SAFETY: the buffer is at least the unit's size, which the pool was
        // grown to before this call, and both stay put during it.
        let copied = unsafe { (self.drain)(self.raw, spare.as_mut_ptr(), spare.len()) };
        match copied {
            ..0 => bail!("the {:?} encoder lost the frame it produced", self.codec),
            copied => Ok(copied as usize),
        }
    }

    /// Encodes the planes, with x264 taking over when the GPU's H264 encoder
    /// failed.
    fn encode_once_fallback(
        &mut self,
        planes: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Written> {
        match self.encode_once(planes, pts, keyframe) {
            Err(error) if self.hardware() => {
                tracing::warn!(
                    engine = self.engine(),
                    %error,
                    "the GPU's encoder failed; reopening another"
                );
                self.reopen()?;
                // The picture is submitted again, with a keyframe: the
                // receiver never got the one that failed, so the frame it
                // holds is one this stream's pictures no longer reference.
                self.encode_once(planes, pts, true)
            }
            written => written,
        }
    }

    /// Replaces the encoder that failed with the next one that opens: another
    /// hardware encoder on this machine — the same codec, on another adapter or
    /// backend, which is what the adapter preference's fallbacks are for — or,
    /// when none is left, the processor's, which H264 alone has.
    ///
    /// The codec is kept rather than renegotiated: a stream's viewers were
    /// promised it, so a failure changes the encoder and not the format.
    fn reopen(&mut self) -> Result<()> {
        let (codec, width, height, fps, kbps, effort) =
            (self.codec, self.width, self.height, self.fps, self.kbps, self.effort);
        if let Some(opened) = open_hardware(
            codec,
            width,
            height,
            fps,
            kbps,
            Tuning::default(),
            self.capture,
            &self.tried,
        )
        .ok()
        {
            tracing::warn!(
                engine = opened.name.as_str(),
                fallback = true,
                "the GPU's encoder was replaced by another on this machine"
            );
            self.replace(opened);
            return Ok(());
        }
        // Nothing left on a GPU, so it is the processor's or nothing: H265 and
        // AV1 have no encoder there, and the error stays the GPU's.
        let opened = open_software(codec, width, height, fps, kbps, effort)?;
        // Replaced first, so the encoder this names is the one that runs now
        // rather than the one that just failed.
        self.replace(opened);
        tracing::warn!(
            engine = self.engine(),
            fallback = true,
            "the GPU's encoder failed; encoding on the processor"
        );
        Ok(())
    }

    /// How many frames the GPU's encoder has answered late, and which
    /// native/hwenc.c dropped rather than let them be sent under a newer
    /// frame's time. An encoder on the processor has none to report.
    fn late(&self) -> u32 {
        #[cfg(hwenc)]
        {
            if self.hardware() {
                // SAFETY: a hardware engine is native/hwenc.c's, and is live.
                return unsafe { yapfast_hwenc_late(self.raw) };
            }
        }
        0
    }

    /// Warns about frames the GPU's encoder answered late, when one is
    /// answered: a driver falling behind, whose frame is dropped rather than
    /// sent under this frame's time.
    fn note_late(&mut self) {
        let late = self.late();
        let fresh = late.saturating_sub(self.late);
        self.late = late;
        if fresh == 0 {
            return;
        }
        tracing::warn!(
            engine = self.engine(),
            frames = fresh,
            "the GPU's encoder answered frames late; they were dropped"
        );
    }

    /// Encodes the planes, and reports what the encoder produced. What it
    /// produced stays in the encoder, for [`Self::drain_produced`] to copy out:
    /// no output buffer crosses this boundary, so a frame too big for the one
    /// waiting at the other side is copied rather than encoded again.
    fn encode_once(
        &mut self,
        [(y, y_stride), (u, u_stride), (v, v_stride)]: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Written> {
        let mut is_key: c_int = 0;
        #[cfg(test)]
        {
            self.submitted += 1;
        }
        // SAFETY: the planes hold a picture of this encoder's size at these
        // strides, and the flag has a place of its own; all stay put during
        // the call.
        let written = unsafe {
            (self.encode)(
                self.raw,
                y.as_ptr(),
                y_stride as c_int,
                u.as_ptr(),
                u_stride as c_int,
                v.as_ptr(),
                v_stride as c_int,
                pts as c_longlong,
                (CLOCK / u64::from(self.fps)) as c_ulong,
                c_int::from(keyframe),
                &mut is_key,
            )
        };
        match written {
            ..0 => bail!("{:?} encoding failed ({written})", self.codec),
            0 => Ok(Written::Skipped),
            _ => Ok(Written::Frame(written as usize, is_key != 0)),
        }
    }
}

/// libvpx for VP8, x264 for H264.
fn open_software(
    codec: Codec,
    width: u32,
    height: u32,
    fps: u32,
    kbps: u32,
    effort: Effort,
) -> Result<Opened> {
    let threads = std::thread::available_parallelism().map_or(1, |cores| cores.get().min(4)) as u32;
    // SAFETY: plain values; null means the library refused the configuration.
    let opened = match codec {
        Codec::Vp8 => Opened {
            raw: unsafe { yapfast_vp8_new(width, height, fps, kbps, threads) },
            encode: yapfast_vp8_encode,
            drain: yapfast_vp8_drain,
            staging: yapfast_vp8_staging,
            set_bitrate: yapfast_vp8_set_bitrate,
            free: yapfast_vp8_free,
            configuration: software_configuration("libvpx", effort),
            name: "libvpx".to_owned(),
            candidate: None,
        },
        Codec::H264 => Opened {
            raw: unsafe { yapfast_h264_new(width, height, fps, kbps, threads, effort as c_uint) },
            encode: yapfast_h264_encode,
            drain: yapfast_h264_drain,
            staging: yapfast_h264_staging,
            set_bitrate: yapfast_h264_set_bitrate,
            free: yapfast_h264_free,
            configuration: software_configuration("x264", effort),
            name: "x264".to_owned(),
            candidate: None,
        },
        Codec::H265 | Codec::Av1 => bail!(
            "{} encodes only on a GPU, and this one has no encoder for it",
            codec.name()
        ),
    };
    if opened.raw.is_null() {
        bail!("could not start the {codec:?} encoder");
    }
    // The settings a stream is compared by, said once rather than left to be
    // guessed from the picture: x264's effort is the only one of them Rust
    // chooses, and native/h264.c's own choices are its to report, not this
    // side's to claim.
    tracing::debug!(
        engine = opened.configuration.engine.as_str(),
        asked = opened.configuration.asked.as_str(),
        "encoding on the processor"
    );
    Ok(opened)
}

/// What an encoder on the processor was opened with: the one setting Rust
/// chooses for it is its effort, and its shim reports nothing back of its own,
/// so the tuning stays empty rather than become a claim about presets that
/// nothing read.
fn software_configuration(engine: &str, effort: Effort) -> Configuration {
    Configuration {
        engine: engine.to_owned(),
        asked: format!("{effort:?} effort"),
        tuning: String::new(),
        settings_dropped: String::new(),
    }
}

/// FFmpeg's name for `codec`'s encoder on `backend`, or `None` where that
/// backend has none for it — VP8 is the processor's alone — so no candidate is
/// opened on a name that doesn't exist.
fn engine_name(backend: gpu::Backend, codec: Codec) -> Option<&'static str> {
    use gpu::Backend::{Amf, Nvenc, Qsv, Vaapi};
    Some(match (codec, backend) {
        (Codec::Vp8, _) => return None,
        (Codec::H264, Nvenc) => "h264_nvenc",
        (Codec::H265, Nvenc) => "hevc_nvenc",
        (Codec::Av1, Nvenc) => "av1_nvenc",
        (Codec::H264, Amf) => "h264_amf",
        (Codec::H265, Amf) => "hevc_amf",
        (Codec::Av1, Amf) => "av1_amf",
        (Codec::H264, Qsv) => "h264_qsv",
        (Codec::H265, Qsv) => "hevc_qsv",
        (Codec::Av1, Qsv) => "av1_qsv",
        (Codec::H264, Vaapi) => "h264_vaapi",
        (Codec::H265, Vaapi) => "hevc_vaapi",
        (Codec::Av1, Vaapi) => "av1_vaapi",
    })
}

/// Whether this FFmpeg build has the encoder `name`: what its configure line
/// enabled, which is not the same as this machine being able to open it.
#[cfg(hwenc)]
fn has(name: &str) -> bool {
    let Ok(name) = std::ffi::CString::new(name) else {
        return false;
    };
    // SAFETY: a name with a zero of its own, which the shim only asks FFmpeg
    // about.
    unsafe { yapfast_hwenc_has(name.as_ptr()) != 0 }
}

#[cfg(not(hwenc))]
fn has(_: &str) -> bool {
    false
}

/// The encoders a stream may try for one codec, in the order it may try them.
#[cfg(hwenc)]
struct Plan {
    /// What to open, best first: the encoder on the adapter the preference
    /// picks, then the one on the adapter the capture filled its frames on,
    /// then the rest of the machine.
    tried: Vec<(Candidate, &'static str)>,
    /// The encoders that order wanted and this FFmpeg wasn't built with, by
    /// name: what a machine with Intel's GPU is told when the build has no
    /// Quick Sync.
    missing: Vec<&'static str>,
}

/// The encoders to try for `codec` and the ones this build hasn't got.
///
/// Empty when this machine's adapters couldn't be listed, which leaves
/// native/hwenc.c's own order — the encoder FFmpeg finds first — exactly as it
/// was before the adapter choice existed.
#[cfg(hwenc)]
fn plan(codec: Codec, capture: Option<Luid>) -> Plan {
    let mut plan = Plan {
        tried: Vec::new(),
        missing: Vec::new(),
    };
    let adapters = gpu::machine();
    if adapters.is_empty() {
        return plan;
    }
    for candidate in gpu::candidates(gpu::preference(), &adapters, capture) {
        let Some(name) = engine_name(candidate.backend, codec) else {
            continue;
        };
        // One name, one open: two adapters of one vendor offer the encoder of
        // the same name, and opening it twice tells the second nothing.
        if plan.tried.iter().any(|(_, tried)| *tried == name)
            || plan.missing.contains(&name)
        {
            continue;
        }
        if has(name) {
            plan.tried.push((candidate, name));
        } else {
            plan.missing.push(name);
        }
    }
    plan
}

/// The GPU's encoder for `codec`, if it has one that opens: the first of the
/// candidates that isn't in `skip`, in the order the adapter preference puts
/// them. The error is what was tried and why each was passed over, in the
/// encoders' own words, for a log or the stream's report.
#[cfg(hwenc)]
#[allow(clippy::too_many_arguments)]
fn open_hardware(
    codec: Codec,
    width: u32,
    height: u32,
    fps: u32,
    kbps: u32,
    tuning: Tuning,
    capture: Option<Luid>,
    skip: &[String],
) -> Result<Opened, String> {
    let Some(number) = ([Codec::H264, Codec::H265, Codec::Av1])
        .iter()
        .position(|one| *one == codec)
    else {
        return Err(format!("{} has no hardware encoder", codec.name()));
    };
    let plan = plan(codec, capture);
    let trying: Vec<(Candidate, &'static str)> = plan
        .tried
        .iter()
        .copied()
        .filter(|(_, name)| !skip.iter().any(|tried| tried == name))
        .collect();
    // The names to try, best first, which is how the adapter preference reaches
    // the device choice; null when nothing could choose, which leaves the
    // shim's own order.
    let listed = if trying.is_empty() {
        None
    } else {
        let names: Vec<&'static str> = trying.iter().map(|(_, name)| *name).collect();
        Some(std::ffi::CString::new(names.join(",")).map_err(|_| "a name with a zero in it".to_owned())?)
    };
    // The kind of GPU the stream asked for. native/hwenc.c uses it for the
    // render nodes VA-API chooses between itself on Linux; nothing else here
    // has a device choice of its own to give it.
    let preference = match gpu::preference() {
        gpu::Preference::Dedicated => c"dedicated",
        gpu::Preference::Integrated => c"integrated",
    };
    let request = OpenRequest {
        codec: number as c_uint,
        width,
        height,
        fps,
        kbps,
        tuning: tuning.name().as_ptr(),
        encoders: listed.as_ref().map_or(std::ptr::null(), |names| names.as_ptr()),
        preference: preference.as_ptr(),
    };
    let mut report = [0u8; 512];
    // SAFETY: the request's strings outlive the call, and the report is a
    // buffer of the size given, which the shim ends with a zero.
    let raw = unsafe { yapfast_hwenc_new(&request, report.as_mut_ptr().cast(), report.len()) };
    let report = c_text(&report);
    if raw.is_null() {
        let mut why = report;
        // An encoder the build hasn't got can't be reported on: naming it as one
        // this FFmpeg was never built with is what tells an Intel machine the
        // encoder it has is not in this build.
        if !plan.missing.is_empty() {
            if !why.is_empty() {
                why.push_str("; ");
            }
            why.push_str(&plan.missing.join(", "));
            why.push_str(": not in this FFmpeg");
        }
        tracing::info!(?codec, tried = why.as_str(), "no GPU encoder opened");
        return Err(why);
    }
    // Which encoder opened: FFmpeg's own name for it, since the report beside it
    // names VA-API's device as well.
    let mut name = [0u8; 32];
    // SAFETY: raw is an open encoder, and the buffer is as large as its size.
    unsafe { yapfast_hwenc_engine(raw, name.as_mut_ptr().cast(), name.len()) };
    let name = c_text(&name);
    let candidate = trying
        .iter()
        .find(|(_, tried)| *tried == name)
        .map(|(candidate, _)| *candidate);
    // The settings the encoder needs made it open; the ones only better are
    // its to drop, so the stream says which it isn't getting — in the log at
    // the open, and in every report while it runs.
    let settings_dropped = read_notes(yapfast_hwenc_notes, raw);
    if !settings_dropped.is_empty() {
        tracing::warn!(
            engine = report.as_str(),
            ?codec,
            ignored = settings_dropped.as_str(),
            "the GPU's encoder dropped optional settings"
        );
    }
    // What the encoder took, which Rust can't see from this side of the call:
    // the format it fills a frame in, the colour it was told, the time base
    // frames are timed in. A stream that looks wrong says which it encoded.
    let mut config = [0u8; 80];
    // SAFETY: raw is an open encoder, and the buffer is as large as its size.
    unsafe { yapfast_hwenc_config(raw, config.as_mut_ptr().cast(), config.len()) };
    // And how hard it works per frame, how its cap is spent and how often a
    // viewer waits for a keyframe: the settings a change to the picture's
    // quality moves, said at the open so a stream's picture can be compared
    // with the encoder that made it rather than with what it was meant to be.
    let mut opened_with = [0u8; 96];
    // SAFETY: raw is an open encoder, and the buffer is as large as its size.
    unsafe { yapfast_hwenc_tuning(raw, opened_with.as_mut_ptr().cast(), opened_with.len()) };
    let opened_with = c_text(&opened_with);
    tracing::debug!(
        engine = report.as_str(),
        ?codec,
        tuning = opened_with.as_str(),
        config = c_text(&config),
        "encoding on the GPU"
    );
    // Said once at the open, since it holds for as long as the encoder does:
    // this encoder reads its frames from system memory, and the adapter it is
    // on is the one the upload lands on.
    if let Some(candidate) = candidate {
        if candidate.reach == Reach::Crosses {
            tracing::warn!(
                engine = report.as_str(),
                backend = candidate.backend.name(),
                "the GPU's encoder is on another adapter than the capture: every frame is copied 
                 across to it"
            );
        } else {
            tracing::info!(
                engine = report.as_str(),
                backend = candidate.backend.name(),
                "encoding on the GPU's encoder from the processor: every frame is uploaded to it"
            );
        }
    }
    Ok(Opened {
        raw,
        encode: yapfast_hwenc_encode,
        drain: yapfast_hwenc_drain,
        staging: yapfast_hwenc_staging,
        set_bitrate: yapfast_hwenc_set_bitrate,
        free: yapfast_hwenc_free,
        configuration: Configuration {
            engine: report,
            asked: tuning.name().to_string_lossy().into_owned(),
            tuning: opened_with,
            settings_dropped,
        },
        name,
        candidate,
    })
}

/// The optional settings an encoder's shim says its driver didn't keep, by
/// name and separated by commas, and empty when it kept them all. Taking the
/// shim's function as an argument lets a test stand a fake shim in its place,
/// since no machine can make a driver drop a setting on demand.
#[cfg(hwenc)]
fn read_notes(
    notes: unsafe extern "C" fn(*mut Raw, *mut std::ffi::c_char, usize),
    encoder: *mut Raw,
) -> String {
    let mut written = [0u8; 96];
    // SAFETY: encoder is an open encoder, and the buffer is as large as its
    // size; the shim ends what it writes with a zero.
    unsafe { notes(encoder, written.as_mut_ptr().cast(), written.len()) };
    c_text(&written)
}

/// The text a shim wrote into `bytes`, which ends with a zero.
#[cfg(hwenc)]
fn c_text(bytes: &[u8]) -> String {
    std::ffi::CStr::from_bytes_until_nul(bytes).map_or_else(
        |_| String::new(),
        |text| text.to_string_lossy().into_owned(),
    )
}

#[cfg(not(hwenc))]
#[allow(clippy::too_many_arguments)]
fn open_hardware(
    _: Codec,
    _: u32,
    _: u32,
    _: u32,
    _: u32,
    _: Tuning,
    _: Option<Luid>,
    _: &[String],
) -> Result<Opened, String> {
    Err("this build has no hardware encoders".to_owned())
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: raw came from this codec's constructor and is freed once.
        unsafe { (self.free)(self.raw) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Decoder;
    use std::{
        collections::VecDeque,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering as AtomicOrdering},
        },
    };

    // The settings native/hwenc.c opens NVENC with for the tuning named, and
    // whether the stream can't do without each: the list the open reads, so a
    // test holds the shipped tuning to what the stream sends rather than to a
    // copy of it, and needs no GPU in the shape of the question.
    #[cfg(hwenc)]
    unsafe extern "C" {
        fn yapfast_hwenc_nvenc_settings(
            tuning: *const std::ffi::c_char,
            asked: *mut std::ffi::c_char,
            asked_size: usize,
        ) -> c_int;
    }

    /// Four frames of moving colour, the third a forced keyframe, and what
    /// the encoder that made them was opened with.
    fn encode_four(
        codec: Codec,
        effort: Effort,
        (width, height): (u32, u32),
        hardware: bool,
    ) -> (Vec<(bool, Vec<u8>)>, Configuration) {
        let mut encoder = Encoder::new(codec, width, height, 30, 500, effort, hardware).unwrap();
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let mut frames = vec![];
        for frame in 0..4u64 {
            for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
                pixel.copy_from_slice(&[(index as u64 + frame * 8) as u8, 90, 200, 255]);
            }
            if frame == 1 {
                encoder.set_bitrate(250).unwrap();
                assert_eq!(encoder.bitrate(), 250);
            }
            let encoded = encoder
                .encode(&rgba, frame * 3000, frame == 2)
                .unwrap()
                .unwrap();
            assert_eq!(
                encoded.timestamp,
                (frame * 3000) as u32,
                "the frame carries the time it was encoded at"
            );
            frames.push((encoded.keyframe, encoded.data.to_vec()));
        }
        assert!(Encoder::new(codec, 63, 36, 30, 500, effort, hardware).is_err());
        (frames, encoder.configuration().clone())
    }

    /// The stream's clock is worked out in [`timestamp`] and nowhere else:
    /// 90 kHz over RTP's 32 bits, which wrap after 13 hours.
    #[test]
    fn the_stream_clock_is_one_derivation() {
        assert_eq!(timestamp(Duration::ZERO), 0);
        assert_eq!(timestamp(Duration::from_millis(1)), 90);
        assert_eq!(timestamp(Duration::from_secs(1)), CLOCK);
        // 48,000 seconds is 4,320,000,000 ticks: more than 32 bits holds.
        assert_eq!(timestamp(Duration::from_secs(48_000)), 4_320_000_000);
        assert_eq!(timestamp(Duration::from_secs(48_000)) as u32, 25_032_704);
    }

    /// Three frames alive at once keep the bytes they were encoded with while
    /// the encoder encodes on: each owns its buffer, rather than viewing one
    /// the encoder overwrites.
    #[test]
    fn frames_keep_their_bytes_past_more_encodes() {
        let mut encoder = Encoder::new(Codec::Vp8, 64, 36, 30, 500, Effort::Light, false).unwrap();
        let mut rgba = vec![0u8; 64 * 36 * 4];
        let paint = |rgba: &mut [u8], frame: u64| {
            for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
                pixel.copy_from_slice(&[(index as u64 + frame * 8) as u8, 90, 200, 255]);
            }
        };
        let mut held = vec![];
        for frame in 0..3u64 {
            paint(&mut rgba, frame);
            held.push(
                encoder
                    .encode(&rgba, frame * 3000, frame == 0)
                    .unwrap()
                    .unwrap(),
            );
        }
        let encoded: Vec<Vec<u8>> = held.iter().map(|frame| frame.data.to_vec()).collect();
        for frame in 3..12u64 {
            paint(&mut rgba, frame);
            let _ = encoder.encode(&rgba, frame * 3000, false).unwrap();
        }
        for (frame, encoded) in held.iter().zip(&encoded) {
            assert_eq!(&*frame.data, encoded, "bytes changed under a live frame");
        }
    }

    /// A frame's buffer goes back to the pool when the frame is dropped, and
    /// the frames after it take that buffer rather than allocating.
    #[test]
    fn the_pool_takes_its_buffers_back() {
        let pool = Arc::new(Pool::new(1024));
        let held: Vec<Encoded> = (0..3).map(|_| pool.checkout()).collect();
        assert_eq!(pool.created(), 3);
        drop(held);
        let again: Vec<Encoded> = (0..3).map(|_| pool.checkout()).collect();
        assert_eq!(
            pool.created(),
            3,
            "the pool allocated a buffer while three were idle"
        );
        drop(again);
    }

    /// Growing doubles towards the cap a frame of the encoder's size can
    /// need, and stops there; a buffer of the size it left is dropped when
    /// it comes back.
    #[test]
    fn the_pool_grows_up_to_its_max() {
        let pool = Arc::new(Pool::new(1536 * 1024));
        assert_eq!(pool.capacity(), 512 * 1024);
        let old = pool.checkout();
        assert!(pool.grow());
        assert_eq!(pool.capacity(), 1024 * 1024);
        assert!(pool.grow());
        assert_eq!(pool.capacity(), 1536 * 1024, "clamped to the frame's cap");
        assert!(!pool.grow(), "already as large as a frame can need");
        drop(old);
        let mut grown = pool.checkout();
        assert_eq!(grown.spare().len(), 1536 * 1024);
        assert_eq!(
            pool.created(),
            2,
            "the buffer of the size it outgrew was dropped"
        );
    }

    /// A frame too big for the lean buffer grows the pool's buffer, is
    /// encoded into it, and leaves the pool at the size that held it. The
    /// shim is a stand-in, since no real encoder here makes a 700 KiB frame
    /// A frame too big for the lean buffer grows the pool's buffer, is copied
    /// into it from the encoder's own copy, and leaves the pool at the size
    /// that held it. The shim is a stand-in, since no real encoder here makes a
    /// 700 KiB frame on demand: it answers the size it produced, as
    /// native/h264.c and native/hwenc.c now do, and hands that frame over once.
    #[test]
    fn a_frame_too_big_for_the_lean_buffer_grows_the_pool() {
        const BULKY: usize = 700 * 1024;
        // What each encoder call did: a picture this size is one submission and
        // one copy, so a submitted count above the pictures encoded is a frame
        // encoded twice.
        static SUBMITTED: AtomicUsize = AtomicUsize::new(0);
        static DRAINED: AtomicUsize = AtomicUsize::new(0);

        // Produces a frame bigger than the pool's opening buffer, and keeps it
        // the way a real shim does: the size says how big it is, not whether it
        // fits.
        unsafe extern "C" fn produces_bulky(
            _encoder: *mut Raw,
            _y: *const c_uchar,
            _y_stride: c_int,
            _u: *const c_uchar,
            _u_stride: c_int,
            _v: *const c_uchar,
            _v_stride: c_int,
            _pts: c_longlong,
            _duration: c_ulong,
            _keyframe: c_int,
            is_key: *mut c_int,
        ) -> c_long {
            SUBMITTED.fetch_add(1, AtomicOrdering::Relaxed);
            // SAFETY: the caller gives a place for the flag.
            unsafe { *is_key = 1 };
            BULKY as c_long
        }

        // Hands the frame it kept to a buffer that holds it.
        unsafe extern "C" fn drains_bulky(
            _encoder: *mut Raw,
            out: *mut c_uchar,
            capacity: usize,
        ) -> c_long {
            if capacity < BULKY {
                return -1;
            }
            // SAFETY: the caller gives a buffer of `capacity` bytes, at least
            // BULKY here.
            unsafe { std::ptr::write_bytes(out, 0xab, BULKY) };
            DRAINED.fetch_add(1, AtomicOrdering::Relaxed);
            BULKY as c_long
        }

        let mut encoder =
            Encoder::new(Codec::H264, 1280, 720, 30, 500, Effort::Light, false).unwrap();
        assert_eq!(encoder.out.capacity(), 512 * 1024);
        encoder.encode = produces_bulky;
        encoder.drain = drains_bulky;
        let rgba = vec![0u8; 1280 * 720 * 4];
        let frame = encoder.encode(&rgba, 0, true).unwrap().unwrap();
        assert_eq!(frame.data.len(), BULKY);
        assert!(frame.keyframe);
        assert!(frame.data.iter().all(|&byte| byte == 0xab));
        // The frame that outgrew the pool was produced once and copied once:
        // one frame out, at the time it was captured, not a frame later.
        assert_eq!(frame.timestamp, 0);
        assert_eq!(SUBMITTED.load(AtomicOrdering::Relaxed), 1);
        assert_eq!(DRAINED.load(AtomicOrdering::Relaxed), 1);
        assert_eq!(encoder.out.capacity(), 1024 * 1024);
        drop(frame);
        // The grown buffer came back, and is the one the next frame uses.
        let again = encoder.encode(&rgba, 3000, false).unwrap().unwrap();
        assert_eq!(again.data.len(), BULKY);
        assert_eq!(again.timestamp, 3000);
        // The buffer is taken at the size that holds the frame, so the pool never
        // made one of the size it outgrew: one buffer, not two.
        assert_eq!(encoder.created(), 1);
        assert_eq!(
            SUBMITTED.load(AtomicOrdering::Relaxed),
            2,
            "two pictures were encoded, whatever the pool had to do"
        );
    }

    /// A frame that outgrows the pool is copied out of the encoder, not encoded
    /// again: every access unit decodes in order, one submission went in for each
    /// picture, and the encoder keeps one frame's worth of memory for the copy
    /// rather than a growing history of them.
    ///
    /// The pool's opening buffer is 512 KiB, so a frame has to be bigger than
    /// that to provoke the growth a stream sees when a picture of a hard scene
    /// arrives; a keyframe of noise at a high rate is. The frame after the
    /// keyframe then goes through a pool at its opening size, which is where the
    /// old code encoded that picture a second time. A second submission would be
    /// an inter frame referencing the copy the receiver never got, which is what
    /// decoding every access unit says isn't happening.
    #[test]
    fn a_frame_that_outgrows_the_pool_is_copied_not_encoded_again() {
        const FRAMES: u64 = 10;
        // 1080p at a rate a screen of noise can fill, so a frame of it is larger
        // than the pool's opening 512 KiB.
        const WIDTH: u32 = 1920;
        const HEIGHT: u32 = 1080;
        const KBPS: u32 = 200_000;
        // What a pool hands out before a frame grows it.
        const OPENING: usize = 512 * 1024;
        for (codec, hardware) in [
            (Codec::Vp8, false),
            (Codec::H264, false),
            (Codec::H264, true),
        ] {
            let mut encoder =
                match Encoder::new(codec, WIDTH, HEIGHT, 30, KBPS, Effort::Light, hardware) {
                    Ok(encoder) => encoder,
                    Err(why) => {
                        eprintln!("no {codec:?} encoder here (hardware {hardware}): {why:#}");
                        continue;
                    }
                };
            if hardware && !encoder.hardware() {
                eprintln!(
                    "no GPU encoder for {codec:?} here: the open fell back to {}",
                    encoder.engine()
                );
                continue;
            }
            eprintln!("outgrowing the pool on {}", encoder.engine());
            assert_eq!(encoder.out.capacity(), OPENING);
            // A picture that changes every frame, so the encoder has bits to
            // spend and a delta frame can outgrow a buffer as a keyframe of a
            // harsh scene does.
            let mut rgba = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
            let paint = |rgba: &mut [u8], frame: u64| {
                let mut state =
                    0x9e37_79b9_7f4a_7c15u64 ^ frame.wrapping_mul(0x2545_f491_4f6c_dd1d);
                for pixel in rgba.chunks_exact_mut(4) {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    pixel.copy_from_slice(&[
                        state as u8,
                        (state >> 8) as u8,
                        (state >> 16) as u8,
                        255,
                    ]);
                }
            };

            // The keyframe goes through the pool the encoder opened with, and
            // succeeds: it is the picture the pool may grow to hold.
            paint(&mut rgba, 0);
            let keyframe = encoder.encode(&rgba, 0, true).unwrap().unwrap();
            assert!(
                keyframe.keyframe,
                "{codec:?}: the first frame isn't a keyframe"
            );
            assert_eq!(encoder.submitted(), 1);
            let mut frames = vec![keyframe];

            // From here a frame meets the pool at its opening size: the case a
            // stream hits when the picture after a keyframe is bigger than the
            // buffer waiting for it.
            encoder.out = Arc::new(Pool::new((WIDTH * HEIGHT * 2 + 4096) as usize));
            assert_eq!(encoder.out.capacity(), OPENING);
            for frame in 1..FRAMES {
                paint(&mut rgba, frame);
                let pts = frame * 3000;
                let encoded = encoder
                    .encode(&rgba, pts, false)
                    .unwrap_or_else(|why| panic!("{codec:?} lost frame {frame}: {why:#}"))
                    .unwrap_or_else(|| panic!("{codec:?} made no frame for {frame}"));
                assert_eq!(
                    encoded.timestamp, pts as u32,
                    "{codec:?}: another frame's time"
                );
                frames.push(encoded);
                // One picture, one submission: the encoder's state moved on with
                // the first, so a retry would be a second picture referencing a
                // frame that was never sent.
                assert_eq!(
                    encoder.submitted() as u64,
                    frame + 1,
                    "{codec:?} submitted the picture at {pts} more than once"
                );
            }
            // Each frame after the keyframe is larger than the buffer the pool
            // had at its opening size, or nothing about growth was proved here.
            let grown = encoder.out.capacity();
            let smallest = frames[1..]
                .iter()
                .map(|frame| frame.data.len())
                .min()
                .unwrap();
            assert!(
                smallest > OPENING,
                "{codec:?}: a frame of {smallest} bytes fits the pool's opening buffer, so the \
                 pool never had to grow and nothing was proved"
            );
            assert!(
                grown >= smallest,
                "{codec:?}: a {smallest} byte frame in a {grown} byte pool"
            );

            // Every access unit decodes, in order: a frame encoded twice would
            // have referenced the copy the receiver never got.
            let mut decoder = Decoder::new(codec).unwrap();
            let mut shown = 0;
            for (at, frame) in frames.iter().enumerate() {
                let picture = decoder
                    .decode(&frame.data)
                    .unwrap_or_else(|why| panic!("{codec:?}: frame {at} doesn't decode: {why:#}"));
                if picture.is_some() {
                    shown += 1;
                }
            }
            assert_eq!(
                shown as u64, FRAMES,
                "{codec:?}: {shown} pictures from {FRAMES} frames"
            );
            // The copy's buffer is one frame's worth: as big as the largest frame
            // produced, not the sum of them, and no larger than the most a frame
            // of this size can need.
            let largest = frames.iter().map(|frame| frame.data.len()).max().unwrap();
            assert_eq!(
                encoder.staging(),
                largest,
                "{codec:?} keeps {} bytes for its frame to be copied out of",
                encoder.staging()
            );
            assert!(
                encoder.staging() <= (WIDTH * HEIGHT * 2 + 4096) as usize,
                "{codec:?} keeps {} bytes, more than a frame of this size can need",
                encoder.staging()
            );
        }
    }

    /// Thirty frames in a row, with three alive at once, stay within the
    /// pool's three buffers: the encoder allocates no more than the first
    /// three.
    #[test]
    fn encoding_thirty_frames_reuses_three_buffers() {
        let mut encoder = Encoder::new(Codec::Vp8, 64, 36, 30, 500, Effort::Light, false).unwrap();
        let mut rgba = vec![0u8; 64 * 36 * 4];
        let mut held = VecDeque::new();
        for frame in 0..30u64 {
            // The next frame is encoded before an old one is dropped, so this
            // is three alive at once: what the pool has to cover.
            if held.len() == 3 {
                held.pop_front();
            }
            for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
                pixel.copy_from_slice(&[(index as u64 + frame * 8) as u8, 90, 200, 255]);
            }
            if let Some(frame) = encoder.encode(&rgba, frame * 3000, frame == 0).unwrap() {
                held.push_back(frame);
            }
        }
        assert_eq!(held.len(), 3);
        assert!(
            encoder.created() <= 3,
            "the encoder allocated {} buffers over 30 frames",
            encoder.created()
        );
    }

    /// A frame crosses to the sending thread, so the buffer it owns has to.
    #[test]
    fn encoded_frames_are_send() {
        fn send<T: Send>() {}
        send::<Encoded>();
        send::<Frame>();
    }

    /// A fallback starts the new encoder's drop count at none: the count a
    /// stream warns about is the encoder's own, and one kept from the encoder
    /// that failed would read 0 until the replacement dropped more frames than
    /// its predecessor did.
    #[test]
    fn a_replaced_encoder_starts_its_drop_count_at_none() {
        let mut encoder =
            Encoder::new(Codec::H264, 64, 36, 30, 500, Effort::Light, false).unwrap();
        encoder.late = 7;
        let opened = open_software(Codec::H264, 64, 36, 30, 500, Effort::Light).unwrap();
        encoder.replace(opened);
        assert_eq!(encoder.late, 0);
    }

    #[test]
    fn encodes_vp8_keyframes_and_deltas() {
        let (frames, _) = encode_four(Codec::Vp8, Effort::Light, (64, 36), false);
        for (keyframe, data) in &frames {
            // A keyframe's tag has bit 0 clear, and its size follows the start code.
            assert_eq!(*keyframe, data[0] & 1 == 0);
            if *keyframe {
                assert_eq!(&data[3..6], &[0x9d, 0x01, 0x2a]);
                let size = |at: usize| u16::from_le_bytes([data[at], data[at + 1]]) & 0x3fff;
                assert_eq!((size(6), size(8)), (64, 36));
            }
        }
        let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys[..3], [true, false, true]);
    }

    /// The colour the encoders declare is the colour the pixels were
    /// converted with, and nothing else is: `abgr_to_i420` writes BT.601's
    /// matrix over its limited range, which is what h264.c's VUI (6, 6, 6,
    /// not full range) and hwenc.c's `AVCOL_SPC_SMPTE170M` and
    /// `AVCOL_RANGE_MPEG` say the stream is. Black and white land on the
    /// range's ends rather than on 0 and 255, and green's luma is 601's
    /// weights, not 709's. A converter that changed would fail this, and both
    /// declarations would have to move with it.
    #[test]
    fn the_declared_colour_is_the_colour_that_was_converted() {
        let (width, height) = (16u32, 16u32);
        let planes = |pixel: [u8; 4]| {
            let mut frame = vec![0u8; (width * height * 4) as usize];
            for at in frame.chunks_exact_mut(4) {
                at.copy_from_slice(&pixel);
            }
            let chroma = (width / 2 * (height / 2)) as usize;
            let (mut y, mut u, mut v) = (
                vec![0; (width * height) as usize],
                vec![0; chroma],
                vec![0; chroma],
            );
            yuv_helper::abgr_to_i420(
                &frame,
                width * 4,
                &mut y,
                width,
                &mut u,
                width / 2,
                &mut v,
                width / 2,
                width as i32,
                height as i32,
            );
            (y[0], u[0], v[0])
        };
        // Limited range: 16 at the bottom, 235 at the top, so a plane of
        // mid-grey sits below the middle rather than on it.
        assert_eq!(planes([0, 0, 0, 255]).0, 16);
        assert_eq!(planes([255, 255, 255, 255]).0, 235);
        assert!((124..=127).contains(&planes([128, 128, 128, 255]).0));
        // 601's matrix: green is 0.587 of the way up the range, where 709's
        // 0.715 would put it at 173.
        let (green, ..) = planes([0, 255, 0, 255]);
        assert!((143..=147).contains(&green), "green at Y {green}");
    }

    /// The report the worker logs as "no GPU encoder opened" names every
    /// encoder the list holds, including the ones passed over before they
    /// could be tried: a size no H264 encoder takes leaves a reason for each.
    /// Which reason VA-API gives depends on the build, and the box this was
    /// written on showed that plainly: FFmpeg for Windows has no h264_vaapi
    /// in it, so it is "not in this FFmpeg" there rather than the no-device
    /// reason a Linux build without a driver would give.
    #[cfg(hwenc)]
    #[test]
    fn a_backend_passed_over_says_why() {
        let mut report = [0u8; 512];
        // Nothing named but the picture: this build's own order of encoders and
        // the shipped tuning, which is what a machine whose adapters can't be
        // listed is given.
        let ask = OpenRequest {
            codec: 0,
            width: 8192,
            height: 8192,
            fps: 30,
            kbps: 500,
            tuning: std::ptr::null(),
            encoders: std::ptr::null(),
            preference: std::ptr::null(),
        };
        // SAFETY: plain values and a buffer of the size given, which the shim
        // ends with a zero.
        let raw = unsafe { yapfast_hwenc_new(&ask, report.as_mut_ptr().cast(), report.len()) };
        if !raw.is_null() {
            // SAFETY: raw came from the call above and is freed once.
            unsafe { yapfast_hwenc_free(raw) };
            panic!("an H264 encoder opened on 8192 by 8192, so nothing was passed over");
        }
        let report = c_text(&report);
        // Each name, and a reason after it rather than a bare mention.
        for name in ["h264_nvenc", "h264_amf", "h264_vaapi"] {
            assert!(
                report.contains(&format!("{name}: ")) || report.contains(&format!("{name} (")),
                "{name} is unmentioned in {report:?}"
            );
        }
    }

    /// Main profile H264 at `size` that decodes: parameter sets and an IDR
    /// first, a forced IDR third. Returns what the encoder was opened with.
    fn check_h264(effort: Effort, size: (u32, u32), hardware: bool) -> Configuration {
        let (frames, configuration) = encode_four(Codec::H264, effort, size, hardware);
        {
            let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
            assert_eq!(keys[..3], [true, false, true]);
            let units = |data| yapfast_discord::rtp::nal_units(data);
            let types =
                |data| -> Vec<u8> { units(data).iter().map(|unit| unit[0] & 0x1f).collect() };
            let first = types(&frames[0].1);
            assert!(
                first.starts_with(&[7, 8]) && first.contains(&5),
                "parameter sets, then an IDR slice: {first:?}"
            );
            // The sequence parameter set's profile_idc: Main, as Discord's
            // client sends.
            assert_eq!(units(&frames[0].1)[0][1], 77);
            assert!(!types(&frames[1].1).contains(&5), "a delta frame");
            let mut decoder = Decoder::new(Codec::H264).unwrap();
            let mut shown = 0;
            for (_, data) in &frames {
                if let Some(picture) = decoder.decode(data).unwrap() {
                    assert_eq!((picture.width, picture.height), size);
                    shown += 1;
                }
            }
            assert!(
                shown >= 3,
                "{shown} pictures from {} at {effort:?} effort",
                configuration.engine
            );
        }
        configuration
    }

    #[test]
    fn encodes_main_profile_h264_that_decodes() {
        for effort in [Effort::Light, Effort::High] {
            assert_eq!(check_h264(effort, (64, 36), false).engine, "x264");
        }
    }

    /// H265 on the GPU, where it has an encoder for it: parameter sets and
    /// an IDR first, a delta, then a forced IDR. Yapfast can't decode H265,
    /// so this reads the units' types.
    #[test]
    fn encodes_h265_on_the_gpu() {
        if Encoder::new(Codec::H265, 320, 180, 30, 500, Effort::Light, true).is_err() {
            eprintln!("no GPU encoder for H265 here");
            return;
        }
        assert!(Encoder::new(Codec::H265, 320, 180, 30, 500, Effort::Light, false).is_err());
        let (frames, configuration) = encode_four(Codec::H265, Effort::Light, (320, 180), true);
        eprintln!("encoded with {configuration:?}");
        let types = |data| -> Vec<u8> {
            yapfast_discord::rtp::nal_units(data)
                .iter()
                .map(|unit| (unit[0] >> 1) & 0x3f)
                .collect()
        };
        let idr = |types: &[u8]| types.iter().any(|kind| matches!(kind, 19 | 20));
        let first = types(&frames[0].1);
        assert!(
            first.starts_with(&[32, 33, 34]) && idr(&first),
            "VPS, SPS, PPS, IDR: {first:?}"
        );
        assert!(!idr(&types(&frames[1].1)), "a delta frame");
        assert!(idr(&types(&frames[2].1)), "the forced keyframe");
        let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys[..3], [true, false, true]);
    }

    /// AV1 on the GPU, where it has an encoder for it: a sequence header
    /// before the first frame and the forced keyframe, not the delta, and
    /// OBUs that packetize whole.
    #[test]
    fn encodes_av1_on_the_gpu() {
        if Encoder::new(Codec::Av1, 320, 180, 30, 500, Effort::Light, true).is_err() {
            eprintln!("no GPU encoder for AV1 here");
            return;
        }
        assert!(Encoder::new(Codec::Av1, 320, 180, 30, 500, Effort::Light, false).is_err());
        let (frames, configuration) = encode_four(Codec::Av1, Effort::Light, (320, 180), true);
        eprintln!("encoded with {configuration:?}");
        let sequence = |data| {
            yapfast_discord::rtp::obu_elements(data)
                .expect("OBUs within the frame")
                .first()
                .is_some_and(|element| (element[0] >> 3) & 0x0f == 1)
        };
        let keys: Vec<bool> = frames.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys[..3], [true, false, true]);
        assert!(sequence(&frames[0].1) && sequence(&frames[2].1));
        assert!(!sequence(&frames[1].1), "a delta frame");
    }

    /// The GPU's encoder where this machine has one, x264 where it hasn't:
    /// the same stream either way.
    #[test]
    fn encodes_h264_on_the_gpu_or_falls_back() {
        let configuration = check_h264(Effort::Light, (320, 180), true);
        eprintln!("encoded with {configuration:?}");
        // What the stream was tuned with is said at the open, and the keyframe
        // interval in it is the encoder's own answer rather than this side's
        // assumption. x264 has its settings in native/h264.c and reports none
        // of them, so it is held to claiming nothing instead.
        assert!(
            configuration.engine == "x264" || configuration.tuning.contains("keyframe every"),
            "{configuration:?}"
        );
    }

    /// An encoder on the processor says which effort it was asked for and
    /// claims nothing that nobody read back: the picture's settings are
    /// native/h264.c's, and this side never says what they came out as.
    #[test]
    fn a_software_encoder_reports_the_effort_it_was_asked_for() {
        let encoder = Encoder::new(Codec::H264, 64, 36, 30, 500, Effort::High, false).unwrap();
        assert_eq!(
            encoder.configuration(),
            &Configuration {
                engine: "x264".into(),
                asked: "High effort".into(),
                tuning: String::new(),
                settings_dropped: String::new(),
            }
        );
    }

    /// The shipped tuning is the one the stream was measured with, and the
    /// settings a driver may drop are still marked as ones it may drop.
    ///
    /// The list is native/hwenc.c's own, not a copy here, so this reads what
    /// the open applies: p4 at low-latency tuning, capped VBR, no frame
    /// allowed to wait and an IDR on demand. The optional ones are the three
    /// that only improve the picture, which is what makes a driver refusing
    /// one a note the stream warns about ("the GPU's encoder dropped optional
    /// settings") rather than a failed open; a required one marked optional
    /// would go missing in silence instead, and a new picture is a change
    /// nobody has run a comparison of, so it has to be a failing test before
    /// it is a shipped default.
    #[cfg(hwenc)]
    #[test]
    fn the_shipped_tuning_is_the_one_that_was_measured() {
        let tuning = Tuning::default();
        assert_eq!(tuning.name(), c"p4");
        let mut asked = [0u8; 192];
        // SAFETY: a name from the table above, and a buffer of the size given.
        let found = unsafe {
            yapfast_hwenc_nvenc_settings(
                tuning.name().as_ptr(),
                asked.as_mut_ptr().cast(),
                asked.len(),
            )
        };
        assert_eq!(found, 0);
        assert_eq!(
            c_text(&asked),
            "preset=p4(optional) tune=ll(required) rc=vbr(optional) \
             zerolatency=1(optional) delay=0(required) forced-idr=1(required)"
        );
        // Naming nothing asks for the same row, so an encoder opened without a
        // word from Rust is the tuning that was tested.
        let mut unnamed = [0u8; 192];
        // SAFETY: as above, with no name at all.
        let found = unsafe {
            yapfast_hwenc_nvenc_settings(
                std::ptr::null(),
                unnamed.as_mut_ptr().cast(),
                unnamed.len(),
            )
        };
        assert_eq!((found, c_text(&unnamed)), (0, c_text(&asked)));
        // A tuning nobody has is refused rather than quietly replaced by this
        // one: a name that stopped matching would otherwise encode a tuning
        // the stream never chose.
        let mut none = [0u8; 192];
        // SAFETY: as above, with a name no row has.
        let found = unsafe {
            yapfast_hwenc_nvenc_settings(c"p7".as_ptr(), none.as_mut_ptr().cast(), none.len())
        };
        assert_eq!(found, -1);
    }
    /// A shim's `yapfast_hwenc_notes` for a driver that dropped the preset,
    /// writing the names as native/hwenc.c does, and one for a driver that
    /// kept every setting and so leaves the buffer empty.
    #[cfg(hwenc)]
    unsafe extern "C" fn drops_preset(_: *mut Raw, notes: *mut std::ffi::c_char, size: usize) {
        let names = c"preset,tune";
        let bytes = names.to_bytes_with_nul();
        // SAFETY: the caller gives a buffer of `size` bytes, and the shim
        // writes what fits, ending it with a zero.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr().cast(), notes, bytes.len().min(size));
        }
    }
    #[cfg(hwenc)]
    unsafe extern "C" fn keeps_every_setting(
        _: *mut Raw,
        notes: *mut std::ffi::c_char,
        size: usize,
    ) {
        if size > 0 {
            // SAFETY: the caller gives a buffer of `size` bytes.
            unsafe { *notes = 0 };
        }
    }
    /// An encoder whose open read `notes` from a shim, as the GPU's open does
    /// with the shim's own: x264 here, so no driver has to refuse anything.
    #[cfg(hwenc)]
    fn encoder_with_notes(
        notes: unsafe extern "C" fn(*mut Raw, *mut std::ffi::c_char, usize),
    ) -> Encoder {
        let mut encoder = Encoder::new(Codec::H264, 64, 36, 30, 500, Effort::Light, false).unwrap();
        encoder.configuration.settings_dropped = read_notes(notes, std::ptr::null_mut());
        encoder
    }
    /// A setting the driver dropped is not only logged: the name the shim
    /// wrote at the open travels in the report the Stream tab shows, so a
    /// stream says what its picture isn't getting rather than leaving it in a
    /// log nobody reads.
    #[cfg(hwenc)]
    #[test]
    fn a_dropped_setting_reaches_the_stream_report() {
        let encoder = encoder_with_notes(drops_preset);
        assert_eq!(
            crate::stream_worker::encoder_report(&encoder).settings_dropped,
            "preset,tune"
        );
    }
    /// An encoder that kept every setting reports no names, so the Stream
    /// tab's row for them is absent rather than empty.
    #[cfg(hwenc)]
    #[test]
    fn an_encoder_that_dropped_nothing_reports_nothing() {
        let encoder = encoder_with_notes(keeps_every_setting);
        assert!(
            crate::stream_worker::encoder_report(&encoder)
                .settings_dropped
                .is_empty()
        );
    }
}
