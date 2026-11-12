//! Encoding for Go Live and the camera, through native/vp8.c with the libvpx
//! inside libwebrtc, native/h264.c with x264 (vendor/x264-src), and
//! native/hwenc.c with FFmpeg's hardware encoders (vendor/ffmpeg-src) where
//! the GPU has one. Frames arrive as RGBA or I420 and leave as VP8 frames,
//! H264 or H265 access units in Annex B form, or AV1 temporal units of OBUs
//! that give their sizes.
//!
//! On Linux a frame can also arrive as a DMA-BUF, the picture still on the
//! card that made it: [`Encoder::encode_dmabuf`] wraps it into a VA-API surface
//! with FFmpeg's `av_hwframe_map` and the VA-API encoder reads that surface,
//! through native/hwenc_dmabuf.c. No pixel of such a picture comes back to the
//! processor. A picture the import can't take — the BGRA a compositor drew a
//! desktop in — is offered to the card's video processor instead, which scales
//! and converts it into the surface the encoder reads (`cfg(vaapivpp)`, and
//! [`vpp_filter`] is what the processor is asked for); every other encoder here
//! reads its frames from memory, says so, and is handed the picture through the
//! plane path instead.

use std::{
    ffi::{c_char, c_int, c_long, c_longlong, c_uchar, c_uint, c_ulong},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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

/// Whether video encodes on the GPU where it has an encoder: Settings' choice,
/// on until the UI says. One switch for a stream and for a call's camera, which
/// the same card encodes; an encoder that won't open falls back to the
/// processor by itself, so this is the person's answer and not the machine's.
static HARDWARE: AtomicBool = AtomicBool::new(true);

/// Encodes video started from now on with the GPU's encoder, or not.
pub fn set_hardware(on: bool) {
    HARDWARE.store(on, Ordering::Relaxed);
}

/// Whether video started from now on encodes on the GPU where it has an
/// encoder.
pub fn hardware() -> bool {
    HARDWARE.load(Ordering::Relaxed)
}

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
    /// Whether this machine has the card NVENC encodes on, which FFmpeg's own
    /// CUDA device answers: what a Linux share's DMA-BUF path asks before it
    /// opens one of FFmpeg's NVENC sessions to find out.
    #[cfg(target_os = "linux")]
    fn yapfast_hwenc_nvidia() -> c_int;
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
    #[cfg(target_os = "windows")]
    fn yapfast_hwenc_encode_texture(
        encoder: *mut Raw,
        texture: *mut std::ffi::c_void,
        subresource: c_uint,
        pts: c_longlong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    #[cfg(target_os = "windows")]
    fn yapfast_hwenc_shared(encoder: *mut Raw) -> c_int;
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

// The VA-API encoder that reads the card's own pictures, from
// native/hwenc_dmabuf.c: a DMA-BUF wrapped into a VA-API surface and encoded
// there, with no upload. It is a session of its own, on the device the encoder
// above opened on, with the same settings; see [`Encoder::encode_dmabuf`].
#[cfg(dmabufenc)]
unsafe extern "C" {
    #[allow(clippy::too_many_arguments)]
    fn yapfast_hwenc_dmabuf_new(
        codec: c_uint,
        width: c_uint,
        height: c_uint,
        fps: c_uint,
        kbps: c_uint,
        encoder: *const c_char,
        device: *const c_char,
        driver: *const c_char,
        report: *mut c_char,
        report_size: usize,
    ) -> *mut Raw;
    fn yapfast_hwenc_dmabuf_encode(
        encoder: *mut Raw,
        picture: *const DmabufPicture,
        pts: c_longlong,
        keyframe: c_int,
        is_key: *mut c_int,
    ) -> c_long;
    fn yapfast_hwenc_dmabuf_drain(encoder: *mut Raw, out: *mut c_uchar, capacity: usize) -> c_long;
    /// Whether the card's encoder's shim has a video processor at all — FFmpeg's
    /// filters and its `scale_vaapi` — which is a fact about the build, asked
    /// before a picture is looked at: a build without one never opens a session
    /// for a screen.
    #[cfg(vaapivpp)]
    fn yapfast_hwenc_dmabuf_converts() -> c_int;
    /// Encodes a picture the encoder can't read as it arrived, by the card's
    /// video processor making the NV12 surface it reads out of it first:
    /// mapped, scaled to the stream's size, converted, and handed over as that
    /// surface. `args` are the processor's own, from [`vpp_filter`].
    #[cfg(vaapivpp)]
    fn yapfast_hwenc_dmabuf_convert(
        encoder: *mut Raw,
        picture: *const DmabufPicture,
        pts: c_longlong,
        keyframe: c_int,
        is_key: *mut c_int,
        args: *const c_char,
    ) -> c_long;
    fn yapfast_hwenc_dmabuf_set_bitrate(encoder: *mut Raw, kbps: c_uint) -> c_int;
    fn yapfast_hwenc_dmabuf_late(encoder: *mut Raw) -> c_uint;
    fn yapfast_hwenc_dmabuf_config(encoder: *mut Raw, config: *mut c_char, config_size: usize);
    fn yapfast_hwenc_dmabuf_error(encoder: *const Raw) -> *const c_char;
    fn yapfast_hwenc_dmabuf_free(encoder: *mut Raw);
    /// The settings the encoder that reads the card's own pictures is opened
    /// with, for a test to hold them to native/hwenc.c's VA-API branch without a
    /// GPU in the shape of the question. Nothing else in this build asks: what
    /// the session took is its `config`, logged at the open.
    #[cfg(test)]
    fn yapfast_hwenc_dmabuf_settings(
        fps: c_uint,
        kbps: c_uint,
        out: *mut c_char,
        size: usize,
    ) -> c_int;
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
    /// The device a capture's surfaces live on — an `ID3D11Device` on Windows —
    /// which only Quick Sync's shared path reads. Null where the stream has no
    /// surfaces of its own to hand over, which leaves that path closed and the
    /// encoder reading memory as it always did.
    device: *mut std::ffi::c_void,
}

/// A picture in a DMA-BUF, as native/hwenc_dmabuf.c reads it: one DRM object
/// holding an NV12 picture, which is the shape a VA-API surface is made of
/// without a conversion. Laid out as its `struct yapfast_dmabuf_picture`.
#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DmabufPicture {
    width: c_int,
    height: c_int,
    /// The one descriptor the picture is in, and how much of it the picture
    /// needs: a descriptor carries no size of its own.
    fd: c_int,
    size: u64,
    /// The DRM modifier the driver laid the buffer out with.
    modifier: u64,
    /// The buffer's format, as DRM names it, and how many planes of it this
    /// names: NV12's two, or the one of a 32-bit picture.
    fourcc: u32,
    planes: u32,
    /// Where each plane starts in that buffer, and how many bytes apart its
    /// rows are.
    offset: [u32; 2],
    pitch: [u32; 2],
}

/// DRM's fourcc for NV12, as a descriptor names its format: the one picture a
/// VA-API surface can be made of without converting it, and so the one a
/// descriptor can be encoded in as it lies.
#[cfg(target_os = "linux")]
const NV12: u32 = u32::from_le_bytes(*b"NV12");

/// DRM's fourcc for the two orders a compositor draws a desktop in: `AR24`,
/// which is B, G, R and a byte nothing reads, and `AB24`, the same with red and
/// blue the other way round. They are `capture/linear.rs`'s `Order::fourcc`,
/// which is what a screen's descriptor carries, and neither is a VA-API surface:
/// they are what the card's video processor is asked to make one of.
#[cfg(target_os = "linux")]
const AR24: u32 = u32::from_le_bytes(*b"AR24");
#[cfg(target_os = "linux")]
const AB24: u32 = u32::from_le_bytes(*b"AB24");

/// Why a picture didn't go to the encoder where it lay, and so is a picture for
/// the plane path: the caller logs one line with [`Encoder::engine`] and hands
/// the same picture to [`Encoder::encode_i420`], which is the only path here
/// that moves pixels. Nothing is copied on the way to this answer — an encoder
/// with no path from a DMA-BUF is told so rather than handed one it would
/// upload.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The encoder that opened reads its pictures from the processor's memory:
    /// NVENC, Quick Sync, AMF and x264 have no path from a DMA-BUF to their
    /// frames, so taking one would be an upload.
    Uploads,
    /// A VA-API encoder, but this FFmpeg wasn't built with the device and the
    /// DRM PRIME import a descriptor is wrapped with (`--enable-vaapi` and
    /// `--enable-libdrm`), so nothing in this build can read one.
    NotBuilt,
    /// A VA-API encoder, but this picture isn't one a surface can be made of:
    /// the reason is this side's, read from the descriptor without asking the
    /// card.
    Unimportable(&'static str),
    /// The card's VA-API encoder wouldn't take the picture, which it said in
    /// the log when it tried.
    Failed,
    /// The encoder can't read the picture as it arrived, but the card's video
    /// processor can make the surface it reads out of it: the picture is scaled
    /// and converted to NV12 there, and the encoder reads that. It is the one
    /// conversion of a picture that costs the processor no pixel, which is what
    /// separates it from the copy [`Self::Uploads`] and [`Self::Unimportable`]
    /// send a picture to.
    Converted,
}

#[cfg(target_os = "linux")]
impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uploads => f.write_str("the encoder reads its pictures from memory"),
            Self::NotBuilt => f.write_str("this build has no VA-API path for a DMA-BUF"),
            Self::Unimportable(why) => f.write_str(why),
            Self::Failed => f.write_str("the card's encoder wouldn't take the picture"),
            Self::Converted => f.write_str(
                "the card's video processor scaled and converted it to NV12 before the encoder \
                 read it",
            ),
        }
    }
}

#[cfg(target_os = "linux")]
impl std::error::Error for Refused {}

/// Whether the encoder FFmpeg calls `name` reads a picture where it lies.
///
/// A VA-API encoder is handed a VA-API surface, which FFmpeg makes from a
/// DMA-BUF by importing the buffer rather than copying it; NVENC, Quick Sync,
/// AMF and x264 are handed frames in system memory, so a picture in a DMA-BUF
/// would have to be copied into one. The name decides it, so a stream on a GPU
/// without the path is told before its picture is looked at.
fn takes_dmabuf(name: &str) -> bool {
    name.ends_with("_vaapi")
}

#[cfg(all(target_os = "windows", hwenc))]
thread_local! {
    /// The capture's device, for the open that is happening now on this thread:
    /// a session reads a texture only on the device it was opened on.
    static CAPTURE_DEVICE: std::cell::RefCell<Option<*mut std::ffi::c_void>> =
        const { std::cell::RefCell::new(None) };
}

/// The encoder the card will open for `codec`, as the adapter preference picks
/// it: what decides whether a capture's own texture can reach the encoder at
/// all. Quick Sync and AMF read a Direct3D 11 surface through FFmpeg's own
/// encoders; NVENC has no such path there, and reads one through
/// `native/d3d11enc.c` instead.
#[cfg(target_os = "windows")]
pub fn planned(codec: Codec) -> Option<&'static str> {
    plan(codec, None).tried.first().map(|(_, name)| *name)
}

/// The render node and libva driver the encoder opened on, out of the report it
/// wrote: `"h264_vaapi (/dev/dri/renderD128, radeonsi)"`. `None` for a report
/// without a device in it — an encoder on the processor, or one opened where
/// the shim had no render node to name — which leaves the encoder that reads
/// the card's pictures to FFmpeg to place.
#[cfg(target_os = "linux")]
#[cfg_attr(not(dmabufenc), allow(dead_code))]
fn vaapi_device(report: &str) -> Option<(&str, &str)> {
    let (_, inside) = report.split_once(" (")?;
    let (path, driver) = inside.strip_suffix(')')?.split_once(", ")?;
    (!path.is_empty() && !driver.is_empty()).then_some((path, driver))
}

/// The glibc this process runs on, as libc names it: `"2.40"`.
#[cfg(target_os = "linux")]
fn glibc_here() -> String {
    // SAFETY: no arguments; libc's own version string, which lives as long as
    // the process does.
    let version = unsafe { libc::gnu_get_libc_version() };
    if version.is_null() {
        return String::new();
    }
    // SAFETY: a string libc keeps for the life of the process, ended by a zero.
    unsafe { std::ffi::CStr::from_ptr(version) }
        .to_string_lossy()
        .into_owned()
}

/// The glibc symbol version a driver's `dlopen` failure asked for, where that is
/// what the failure is: `"2.42"` out of `version \`GLIBC_2.42' not found`. A
/// driver built against a newer glibc than this process has can't be loaded
/// into it at all, and this is the size of the gap.
#[cfg(target_os = "linux")]
fn wanted_glibc(why: &str) -> Option<&str> {
    let rest = &why[why.find("GLIBC_")? + "GLIBC_".len()..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
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

/// How much work an encoder puts into each frame, which Settings' share menu
/// ticks as "Higher quality": x264's superfast preset, or its slower one, for
/// a sharper picture at about twice the CPU; libvpx's faster preset, or its
/// slower one; and NVENC's p4, or its slowest preset, p7, which costs the card
/// time a game would have had. VP8 takes no effort. See `Tuning`.
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
/// There is a row per [`Effort`]: the share menu's "Higher quality" box is the
/// one thing that chooses between them, so a stream that asks for the sharper
/// picture gets the slower preset on whichever encoder runs, and one that
/// doesn't spends the card's time on the game instead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tuning {
    /// p4 at low-latency tuning, capped VBR, no frame delay and an IDR on
    /// demand: where screen recordings and games landed.
    #[default]
    LowLatency,
    /// p7, the slowest preset, for the picture "Higher quality" asks for. The
    /// same tuning otherwise: p7 costs card time per frame, not a frame of
    /// delay.
    High,
}

impl Tuning {
    /// The row an encoder of this [`Effort`] is opened with: the card's encoder
    /// follows the same setting x264's does.
    fn for_effort(effort: Effort) -> Self {
        match effort {
            Effort::Light => Self::LowLatency,
            Effort::High => Self::High,
        }
    }

    /// The row's name, as native/hwenc.c reads it.
    #[cfg(hwenc)]
    fn name(self) -> &'static std::ffi::CStr {
        match self {
            Self::LowLatency => c"p4",
            Self::High => c"p7",
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
    /// The encoder that reads the card's own pictures, from
    /// native/hwenc_dmabuf.c: opened on the first DMA-BUF a stream hands over,
    /// on the device the encoder above opened on. `None` until then, and for
    /// every encoder with no such path.
    #[cfg(dmabufenc)]
    dmabuf: Option<Dmabuf>,
    /// FFmpeg's own name for the encoder that runs, such as "h264_vaapi", which
    /// is which backend it is without the device the report adds. Kept beside
    /// `tried` so that reopening for a picture in memory doesn't read the name
    /// of the encoder it is about to try.
    engine_name: String,
    /// How many pictures went to the encoder that reads the card's own without
    /// the processor moving a pixel: the DMA-BUF wrapped as a surface where it
    /// lay, and the surface the card's video processor made of one it couldn't.
    in_place: u64,
    /// Which encoder runs and how its pictures reach it, for the report while a
    /// stream's pictures are the card's own: empty until the first one is
    /// encoded, so the line is the open's own until there is something to add.
    in_place_engine: String,
    /// How many pictures the card's video processor converted before the encoder
    /// read them, which is the count of the ones not read where they lay. Zero on
    /// every build without the processor, and on every stream that has never
    /// handed one over.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    converted: u64,
    /// Whether this stream's pictures are still offered to the card's video
    /// processor: this build's answer while none has failed, and given up for the
    /// stream once one has — no driver that can't convert a picture converts the
    /// next.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    converts: bool,
    /// Whether this stream's pictures are still offered to the card's encoder
    /// where they lie: given up once one is refused, as `converts` is, so a
    /// driver that won't import one isn't asked again, and warned about, on
    /// every frame.
    #[cfg(all(target_os = "linux", dmabufenc))]
    imports: bool,
    /// The card's video processor's arguments for this stream's pictures, and the
    /// shape they were made for: `None` until a picture needs a conversion, and
    /// made again when the pictures' shape changes.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    vpp: Option<Vpp>,
    /// How many pictures the processor moved into the encoder's own memory,
    /// which is every picture the plane path hands over.
    moved: u64,
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

/// The VA-API encoder that reads the card's own pictures: a session with its
/// own surfaces, which FFmpeg imports a DMA-BUF into for each picture. It runs
/// beside the encoder that reads the processor's memory while both may be
/// needed, which is why a stream whose pictures are all the card's own gives
/// that one up ([`Encoder::give_up_planes`]).
#[cfg(dmabufenc)]
struct Dmabuf {
    raw: *mut Raw,
    /// Pictures this encoder answered late, dropped rather than sent under the
    /// next picture's time, as last read, so that a driver falling behind is
    /// warned about once for each.
    late: u32,
}

/// The card's video processor's arguments for a stream's pictures, and the shape
/// they were made for. A screen doesn't change size or format under a stream, so
/// the text is made once and kept rather than for each picture; it is C text
/// because that is how it crosses to native/hwenc_dmabuf.c.
#[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
struct Vpp {
    /// The arrived picture's format, width and height the arguments were made
    /// for: a picture of another shape is a graph of its own.
    from: (u32, u32, u32),
    /// The processor's own options, `scale_vaapi`'s.
    args: std::ffi::CString,
}

/// What the card's video processor takes of the pictures a descriptor can name:
/// how many planes the format has in the one buffer, and what it is called in a
/// log. `None` for a picture of a format no processor here converts, which is a
/// picture for the processor's path. native/hwenc_dmabuf.c's `source_format`
/// names the same list, so the two change together, as its VA-API settings are
/// held together.
#[cfg(all(target_os = "linux", vaapivpp))]
fn vpp_format(fourcc: u32) -> Option<(usize, &'static str)> {
    match fourcc {
        AR24 => Some((1, "BGRA")),
        AB24 => Some((1, "RGBA")),
        NV12 => Some((2, "NV12")),
        _ => None,
    }
}

/// The card's video processor's own arguments for making the surface the encoder
/// reads out of a picture `fourcc` names, for a stream sending `(width,
/// height)`: the size it scales to and the format it makes, and — for a picture
/// that arrives as 32-bit colour rather than as luma and chroma — the colour the
/// conversion makes of it, which is the BT.601 limited range that libyuv made of
/// the same picture and that the encoder declares it receives.
#[cfg(all(target_os = "linux", vaapivpp))]
fn vpp_filter(fourcc: u32, (width, height): (u32, u32)) -> String {
    let mut args = format!("w={width}:h={height}:format=nv12");
    if fourcc != NV12 {
        args.push_str(":colour_matrix=smpte170m:range=limited");
    }
    args
}

#[cfg(dmabufenc)]
impl Drop for Dmabuf {
    fn drop(&mut self) {
        // SAFETY: raw came from yapfast_hwenc_dmabuf_new and is freed once.
        unsafe { yapfast_hwenc_dmabuf_free(self.raw) };
    }
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

    /// An encoder opened on the device a capture fills its textures on, which is
    /// what lets Quick Sync and AMF read a frame where the capture drew it: a
    /// surface is only readable by the device it lives on, and only a session
    /// opened there can be handed one.
    ///
    /// The device travels with the open rather than with the encoder: the shim
    /// takes it once, in the request that opens the session, and keeps a
    /// reference for as long as that session lives.
    #[cfg(all(target_os = "windows", hwenc))]
    #[allow(clippy::too_many_arguments)]
    pub fn on_device(
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        kbps: u32,
        effort: Effort,
        hardware: bool,
        device: *mut std::ffi::c_void,
    ) -> Result<Self> {
        /// Clears the capture device however the open ends, a panic included: the
        /// device is only valid for the call it was set for.
        struct Carried;

        impl Drop for Carried {
            fn drop(&mut self) {
                CAPTURE_DEVICE.with(|slot| *slot.borrow_mut() = None);
            }
        }

        CAPTURE_DEVICE.with(|slot| *slot.borrow_mut() = Some(device));
        let _carried = Carried;
        Self::new(codec, width, height, fps, kbps, effort, hardware)
    }

    /// Whether the session that opened reads a capture's own surfaces rather
    /// than frames in memory: Quick Sync and AMF do, and NVENC's session here
    /// does not.
    #[cfg(all(target_os = "windows", hwenc))]
    pub fn took_surfaces(&self) -> bool {
        // SAFETY: raw is an open encoder, and the shim answers whether this
        // session reads a capture's surfaces or frames in memory.
        unsafe { yapfast_hwenc_shared(self.raw) != 0 }
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
            open_hardware(
                codec,
                width,
                height,
                fps,
                kbps,
                Tuning::for_effort(effort),
                capture,
                &[],
            )
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
                    // A driver built against a newer glibc than this build was
                    // made with can't be loaded into it, and its dlopen failure
                    // names the symbol version and nothing to do about it. NixOS
                    // is where this is met: the drivers in /run/opengl-driver
                    // are built from the channel the machine runs, and a build
                    // from a pinned flake older than that channel can't load
                    // them. Both versions are named, since which channel to
                    // build against is the reader's to pick.
                    #[cfg(target_os = "linux")]
                    if let Some(wanted) = wanted_glibc(&why) {
                        tracing::warn!(
                            wanted = wanted,
                            here = glibc_here().as_str(),
                            "a GPU driver here asks for a newer glibc than this build has, \
                             so it can't load at all: build against the machine's own \
                             nixpkgs (nix develop path:./nix --override-input nixpkgs \
                             <its channel>), or use the AppImage, which runs on the \
                             host's glibc"
                        );
                    }
                }
                open_software(codec, width, height, fps, kbps, effort)?
            }
        };
        let pixels = (width * height) as usize;
        let engine_name = opened.name.clone();
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
            #[cfg(dmabufenc)]
            dmabuf: None,
            engine_name,
            in_place: 0,
            in_place_engine: String::new(),
            #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
            converted: 0,
            #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
            // SAFETY: the shim answers whether it was built with the video
            // processor without touching an encoder or a device.
            converts: unsafe { yapfast_hwenc_dmabuf_converts() != 0 },
            #[cfg(all(target_os = "linux", dmabufenc))]
            imports: true,
            #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
            vpp: None,
            moved: 0,
            #[cfg(test)]
            submitted: 0,
            planes: Vec::new(),
            // Starts lean; a keyframe of noise is the most the pool can grow to.
            out: Arc::new(Pool::new(pixels * 2 + 4096)),
        })
    }

    /// Which encoder runs: "libvpx", "x264", or FFmpeg's name for a
    /// hardware one, such as "h264_nvenc". Once a picture has been encoded
    /// where it lay, the line says that too: the encoder and the device, and
    /// that its pictures are the card's own rather than uploaded to it.
    pub fn engine(&self) -> &str {
        if self.in_place > 0 {
            return &self.in_place_engine;
        }
        &self.configuration.engine
    }

    /// What this encoder was opened with, for a log or a measurement to quote
    /// beside its numbers.
    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    fn hardware(&self) -> bool {
        // The line [`Self::engine`] reports is not what decides this: it grows
        // a note about where the pictures come from, and an encoder's kind is
        // what it opened as.
        !matches!(self.configuration.engine.as_str(), "libvpx" | "x264")
    }

    /// FFmpeg's own name for the encoder that runs, such as "h264_vaapi":
    /// which backend it is, without the device the report adds beside it.
    fn ffmpeg_name(&self) -> &str {
        &self.engine_name
    }

    /// Whether this encoder reads a picture where the card holds it, rather than
    /// taking one uploaded to it: VA-API does, and NVENC, Quick Sync, AMF and x264
    /// do not. A caller holding a picture on the card asks before it hands one
    /// over, so that a camera filmed on the card is not encoded by one that
    /// uploads every frame it is given.
    pub fn reads_descriptors(&self) -> bool {
        takes_dmabuf(self.ffmpeg_name())
    }

    /// How many pictures the processor moved into the encoder's own memory
    /// since this encoder opened: every picture the plane path hands over, and
    /// none of the ones the card's encoder reads where they lie. A capture that
    /// started converting DMA-BUFs into pictures would say so here without any
    /// picture looking different.
    pub fn copied(&self) -> u64 {
        self.moved
    }

    /// How many pictures were made into the encoder's own frames on the card: no
    /// pixel of one of these came back to the processor. A picture is one of them
    /// whether the encoder read the DMA-BUF where it lay or the card's video
    /// processor made the surface of it first. Zero while every picture is a
    /// picture in memory, which is every stream on an encoder with no path from a
    /// descriptor, and every stream that has never handed one over.
    pub fn encoded_in_place(&self) -> u64 {
        self.in_place
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
        // SAFETY: raw came from its codec's constructor and is freed once; an
        // encoder given up for the card's own pictures has none.
        if !self.raw.is_null() {
            unsafe { (self.free)(self.raw) };
        }
        self.raw = opened.raw;
        self.encode = opened.encode;
        self.drain = opened.drain;
        self.staging = opened.staging;
        self.set_bitrate = opened.set_bitrate;
        self.free = opened.free;
        self.configuration = opened.configuration;
        self.engine_name = opened.name.clone();
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
        // SAFETY: raw came from this codec's constructor and is still live; an
        // encoder given up for the card's own pictures has nothing staged.
        if self.raw.is_null() {
            return 0;
        }
        unsafe { (self.staging)(self.raw) }
    }

    /// Aims for `kbps` from the next frame on, without a keyframe.
    pub fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        // SAFETY: raw came from this codec's constructor and is still live; an
        // encoder given up for the card's own pictures takes the rate when it
        // is opened again.
        if !self.raw.is_null() && unsafe { (self.set_bitrate)(self.raw, kbps) } != 0 {
            bail!("the {:?} encoder refused {kbps} kbit/s", self.codec);
        }
        // The encoder that reads the card's pictures is the one the frames come
        // from while it runs, so the new rate has to reach it as well.
        #[cfg(dmabufenc)]
        if let Some(dmabuf) = &self.dmabuf {
            // SAFETY: dmabuf is an open encoder.
            unsafe { yapfast_hwenc_dmabuf_set_bitrate(dmabuf.raw, kbps) };
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

    /// Encodes a picture the card already holds, in the DMA-BUF it arrived in:
    /// its time is `time_ms` into the stream, which is [`timestamp`]ed into
    /// [`CLOCK`] units, so a picture is encoded at one time whichever way it
    /// reached the encoder.
    ///
    /// A VA-API encoder wraps the descriptor into a VA-API surface with FFmpeg's
    /// `av_hwframe_map` and encodes that surface (native/hwenc_dmabuf.c), so no
    /// pixel of the picture comes back to the processor.
    ///
    /// A picture the encoder can't read as it arrived — the BGRA a compositor
    /// draws a desktop in, of the screen's size rather than the stream's — is not
    /// a picture for the processor either: where this build carries the card's
    /// video processor, it scales and converts the picture into the NV12 surface
    /// the encoder reads, on the card (native/hwenc_dmabuf.c), so no pixel of it
    /// comes back to the processor.
    ///
    /// Every other encoder here — NVENC, Quick Sync, AMF, x264 — reads its
    /// pictures from memory, and answers [`Refused`] rather than copying a
    /// DMA-BUF into one. Any outer `Err` is one line in the log and then the same
    /// picture through [`Self::encode_i420`], which is the fallback and the only
    /// path that moves pixels; nothing here copies a picture on the way to saying
    /// it can't take one. The inner result is the encode itself, for an encoder
    /// that took the picture and then failed.
    #[cfg(target_os = "linux")]
    pub fn encode_dmabuf(
        &mut self,
        picture: &crate::dmabuf::Descriptor<'_>,
        keyframe: bool,
        time_ms: u64,
    ) -> std::result::Result<Result<Option<Frame>>, Refused> {
        let at = timestamp(Duration::from_millis(time_ms));
        // Which encoder runs, and whether it reads a picture where it lies: the
        // name answers that, so a stream on NVENC or x264 is told before its
        // picture is looked at.
        if !takes_dmabuf(self.ffmpeg_name()) {
            return Err(Refused::Uploads);
        }
        let refused = match self.importable(picture) {
            Ok(picture) => return self.encode_in_place(&picture, at, keyframe),
            Err(refused) => refused,
        };
        // The one NV12 picture a surface is made of is not what a screen hands
        // over, so the card's video processor makes the surface instead: only
        // where this build has one, and only for a picture whose format it takes.
        // Everything else is the processor's copy, and `refused` is why.
        #[cfg(vaapivpp)]
        if let Some(picture) = self.convertible(picture) {
            return self.encode_converted(&picture, at, keyframe);
        }
        Err(refused)
    }

    /// Encodes the picture in `texture` — a capture's own D3D11 texture, from
    /// the device the session was opened on — where it lies: Quick Sync's shared
    /// path hands the surface straight to the encoder, so no pixel of the picture
    /// is converted or uploaded. What comes back is the access unit for the
    /// picture, or none when the encoder wants another first.
    ///
    /// Only a session that opened its shared path reads surfaces: one opened from
    /// memory answers -1, which is a picture for [`Self::encode_i420`] and the
    /// reason the caller falls back.
    #[cfg(all(target_os = "windows", hwenc))]
    pub fn encode_texture(
        &mut self,
        texture: *mut std::ffi::c_void,
        subresource: u32,
        pts: u64,
        keyframe: bool,
    ) -> Result<Option<Frame>> {
        let mut is_key: c_int = 0;
        // SAFETY: the texture is the capture's and lives through the call, and
        // the flag has a place of its own.
        let written = unsafe {
            yapfast_hwenc_encode_texture(
                self.raw,
                texture,
                subresource,
                pts as c_longlong,
                c_int::from(keyframe),
                &mut is_key,
            )
        };
        let (size, keyframe) = match written {
            ..0 => bail!(
                "{:?} wouldn't encode the capture's own surface; it goes through the processor \
                 instead",
                self.codec
            ),
            0 => return Ok(None),
            size => (size as usize, is_key != 0),
        };
        // The picture was encoded where it lay: a failure here is the copy of its
        // bitstream, and handing the picture to the processor now would encode it
        // twice, against a reference the receiver never got.
        let frame = self.collect(size, keyframe, pts, self.raw, yapfast_hwenc_drain)?;
        self.in_place += 1;
        Ok(Some(frame))
    }

    /// `picture`, as native/hwenc_dmabuf.c reads it, when the encoder can take it
    /// where it lies: one DRM object holding an NV12 picture of the encoder's
    /// size, which is the shape a VA-API surface is made of without a
    /// conversion. These checks are this side's, so a descriptor that can't be a
    /// surface is answered before the card is asked, and a picture the encoder
    /// would have to convert is never taken for one it can encode.
    #[cfg(target_os = "linux")]
    fn importable(
        &self,
        picture: &crate::dmabuf::Descriptor<'_>,
    ) -> Result<DmabufPicture, Refused> {
        if (picture.width, picture.height) != (self.width, self.height) {
            return Err(Refused::Unimportable(
                "a picture of another size than the encoder's",
            ));
        }
        if picture.fourcc != NV12 {
            return Err(Refused::Unimportable(
                "a picture that isn't NV12, which a VA-API surface is made of",
            ));
        }
        Self::described(picture, NV12, 2)
    }

    /// `picture` as the descriptor hands it to the card whichever way it goes
    /// there: the one DRM object it is in, the `planes` planes of `fourcc`'s
    /// format in that object, and how much of the buffer the picture needs. These
    /// checks are this side's, so a descriptor no import could name is answered
    /// before the card is asked, and both the encoder's import and the video
    /// processor's conversion read the same picture.
    #[cfg(target_os = "linux")]
    fn described(
        picture: &crate::dmabuf::Descriptor<'_>,
        fourcc: u32,
        planes: usize,
    ) -> Result<DmabufPicture, Refused> {
        if picture.planes.len() != planes || picture.fds.len() != planes {
            return Err(Refused::Unimportable(match planes {
                2 => "a picture that isn't NV12's two planes in one buffer",
                _ => "a picture that isn't one 32-bit plane in one buffer",
            }));
        }
        if picture.fds.iter().any(|fd| *fd < 0) {
            return Err(Refused::Unimportable("a plane with no buffer"));
        }
        // One DRM object is what FFmpeg imports as a surface; two buffers would
        // have to be one, and this won't pretend they are.
        if picture.fds.iter().any(|fd| *fd != picture.fds[0]) {
            return Err(Refused::Unimportable(
                "planes in two buffers, which one surface can't be made of",
            ));
        }
        let width = u64::from(picture.width);
        if picture
            .planes
            .iter()
            .any(|plane| u64::from(plane.stride) < width)
        {
            return Err(Refused::Unimportable("a pitch narrower than the picture"));
        }
        // Where the picture ends in the buffer, which is the size the import is
        // told: a descriptor carries the offset and pitch of each plane and no
        // size of its own, and a plane's last row has to fit. The planes after
        // the first are subsampled by two in each direction, which is NV12's
        // chroma.
        let (mut offset, mut pitch, mut size) = ([0; 2], [0; 2], 0u64);
        for (index, plane) in picture.planes.iter().enumerate() {
            offset[index] = plane.offset;
            pitch[index] = plane.stride;
            let rows = if index == 0 {
                u64::from(picture.height)
            } else {
                u64::from(picture.height).div_ceil(2)
            };
            size = size.max(u64::from(plane.offset) + u64::from(plane.stride) * rows);
        }
        Ok(DmabufPicture {
            width: picture.width as c_int,
            height: picture.height as c_int,
            fd: picture.fds[0],
            size,
            modifier: picture.modifier,
            fourcc,
            planes: planes as u32,
            offset,
            pitch,
        })
    }

    /// `picture` as the card's video processor reads it, when this build carries
    /// one and it takes the picture's format: `None` when there is no such path,
    /// and then the picture is the processor's — read back and converted with
    /// libyuv, which is the copy this one saves.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    fn convertible(&mut self, picture: &crate::dmabuf::Descriptor<'_>) -> Option<DmabufPicture> {
        let (planes, _) = vpp_format(picture.fourcc)?;
        let described = Self::described(picture, picture.fourcc, planes).ok()?;
        // Whether there is a processor to convert it on is this build's answer,
        // and whether this stream still offers it its pictures is a session's: a
        // build without one never opens a session for a screen, and a stream
        // whose conversion failed has stopped asking.
        if !self.converts {
            return None;
        }
        self.descriptor_encoder().ok()?;
        Some(described)
    }

    /// Encodes `picture` on the card by the card's video processor: the picture
    /// is mapped as a surface of the format it arrived in, scaled to the stream's
    /// size and converted to NV12 there, and the encoder is handed that surface,
    /// so no pixel of it comes back to the processor.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    fn encode_converted(
        &mut self,
        picture: &DmabufPicture,
        at: u64,
        keyframe: bool,
    ) -> std::result::Result<Result<Option<Frame>>, Refused> {
        let raw = self.descriptor_encoder()?;
        let args = self.vpp_args(picture);
        let mut is_key: c_int = 0;
        // SAFETY: raw is the descriptor encoder's and is open, the picture is one
        // of this encoder's planes, and the arguments outlive the call.
        let written = unsafe {
            yapfast_hwenc_dmabuf_convert(
                raw,
                picture,
                at as c_longlong,
                c_int::from(keyframe),
                &mut is_key,
                args.as_ptr(),
            )
        };
        let (size, keyframe) = match written {
            ..0 => {
                // The processor couldn't make the surface of this picture, so it
                // goes the processor's way — and the stream stops offering its
                // pictures, since a driver that can't convert one won't convert
                // the next.
                self.converts = false;
                tracing::warn!(
                    engine = self.engine(),
                    why = self.descriptor_error().as_str(),
                    "the card's video processor wouldn't make the picture's surface; it goes \
                     through the processor instead"
                );
                return Err(Refused::Failed);
            }
            0 => return Ok(Ok(None)),
            size => (size as usize, is_key != 0),
        };
        // The picture was encoded on the card, so a failure here is the copy of
        // its bitstream, not a picture for the fallback.
        let frame = match self.collect(size, keyframe, at, raw, yapfast_hwenc_dmabuf_drain) {
            Ok(frame) => frame,
            Err(error) => return Ok(Err(error)),
        };
        if self.converted == 0 {
            // Said once, as the processor's own copy is: which way a stream's
            // pictures are converted does not change under it. The engine's own
            // line grows the note, so a reader of the report sees it there too.
            tracing::info!(
                engine = self.configuration.engine.as_str(),
                why = %Refused::Converted,
                "converting on the card: the video processor scales every picture to the \
                 stream's size and turns it into NV12, and the encoder reads that surface"
            );
            self.in_place_engine = format!(
                "{}, scaling and converting DMA-BUFs on the card",
                self.configuration.engine
            );
        }
        self.converted += 1;
        self.note_dmabuf_late();
        self.in_place += 1;
        Ok(Ok(Some(frame)))
    }

    /// Encodes a picture that is on the card, where it lies, and gives the
    /// access unit it produced the time `at` in [`CLOCK`] units.
    #[cfg(all(target_os = "linux", dmabufenc))]
    fn encode_in_place(
        &mut self,
        picture: &DmabufPicture,
        at: u64,
        keyframe: bool,
    ) -> std::result::Result<Result<Option<Frame>>, Refused> {
        if !self.imports {
            return Err(Refused::Failed);
        }
        let raw = self.descriptor_encoder()?;
        let mut is_key: c_int = 0;
        // SAFETY: raw is the descriptor encoder's and is open, and the picture
        // is one of this encoder's size, mapped from a descriptor.
        let written = unsafe {
            yapfast_hwenc_dmabuf_encode(
                raw,
                picture,
                at as c_longlong,
                c_int::from(keyframe),
                &mut is_key,
            )
        };
        let (size, keyframe) = match written {
            ..0 => {
                self.imports = false;
                tracing::warn!(
                    engine = self.engine(),
                    why = self.descriptor_error().as_str(),
                    "the card's encoder wouldn't take the picture where it lies; it and the \
                     pictures after it go through the processor instead"
                );
                return Err(Refused::Failed);
            }
            0 => return Ok(Ok(None)),
            size => (size as usize, is_key != 0),
        };
        // The picture was encoded where it lay, so a failure here is the copy of
        // its bitstream, not a picture for the fallback: the encoder's state has
        // moved on and encoding it again would reference one nobody got.
        let frame = match self.collect(size, keyframe, at, raw, yapfast_hwenc_dmabuf_drain) {
            Ok(frame) => frame,
            Err(error) => return Ok(Err(error)),
        };
        self.note_dmabuf_late();
        self.in_place += 1;
        Ok(Ok(Some(frame)))
    }

    /// A VA-API encoder, but this build has nothing to wrap a descriptor with:
    /// FFmpeg wasn't configured for the device and the DRM PRIME import it
    /// needs, so no encoder in it can read one.
    #[cfg(all(target_os = "linux", not(dmabufenc)))]
    fn encode_in_place(
        &mut self,
        _: &DmabufPicture,
        _: u64,
        _: bool,
    ) -> std::result::Result<Result<Option<Frame>>, Refused> {
        Err(Refused::NotBuilt)
    }

    /// The encoder that reads the card's pictures, opened on the first one: the
    /// same codec on the same GPU as the encoder this stream opened, with the
    /// same settings, reading a surface rather than a frame in memory. Its
    /// reason for not opening is logged once, and every picture after it gets
    /// [`Refused::Failed`] without the card being asked again.
    #[cfg(all(target_os = "linux", dmabufenc))]
    fn descriptor_encoder(&mut self) -> Result<*mut Raw, Refused> {
        if let Some(dmabuf) = &self.dmabuf {
            return Ok(dmabuf.raw);
        }
        // An encoder a stream has already handed pictures in memory to may
        // still be given one, so a failure here changes nothing: it is one line
        // and the plane path.
        let Ok(name) = std::ffi::CString::new(self.ffmpeg_name()) else {
            return Err(Refused::Failed);
        };
        // The device the encoder opened on, so that the picture is imported on
        // the GPU the stream chose. A report with no device in it leaves FFmpeg
        // to place the session, which is what a machine whose adapters couldn't
        // be listed gets.
        let (path, driver) = vaapi_device(&self.configuration.engine).unzip();
        let device = path.and_then(|path| std::ffi::CString::new(path).ok());
        let driver = driver.and_then(|driver| std::ffi::CString::new(driver).ok());
        let (device, driver) = (
            device
                .as_ref()
                .map_or(std::ptr::null(), |text| text.as_ptr()),
            driver
                .as_ref()
                .map_or(std::ptr::null(), |text| text.as_ptr()),
        );
        let codec = match self.codec {
            Codec::H264 => 0,
            Codec::H265 => 1,
            Codec::Av1 => 2,
            Codec::Vp8 => return Err(Refused::Uploads),
        };
        let mut report = [0u8; 128];
        // SAFETY: the strings outlive the call, and the report is a buffer of the
        // size given, which the shim ends with a zero.
        let raw = unsafe {
            yapfast_hwenc_dmabuf_new(
                codec,
                self.width,
                self.height,
                self.fps,
                self.kbps,
                name.as_ptr(),
                device,
                driver,
                report.as_mut_ptr().cast(),
                report.len(),
            )
        };
        if raw.is_null() {
            tracing::warn!(
                engine = self.configuration.engine.as_str(),
                // SAFETY: a failed open leaves its reason in the shim's own
                // thread-local, which needs no encoder to read.
                why = self.descriptor_error().as_str(),
                "no encoder here reads the card's pictures where they lie; every picture goes \
                 through the processor"
            );
            return Err(Refused::Failed);
        }
        // Two sessions on the card are two hungers for its encode engines, so
        // the encoder this opened for pictures in memory is given up when it has
        // never encoded one: a stream whose pictures are the card's own runs one
        // session, and a picture that has to come through memory opens the other
        // again.
        if self.moved == 0 {
            self.give_up_planes();
        }
        let mut config = [0u8; 80];
        // SAFETY: raw is an open encoder, and the buffer is as large as its size.
        unsafe { yapfast_hwenc_dmabuf_config(raw, config.as_mut_ptr().cast(), config.len()) };
        let report = c_text(&report);
        self.in_place_engine = format!("{report}, encoding DMA-BUFs where they lie");
        tracing::info!(
            engine = self.in_place_engine.as_str(),
            config = c_text(&config),
            "encoding the card's own pictures: a DMA-BUF is wrapped as a VA-API surface for the \
             encoder to read, and no pixel of it is uploaded"
        );
        self.dmabuf = Some(Dmabuf { raw, late: 0 });
        Ok(raw)
    }

    /// The card's video processor's arguments for `picture`, made once for a
    /// shape and kept while it stays that: a stream's pictures arrive the same
    /// size and format, so a frame builds no string.
    #[cfg(all(target_os = "linux", dmabufenc, vaapivpp))]
    fn vpp_args(&mut self, picture: &DmabufPicture) -> &std::ffi::CStr {
        let from = (picture.fourcc, picture.width as u32, picture.height as u32);
        if self.vpp.as_ref().is_none_or(|vpp| vpp.from != from) {
            let args = vpp_filter(from.0, (self.width, self.height));
            // The text is this file's own, and has no zero in it.
            let args = std::ffi::CString::new(args).expect("the arguments carry no nul");
            self.vpp = Some(Vpp { from, args });
        }
        &self.vpp.as_ref().expect("made for this picture").args
    }

    /// The card's encoder's own last word about a picture, or about not opening
    /// at all, in words: what the log line beside a fallback says.
    #[cfg(all(target_os = "linux", dmabufenc))]
    fn descriptor_error(&self) -> String {
        let encoder = self
            .dmabuf
            .as_ref()
            .map_or(std::ptr::null_mut(), |dmabuf| dmabuf.raw);
        // SAFETY: an open encoder, or null, which the shim answers with the
        // reason its last open failed.
        let why = unsafe { yapfast_hwenc_dmabuf_error(encoder) };
        if why.is_null() {
            return String::new();
        }
        // SAFETY: the shim keeps the text alive until its next call.
        unsafe { std::ffi::CStr::from_ptr(why) }
            .to_string_lossy()
            .into_owned()
    }

    /// Gives up the encoder that reads pictures from the processor's memory,
    /// when the card's own path has taken the stream over: one session on the
    /// card while every picture arrives where it is encoded. The encoder given
    /// up is not one that failed, so it is the first this stream opens again if a
    /// picture has to come through memory.
    #[cfg(all(target_os = "linux", dmabufenc))]
    fn give_up_planes(&mut self) {
        if self.raw.is_null() {
            return;
        }
        // SAFETY: raw came from its codec's constructor and is freed once.
        unsafe { (self.free)(self.raw) };
        self.raw = std::ptr::null_mut();
        self.tried.pop();
    }

    /// Warns about pictures the card's encoder answered late, when one was: a
    /// driver falling behind, whose picture is dropped rather than sent under
    /// this one's time, as the plane path's own encoder is watched.
    #[cfg(all(target_os = "linux", dmabufenc))]
    fn note_dmabuf_late(&mut self) {
        let fresh = {
            let Some(dmabuf) = self.dmabuf.as_mut() else {
                return;
            };
            // SAFETY: dmabuf is an open encoder.
            let late = unsafe { yapfast_hwenc_dmabuf_late(dmabuf.raw) };
            let seen = std::mem::replace(&mut dmabuf.late, late);
            late.saturating_sub(seen)
        };
        if fresh == 0 {
            return;
        }
        tracing::warn!(
            engine = self.engine(),
            pictures = fresh,
            "the card's encoder answered pictures late; they were dropped"
        );
    }

    /// Encodes three planes, each with its stride, and copies the access unit
    /// the encoder produced into a buffer from the encoder's pool: the frame, or
    /// None when the encoder skipped it. The picture is not submitted again: an
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
        // An encoder given up for the card's own pictures is opened again here,
        // on the adapter and with the settings the open chose: a picture in
        // memory is the one case that path can't take.
        if self.raw.is_null() {
            self.reopen("a picture in memory after the card's own")?;
        }
        let written = self.encode_once_fallback(planes, pts, keyframe)?;
        self.note_late();
        let (size, keyframe) = match written {
            Written::Skipped => return Ok(None),
            Written::Frame(size, keyframe) => (size, keyframe),
        };
        Ok(Some(
            self.collect(size, keyframe, pts, self.raw, self.drain)?,
        ))
    }

    /// Copies an access unit `size` bytes long, which the encoder has put aside
    /// for its own copy of it, into a buffer from the encoder's pool: the frame,
    /// given the time it was encoded at. A unit larger than the buffers grows
    /// them, doubling until one holds it. This is the one copy of a frame's
    /// bytes between the encoder and the packetizer, and it happens once per
    /// picture: `encoder` and `drain` are the ones that produced it.
    fn collect(
        &mut self,
        size: usize,
        keyframe: bool,
        pts: u64,
        encoder: *mut Raw,
        drain: Drain,
    ) -> Result<Frame> {
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
        let spare = out.spare();
        // SAFETY: the buffer is at least the unit's size, which the pool was
        // grown to before this call, and both stay put during it.
        let copied = unsafe { drain(encoder, spare.as_mut_ptr(), spare.len()) };
        if copied <= 0 {
            bail!("the {:?} encoder lost the frame it produced", self.codec);
        }
        debug_assert_eq!(copied as usize, size, "the encoder gave back another frame");
        out.finish(copied as usize);
        Ok(Frame {
            data: out,
            keyframe,
            // The frame was encoded at this time, whatever the pool had to do
            // to hold it: one picture, one submission.
            timestamp: pts as u32,
        })
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
                self.reopen("the GPU's encoder failed")?;
                // The picture is submitted again, with a keyframe: the
                // receiver never got the one that failed, so the frame it
                // holds is one this stream's pictures no longer reference.
                self.encode_once(planes, pts, true)
            }
            written => written,
        }
    }

    /// Replaces the encoder that is no longer there with the next one that
    /// opens: another hardware encoder on this machine — the same codec, on
    /// another adapter or backend, which is what the adapter preference's
    /// fallbacks are for — or, when none is left, the processor's, which H264
    /// alone has. `reason` is what the log says happened, which is a failure
    /// for one and a picture in memory for another.
    ///
    /// The codec is kept rather than renegotiated: a stream's viewers were
    /// promised it, so a failure changes the encoder and not the format.
    fn reopen(&mut self, reason: &'static str) -> Result<()> {
        let (codec, width, height, fps, kbps, effort) = (
            self.codec,
            self.width,
            self.height,
            self.fps,
            self.kbps,
            self.effort,
        );
        if let Ok(opened) = open_hardware(
            codec,
            width,
            height,
            fps,
            kbps,
            Tuning::for_effort(effort),
            self.capture,
            &self.tried,
        ) {
            tracing::warn!(
                engine = opened.name.as_str(),
                fallback = true,
                reason,
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
    /// frame's time. An encoder on the processor has none to report, and
    /// neither has one given up for the card's own pictures.
    fn late(&self) -> u32 {
        #[cfg(hwenc)]
        {
            if !self.raw.is_null() && self.hardware() {
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
    /// produced stays in the encoder, for [`Self::collect`] to copy out: no
    /// output buffer crosses this boundary, so a frame too big for the one
    /// waiting at the other side is copied rather than encoded again.
    fn encode_once(
        &mut self,
        [(y, y_stride), (u, u_stride), (v, v_stride)]: [(&[u8], u32); 3],
        pts: u64,
        keyframe: bool,
    ) -> Result<Written> {
        let mut is_key: c_int = 0;
        // Every picture the plane path hands over is one the processor moves
        // into the encoder's own frame, whatever the encoder does with it
        // afterwards: this is the count a stream says moved pixels for.
        self.moved += 1;
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

/// FFmpeg's name for the session NVENC opens for `codec`: what the card's own
/// path (`stream_worker`'s `Cuda`, which talks to NVENC itself rather than
/// through an encoder FFmpeg opens) puts first in the line it reports, since
/// the Stream tab reads where the encoding runs from that name. `None` for a
/// codec NVENC has no session for, which that path never opens.
#[cfg(nvcuda)]
pub(crate) fn nvenc_name(codec: Codec) -> Option<&'static str> {
    engine_name(gpu::Backend::Nvenc, codec)
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

/// Whether this machine has the card NVENC encodes on, which is CUDA's and is
/// asked once. A Linux share's DMA-BUF path wants the card's own session on that
/// card — FFmpeg has no path from a descriptor into NVENC — so this says whether
/// there is one to open without opening FFmpeg's NVENC session to learn it, which
/// would be a session per codec for a picture it can't read. False where this
/// build has no hardware encoders, which is what makes the DMA-BUF path open
/// FFmpeg's own session the way every other machine does.
#[cfg(all(target_os = "linux", hwenc))]
pub fn nvidia_card() -> bool {
    // SAFETY: no arguments, and FFmpeg's own answer to a CUDA device.
    unsafe { yapfast_hwenc_nvidia() != 0 }
}

#[cfg(all(target_os = "linux", not(hwenc)))]
pub fn nvidia_card() -> bool {
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
        if plan.tried.iter().any(|(_, tried)| *tried == name) || plan.missing.contains(&name) {
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
        Some(
            std::ffi::CString::new(names.join(","))
                .map_err(|_| "a name with a zero in it".to_owned())?,
        )
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
        encoders: listed
            .as_ref()
            .map_or(std::ptr::null(), |names| names.as_ptr()),
        preference: preference.as_ptr(),
        // The capture's device, when this open is a share's own: Quick Sync and
        // AMF read the texture it filled, and a session without it reads frames
        // in memory as every encoder here used to.
        #[cfg(all(target_os = "windows", hwenc))]
        device: CAPTURE_DEVICE
            .with(|slot| slot.borrow_mut().take())
            .unwrap_or(std::ptr::null_mut()),
        #[cfg(not(all(target_os = "windows", hwenc)))]
        device: std::ptr::null_mut(),
    };
    // Four encoders' reasons have to fit: one GPU's driver refusal is a dlopen
    // failure whose text names the library and the symbol version it wanted, and
    // a report cut short of that is one nobody can act on.
    let mut report = [0u8; 1024];
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
    // on is the one the upload lands on. Every backend here is in that one
    // case, Quick Sync's too: it is handed NV12 from the processor, which
    // FFmpeg copies into the surfaces its session encodes from, so no stream
    // through this seam is zero-copy and none of them says it is.
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
    } else {
        // An encoder the shim's own order opened, which is what a platform that
        // can't list its adapters is given: the same upload, with no adapter
        // chosen for the line to name.
        tracing::info!(
            engine = report.as_str(),
            "encoding on the GPU's encoder from the processor: every frame is uploaded to it"
        );
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
        // SAFETY: raw came from its codec's constructor and is freed once; it is
        // null when the card's own pictures took the stream over and the encoder
        // on the processor's was given up.
        if !self.raw.is_null() {
            unsafe { (self.free)(self.raw) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Decoder;
    #[cfg(target_os = "linux")]
    use crate::dmabuf;
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
            // What the encoder keeps for a frame to be copied out of is one
            // frame's worth at most, not the sum of them: libvpx's frame is
            // gathered into a buffer as big as the largest, and x264's is
            // copied straight out of its own memory, so nothing is kept.
            let largest = frames.iter().map(|frame| frame.data.len()).max().unwrap();
            let kept = if codec == Codec::H264 { 0 } else { largest };
            assert_eq!(
                encoder.staging(),
                kept,
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
        let mut encoder = Encoder::new(Codec::H264, 64, 36, 30, 500, Effort::Light, false).unwrap();
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
    /// reason a Linux build without a driver would give. Quick Sync is named
    /// whether or not this build carries it — h264_qsv here, or "not in this
    /// FFmpeg" — so a machine with Intel's GPU is told which of the two it is
    /// rather than left out of the report.
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
            // No capture's device: this asks what a machine with none opens.
            device: std::ptr::null_mut(),
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
        for name in ["h264_nvenc", "h264_amf", "h264_qsv", "h264_vaapi"] {
            assert!(
                report.contains(&format!("{name}: ")) || report.contains(&format!("{name} (")),
                "{name} is unmentioned in {report:?}"
            );
        }
    }

    /// Quick Sync is asked for by FFmpeg's name for each codec, and not for
    /// VP8, which has no hardware encoder anywhere: a name that doesn't exist
    /// is one the probe would report as missing from the build on every Intel
    /// machine, so the mapping is pinned here rather than found from a log on
    /// hardware nobody has run this on.
    #[test]
    fn quick_sync_names_the_encoder_of_each_codec_it_encodes() {
        for (codec, name) in [
            (Codec::H264, "h264_qsv"),
            (Codec::H265, "hevc_qsv"),
            (Codec::Av1, "av1_qsv"),
        ] {
            assert_eq!(engine_name(gpu::Backend::Qsv, codec), Some(name));
        }
        assert_eq!(engine_name(gpu::Backend::Qsv, Codec::Vp8), None);
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

    /// Which encoders read a picture where it lies, by the name FFmpeg knows
    /// them by: VA-API's, and no other. NVENC, AMF and Quick Sync are handed
    /// frames in system memory, so a DMA-BUF given to one of them could only be
    /// copied into one; x264 and libvpx read the processor's memory too.
    ///
    /// The names come from `engine_name`, so this is the answer for every
    /// backend of every codec a stream can ask for, and a backend added to it
    /// without one here fails rather than quietly uploading.
    #[test]
    fn only_vaapi_reads_a_picture_where_it_lies() {
        for backend in [
            gpu::Backend::Nvenc,
            gpu::Backend::Amf,
            gpu::Backend::Qsv,
            gpu::Backend::Vaapi,
        ] {
            for codec in [Codec::H264, Codec::H265, Codec::Av1] {
                let name = engine_name(backend, codec).unwrap();
                assert_eq!(
                    takes_dmabuf(name),
                    backend == gpu::Backend::Vaapi,
                    "{name} is on {backend:?}, and its answer changed"
                );
            }
        }
        // The two encoders on the processor report themselves by these names.
        assert!(!takes_dmabuf("x264"));
        assert!(!takes_dmabuf("libvpx"));
        assert!(!takes_dmabuf(""));
    }

    /// An NV12 descriptor becomes one DRM object with two planes, which is what
    /// native/hwenc_dmabuf.c hands FFmpeg to import as a VA-API surface: the
    /// descriptor, the modifier, where each plane starts and how wide its rows
    /// are, and a buffer size that holds both planes. This is the mapping on its
    /// own, so it needs no card to prove it.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_descriptor_maps_to_one_nv12_object() {
        let encoder = Encoder::new(Codec::H264, 320, 180, 30, 500, Effort::Light, false).unwrap();
        let planes = [
            dmabuf::Plane {
                offset: 0,
                stride: 384,
            },
            dmabuf::Plane {
                offset: 73_728,
                stride: 384,
            },
        ];
        let fds = [7, 7];
        let picture = dmabuf::Descriptor {
            width: 320,
            height: 180,
            fourcc: NV12,
            modifier: 0x0100_0000_0000_0002,
            planes: &planes,
            colour: dmabuf::Colour::SDR,
            fds: &fds,
        };
        assert_eq!(
            encoder.importable(&picture),
            Ok(DmabufPicture {
                width: 320,
                height: 180,
                fd: 7,
                // The chroma plane's last row ends at 73,728 + 384 × 90, past
                // where the luma plane's does: that is how much of the buffer
                // the picture needs, and a descriptor carries no size to be
                // told instead.
                size: 108_288,
                modifier: 0x0100_0000_0000_0002,
                fourcc: NV12,
                planes: 2,
                offset: [0, 73_728],
                pitch: [384, 384],
            })
        );
    }

    /// Every descriptor that isn't one NV12 picture in one buffer is refused by
    /// name, rather than imported as a surface that would show something else:
    /// the BGRA a screen capture's DMA-BUF holds, a picture of another size, a
    /// pitch narrower than the picture, planes in two buffers, a plane with no
    /// buffer, and a shape that isn't NV12's two planes.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_picture_that_isnt_nv12_is_refused_by_name() {
        fn picture<'a>(
            fourcc: u32,
            (width, height): (u32, u32),
            planes: &'a [dmabuf::Plane],
            fds: &'a [c_int],
        ) -> dmabuf::Descriptor<'a> {
            dmabuf::Descriptor {
                width,
                height,
                fourcc,
                modifier: 0,
                planes,
                colour: dmabuf::Colour::SDR,
                fds,
            }
        }
        fn refuses(encoder: &Encoder, picture: &dmabuf::Descriptor<'_>, why: &'static str) {
            assert_eq!(encoder.importable(picture), Err(Refused::Unimportable(why)));
        }
        let encoder = Encoder::new(Codec::H264, 320, 180, 30, 500, Effort::Light, false).unwrap();
        let nv12 = [
            dmabuf::Plane {
                offset: 0,
                stride: 384,
            },
            dmabuf::Plane {
                offset: 73_728,
                stride: 384,
            },
        ];
        // A screen capture hands over BGRA, which a VA-API surface can't be made
        // of without converting it: that conversion is the copy this doesn't
        // make, so it is refused rather than done.
        let bgra = u32::from_le_bytes(*b"XR24");
        refuses(
            &encoder,
            &picture(bgra, (320, 180), &nv12, &[7, 7]),
            "a picture that isn't NV12, which a VA-API surface is made of",
        );
        refuses(
            &encoder,
            &picture(NV12, (320, 182), &nv12, &[7, 7]),
            "a picture of another size than the encoder's",
        );
        let narrow = [
            dmabuf::Plane {
                offset: 0,
                stride: 256,
            },
            nv12[1],
        ];
        refuses(
            &encoder,
            &picture(NV12, (320, 180), &narrow, &[7, 7]),
            "a pitch narrower than the picture",
        );
        refuses(
            &encoder,
            &picture(NV12, (320, 180), &nv12, &[7, 8]),
            "planes in two buffers, which one surface can't be made of",
        );
        refuses(
            &encoder,
            &picture(NV12, (320, 180), &nv12, &[-1, -1]),
            "a plane with no buffer",
        );
        let one = [nv12[0]];
        refuses(
            &encoder,
            &picture(NV12, (320, 180), &one, &[7]),
            "a picture that isn't NV12's two planes in one buffer",
        );
    }

    /// The render node and libva driver the stream's encoder opened on are read
    /// back out of the report native/hwenc.c wrote, so that the encoder reading
    /// the card's pictures is on the GPU the stream chose. A report with no
    /// device in it — an encoder on the processor, or a VA-API one whose shim
    /// named no node — leaves FFmpeg to place the session.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_device_the_encoder_opened_on_is_read_back() {
        assert_eq!(
            vaapi_device("h264_vaapi (/dev/dri/renderD128, radeonsi)"),
            Some(("/dev/dri/renderD128", "radeonsi"))
        );
        assert_eq!(
            vaapi_device("av1_vaapi (/dev/dri/renderD129, iHD)"),
            Some(("/dev/dri/renderD129", "iHD"))
        );
        for report in [
            // The encoders on the processor, and NVENC, which has no device to
            // name.
            "x264",
            "libvpx",
            "h264_nvenc",
            // A node with no driver named for it is not one to open.
            "h264_vaapi (/dev/dri/renderD128)",
            "h264_vaapi ()",
        ] {
            assert_eq!(vaapi_device(report), None, "{report:?}");
        }
    }

    /// A driver refused because it was built against a newer glibc says which
    /// one in its own words, and that version is read back out and logged
    /// beside this build's: on NixOS, the size of the gap is what says the
    /// machine's channel is newer than the flake's pin.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_driver_that_wants_a_newer_glibc_says_which() {
        let refusal = "av1_vaapi (renderD128, radeonsi): libva: dlopen of \
                       /run/opengl-driver/lib/dri/radeonsi_drv_video.so failed: \
                       /nix/store/x-glibc-2.40-224/lib/libm.so.6: version `GLIBC_2.42' not \
                       found (required by /run/opengl-driver/lib/dri/radeonsi_drv_video.so)";
        assert_eq!(wanted_glibc(refusal), Some("2.42"));
        // A refusal of any other kind has no gap to report.
        for refusal in [
            "av1_vaapi (renderD128, radeonsi): the driver said no",
            "h264_nvenc: no NVIDIA GPU here",
            "h264_vaapi: no GPU here with a VA-API driver",
        ] {
            assert_eq!(wanted_glibc(refusal), None, "{refusal:?}");
        }
        // This build's own, which is a version and not an empty string.
        let here = glibc_here();
        assert!(here.starts_with(char::is_numeric), "glibc {here}");
    }

    /// A DMA-BUF handed to an encoder on the processor is answered with a
    /// refusal rather than a copy: the caller logs one line and hands the same
    /// picture to the plane path, which is the fallback and the only path that
    /// moves pixels. The counters say which of the two happened.
    #[cfg(target_os = "linux")]
    #[test]
    fn an_encoder_on_the_processor_cannot_take_a_dmabuf() {
        let (width, height) = (320u32, 180u32);
        let mut encoder =
            Encoder::new(Codec::H264, width, height, 30, 500, Effort::Light, false).unwrap();
        assert_eq!(encoder.engine(), "x264");
        let planes = [
            dmabuf::Plane {
                offset: 0,
                stride: 384,
            },
            dmabuf::Plane {
                offset: 73_728,
                stride: 384,
            },
        ];
        let fds = [7, 7];
        let picture = dmabuf::Descriptor {
            width,
            height,
            fourcc: NV12,
            modifier: 0,
            planes: &planes,
            colour: dmabuf::Colour::SDR,
            fds: &fds,
        };
        assert!(matches!(
            encoder.encode_dmabuf(&picture, true, 0),
            Err(Refused::Uploads)
        ));
        // The refusal is an answer, not a picture: nothing was encoded, nothing
        // was moved, and nothing was encoded where it lay.
        assert_eq!((encoder.copied(), encoder.encoded_in_place()), (0, 0));
        assert_eq!(encoder.engine(), "x264", "the line reports what ran");
        // The same picture through the plane path is the fallback, and it is the
        // path the processor moves pixels for.
        let rgba = vec![0u8; (width * height * 4) as usize];
        let frame = encoder.encode(&rgba, 0, true).unwrap().unwrap();
        assert!(frame.keyframe);
        assert_eq!(frame.timestamp, 0);
        assert_eq!((encoder.copied(), encoder.encoded_in_place()), (1, 0));
    }

    /// A refusal says in words what it is, since that is the one log line a
    /// caller writes before falling back; nothing it names is a copy.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_refusal_says_which_it_is() {
        assert_eq!(
            Refused::Uploads.to_string(),
            "the encoder reads its pictures from memory"
        );
        assert_eq!(
            Refused::NotBuilt.to_string(),
            "this build has no VA-API path for a DMA-BUF"
        );
        assert_eq!(
            Refused::Unimportable("a pitch narrower than the picture").to_string(),
            "a pitch narrower than the picture"
        );
        assert_eq!(
            Refused::Failed.to_string(),
            "the card's encoder wouldn't take the picture"
        );
        assert_eq!(
            Refused::Converted.to_string(),
            "the card's video processor scaled and converted it to NV12 before the encoder read it"
        );
    }

    /// The card's video processor takes the two orders a compositor draws a
    /// desktop in, and NV12 itself, which it only scales: a picture of any other
    /// format is one no processor here converts, and so one for the processor's
    /// own copy.
    #[cfg(all(target_os = "linux", vaapivpp))]
    #[test]
    fn the_video_processor_takes_a_desktop_and_nv12() {
        assert_eq!(vpp_format(AR24), Some((1, "BGRA")));
        assert_eq!(vpp_format(AB24), Some((1, "RGBA")));
        assert_eq!(vpp_format(NV12), Some((2, "NV12")));
        // XRGB8888 is another order a compositor could draw a desktop in, and
        // P010 is a picture of a depth this build converts nothing from.
        for fourcc in [
            u32::from_le_bytes(*b"XR24"),
            u32::from_le_bytes(*b"P010"),
            0,
        ] {
            assert_eq!(vpp_format(fourcc), None, "{fourcc:#x}");
        }
    }

    /// The conversion is asked for the stream's size and NV12, and — only where
    /// the picture arrives as 32-bit colour — the BT.601 limited range that the
    /// processor's copy of the same picture was made with. The arguments are
    /// `scale_vaapi`'s own options, so the test reads them as the text the shim
    /// is handed.
    #[cfg(all(target_os = "linux", vaapivpp))]
    #[test]
    fn the_conversions_arguments_are_the_streams_size_and_nv12() {
        assert_eq!(
            vpp_filter(AR24, (1280, 720)),
            "w=1280:h=720:format=nv12:colour_matrix=smpte170m:range=limited"
        );
        assert_eq!(
            vpp_filter(AB24, (640, 360)),
            "w=640:h=360:format=nv12:colour_matrix=smpte170m:range=limited"
        );
        // NV12 is already luma and chroma: the processor scales it and leaves
        // its colour alone.
        assert_eq!(vpp_filter(NV12, (1280, 720)), "w=1280:h=720:format=nv12");
    }

    /// A screen's picture is one 32-bit plane of the compositor's buffer, which
    /// the encoder can't import where it lies and the video processor can take:
    /// the same descriptor that is refused by name is read as one plane of the
    /// screen's size for the conversion, which is what the processor scales to
    /// the stream's.
    #[cfg(all(target_os = "linux", vaapivpp))]
    #[test]
    fn a_screens_picture_is_the_video_processors_not_the_imports() {
        let encoder = Encoder::new(Codec::H264, 320, 180, 30, 500, Effort::Light, false).unwrap();
        let planes = [dmabuf::Plane {
            offset: 0,
            stride: 2560,
        }];
        let fds = [7];
        let picture = dmabuf::Descriptor {
            width: 640,
            height: 360,
            fourcc: AR24,
            modifier: 0,
            planes: &planes,
            colour: dmabuf::Colour::SDR,
            fds: &fds,
        };
        // The import takes one NV12 picture of the encoder's size and nothing
        // else: this is a screen's size, in a format no surface is made of.
        assert_eq!(
            encoder.importable(&picture),
            Err(Refused::Unimportable(
                "a picture of another size than the encoder's"
            ))
        );
        // The conversion reads the same descriptor, and its size is the
        // picture's own: the processor is what scales it.
        assert_eq!(
            Encoder::described(&picture, AR24, 1),
            Ok(DmabufPicture {
                width: 640,
                height: 360,
                fd: 7,
                // The one plane's last row ends at 2560 × 360.
                size: 921_600,
                modifier: 0,
                fourcc: AR24,
                planes: 1,
                offset: [0, 0],
                pitch: [2560, 0],
            })
        );
        // It is still one plane in one buffer: NV12's two are not there, so an
        // import of it would show something else.
        assert_eq!(
            Encoder::described(&picture, NV12, 2),
            Err(Refused::Unimportable(
                "a picture that isn't NV12's two planes in one buffer"
            ))
        );
    }

    /// The encoder that reads the card's own pictures is opened with the same
    /// settings as the one fed from the processor — Main profile H264, a keyframe
    /// every three seconds, no B-frames, capped VBR with the cap equal to the aim
    /// over half a second, one picture in flight — because a stream's picture
    /// must not depend on which way its frames reached the encoder.
    ///
    /// The list is native/hwenc_dmabuf.c's own, so this reads what its open
    /// applies rather than a copy here, and needs no GPU in the shape of the
    /// question. A setting that changed without a measurement behind it is a
    /// failing test rather than a picture nobody compared.
    #[cfg(dmabufenc)]
    #[test]
    fn the_descriptor_encoders_settings_are_the_plane_paths() {
        let mut asked = [0u8; 192];
        // SAFETY: plain values, and a buffer of the size given.
        let found = unsafe {
            yapfast_hwenc_dmabuf_settings(30, 2500, asked.as_mut_ptr().cast(), asked.len())
        };
        assert_eq!(found, 0);
        assert_eq!(
            c_text(&asked),
            "profile=main(required) gop=90(required) b_frames=0(required) \
             rc_mode=VBR(required) async_depth=1(required) rate=2500 kbit/s capped at 2500 \
             kbit/s over 500 ms(required)"
        );
    }
}

/// A picture VA-API decoded is encoded where it lies by VA-API's encoder: the
/// NV12 DMA-BUF the decoder exported is wrapped as a surface on the same card,
/// encoded, and decodes again to the stream it came from. The wrap is the one a
/// capture's NV12 picture takes, and gives FFmpeg the counted buffer its map
/// references. Needs a VA-API driver that decodes and encodes H264, on the node
/// `YAPFAST_VAAPI_NODE` names; says so rather than failing where there is none.
#[cfg(all(test, dmabufenc, vaapidec))]
mod in_place {
    use super::*;
    use crate::decode::{Decoder, vaapi::Hardware, vaapi_export::units};
    use std::ffi::CStr;

    #[test]
    fn a_decoded_picture_is_encoded_where_it_lies() {
        let Ok(node) = std::env::var("YAPFAST_VAAPI_NODE") else {
            eprintln!("YAPFAST_VAAPI_NODE names no VA-API node to test on");
            return;
        };
        let driver = std::env::var("LIBVA_DRIVER_NAME").unwrap_or_default();
        let node_c = std::ffi::CString::new(node.as_str()).unwrap();
        let mut decoder = match Hardware::on(Codec::H264, &node_c) {
            Ok(decoder) => decoder,
            Err(error) => {
                eprintln!("no VA-API decoder on {node}: {error:#}");
                return;
            }
        };
        let units = units();
        let (name, driver_c) = (c"h264_vaapi", std::ffi::CString::new(driver).unwrap());
        let mut encoder: *mut Raw = std::ptr::null_mut();
        let mut reference = Decoder::with_threads(Codec::H264, 1).unwrap();
        let mut again = Decoder::with_threads(Codec::H264, 1).unwrap();
        let (mut encoded, mut compared) = (0, 0);
        for (index, unit) in units.iter().enumerate() {
            // The stream as the software decoder reads it, for the comparison.
            let want = reference
                .decode(unit)
                .unwrap()
                .map(|picture| (picture.width, picture.height, mean(picture.i420().y)));
            if !decoder.decode(unit, index == 0).expect("a VA-API decode") {
                continue;
            }
            let descriptor = decoder.dmabuf().expect("the picture as a descriptor");
            let picture = Encoder::described(&descriptor, NV12, 2).expect("an NV12 picture");
            if encoder.is_null() {
                let mut report = [0u8; 128];
                // SAFETY: the strings outlive the call; the report is its size.
                encoder = unsafe {
                    yapfast_hwenc_dmabuf_new(
                        0,
                        descriptor.width,
                        descriptor.height,
                        30,
                        2_000,
                        name.as_ptr(),
                        node_c.as_ptr(),
                        driver_c.as_ptr(),
                        report.as_mut_ptr().cast(),
                        report.len(),
                    )
                };
                if encoder.is_null() {
                    // SAFETY: a failed open leaves its reason in the shim.
                    let why = unsafe { CStr::from_ptr(yapfast_hwenc_dmabuf_error(encoder)) };
                    eprintln!(
                        "no VA-API H264 encoder on {node}: {}",
                        why.to_string_lossy()
                    );
                    return;
                }
            }
            let mut is_key = 0;
            // SAFETY: the encoder is open; the picture names the decoder's
            // surface, which it holds until `release`.
            let size = unsafe {
                yapfast_hwenc_dmabuf_encode(
                    encoder,
                    &picture,
                    index as c_longlong * 3000,
                    c_int::from(encoded == 0),
                    &mut is_key,
                )
            };
            decoder.release();
            assert!(
                size >= 0,
                "the picture wasn't encoded where it lies: {}",
                unsafe { CStr::from_ptr(yapfast_hwenc_dmabuf_error(encoder)).to_string_lossy() }
            );
            if size == 0 {
                continue;
            }
            let mut unit = vec![0u8; size as usize];
            // SAFETY: the unit the encode above produced, into a buffer its size.
            let drained =
                unsafe { yapfast_hwenc_dmabuf_drain(encoder, unit.as_mut_ptr(), unit.len()) };
            assert_eq!(drained, size as c_long);
            encoded += 1;
            if let (Some(picture), Some((width, height, luma))) =
                (again.decode(&unit).unwrap(), want)
            {
                assert_eq!((picture.width, picture.height), (width, height));
                let ours = mean(picture.i420().y);
                assert!(
                    (ours - luma).abs() < 6.0,
                    "unit {index}: the picture encoded in place averages {ours:.1}, the stream {luma:.1}"
                );
                compared += 1;
            }
        }
        if !encoder.is_null() {
            // SAFETY: opened above, freed once.
            unsafe { yapfast_hwenc_dmabuf_free(encoder) };
        }
        eprintln!("{encoded} pictures encoded where they lay, {compared} compared");
        assert!(compared > 0, "no picture came back to compare");
    }

    fn mean(plane: &[u8]) -> f64 {
        plane.iter().map(|&value| f64::from(value)).sum::<f64>() / plane.len().max(1) as f64
    }

    /// The camera's two encoders against each other at a call's settings —
    /// 1280×720 at 30 frames a second and 1.5 Mbit/s, `camera.rs` and
    /// `discord_video.rs` — on the same moving picture: the bits each spends,
    /// what the picture it decodes to is worth, and the processor time a frame
    /// costs it.
    ///
    /// The source's pictures and each engine's access units go to files under
    /// the build directory, `target/tmp/camera-encoders`, and FFmpeg decodes
    /// them there and measures them against the source with VMAF and SSIM: the
    /// quality figures are the ones a stream is judged by, not the test's own
    /// reading of the encoder's output. A run leaves its streams there, so they
    /// can be looked at afterwards.
    ///
    /// Ignored, and it prints rather than asserts: which encoder is better is
    /// what it is asked, and it needs the machine's own encoders and FFmpeg.
    /// An engine that can't open here is skipped with a line that says so —
    /// the GPU's only opens outside the dev shell, whose library path has no
    /// `libcuda.so.1`.
    ///
    /// `cargo test --profile measure -p yapfast-core -- --ignored --nocapture the_camera_s_encoders`
    #[test]
    #[ignore = "measurement: compares the camera's encoders on this machine"]
    fn the_camera_s_encoders() {
        use std::io::Write as _;

        const WIDTH: u32 = crate::camera::WIDTH;
        const HEIGHT: u32 = crate::camera::HEIGHT;
        /// A call's camera bitrate, as `discord_video` sets it.
        const KBPS: u32 = 1500;
        const FRAMES: u32 = 60;
        /// What decodes each stream and measures it against the source.
        const FFMPEG: &str = "/usr/bin/ffmpeg";

        /// A picture of a room: a gradient that drifts, blocks that move, and a
        /// little grain, so it is neither a still nor a whole new picture every
        /// frame.
        struct Scene {
            y: Vec<u8>,
            u: Vec<u8>,
            v: Vec<u8>,
        }

        impl Scene {
            fn at(frame: u32) -> Self {
                let (width, height) = (WIDTH as usize, HEIGHT as usize);
                let mut state = 0x2545_f491_4f6c_dd1du64 ^ u64::from(frame);
                let mut noise = || {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    ((state >> 24) & 0xff) as i32 - 127
                };
                let mut y = Vec::with_capacity(width * height);
                for row in 0..height {
                    for column in 0..width {
                        let drift = (frame * 6) as usize % width;
                        let base = 40 + (column + drift) % width * 150 / width;
                        let shadow = (row + height / 3) % height;
                        let block = ((column + drift) / 80).is_multiple_of(4) && row > height / 4;
                        let value = base as i32 + if block && shadow > height / 2 { 40 } else { 0 };
                        y.push((value + noise() / 8).clamp(16, 235) as u8);
                    }
                }
                let chroma = width.div_ceil(2) * height.div_ceil(2);
                let mut u = Vec::with_capacity(chroma);
                let mut v = Vec::with_capacity(chroma);
                for index in 0..chroma {
                    let shift = ((index / 64 + frame as usize / 4) % 24) as i32;
                    u.push((128 + shift + noise() / 16) as u8);
                    v.push((128 - shift + noise() / 16) as u8);
                }
                Self { y, u, v }
            }

            fn picture(&self) -> crate::picture::I420<'_> {
                crate::picture::I420 {
                    width: WIDTH,
                    height: HEIGHT,
                    y: &self.y,
                    u: &self.u,
                    v: &self.v,
                    strides: [WIDTH, WIDTH / 2, WIDTH / 2],
                }
            }
        }

        let scenes: Vec<Scene> = (0..FRAMES).map(Scene::at).collect();

        // Two files side by side, so the measurement is FFmpeg's on a stream a
        // player would read: the source's pictures as they were made, and the
        // access units an engine made of them. In the build directory, where
        // the raw pictures are kept for a look rather than thrown away; a
        // library's own test gets no CARGO_TARGET_TMPDIR, so the crate's path
        // reaches the same place.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/tmp/camera-encoders");
        std::fs::create_dir_all(&dir).expect("a directory for the measurement");
        let source = dir.join("source.i420");
        {
            let mut file = std::io::BufWriter::new(
                std::fs::File::create(&source).expect("a file for the source"),
            );
            for scene in &scenes {
                for plane in [&scene.y, &scene.u, &scene.v] {
                    file.write_all(plane).expect("the source's pictures");
                }
            }
            file.flush().expect("the source's pictures");
        }
        eprintln!(
            "the source and each engine's stream are in {}",
            dir.display()
        );

        /// One engine's row of the table: its name, the bits it spent, what VMAF
        /// and SSIM made of its picture, and the processor time a frame cost it.
        type Row = (String, f64, Option<f64>, Option<f64>, f64);

        let mut rows: Vec<Row> = Vec::new();
        for hardware in [false, true] {
            let mut encoder = match Encoder::new(
                Codec::H264,
                WIDTH,
                HEIGHT,
                crate::camera::FPS,
                KBPS,
                Effort::Light,
                hardware,
            ) {
                Ok(encoder) => encoder,
                Err(why) => {
                    eprintln!("no H264 encoder here (hardware {hardware}): {why:#}");
                    continue;
                }
            };
            if hardware && !encoder.hardware() {
                eprintln!(
                    "no GPU encoder here: the open fell back to {}",
                    encoder.engine()
                );
                continue;
            }
            let engine = encoder.engine().to_owned();
            let configuration = encoder.configuration().clone();
            let stem: String = engine
                .chars()
                .map(|letter| {
                    if letter.is_ascii_alphanumeric() {
                        letter
                    } else {
                        '-'
                    }
                })
                .collect();
            let stream = dir.join(format!("{stem}.h264"));
            // The stream is held in memory while it is timed, so writing it
            // doesn't count as a frame's cost, and written out afterwards for
            // FFmpeg to read.
            let mut units = Vec::new();
            let (mut encoded, mut keyframes) = (0, 0);
            let start = std::time::Instant::now();
            for (index, scene) in scenes.iter().enumerate() {
                let picture = scene.picture();
                let pts = index as u64 * 1000 / u64::from(crate::camera::FPS);
                let Some(frame) = encoder
                    .encode_picture(&picture, pts, index == 0)
                    .unwrap_or_else(|why| panic!("{engine}: frame {index}: {why:#}"))
                else {
                    continue;
                };
                encoded += 1;
                keyframes += usize::from(frame.keyframe);
                units.extend_from_slice(&frame.data);
            }
            let elapsed = start.elapsed();
            std::fs::write(&stream, &units).expect("a file for the stream");
            if encoded == 0 {
                eprintln!("{engine}: no picture came out of it");
                continue;
            }
            let frames = f64::from(encoded);
            // Bits over the pictures actually taken, at the camera's rate.
            let kbit_s = units.len() as f64 * 8.0 / 1000.0 * f64::from(crate::camera::FPS) / frames;
            // What a viewer would make of the stream: VMAF for the eye, SSIM
            // for the picture, both measured by FFmpeg against the source.
            let measure =
                |filter: &str, mark: &str| match filter_score(&stream, &source, filter, mark) {
                    Ok(score) => Some(score),
                    Err(why) => {
                        eprintln!("{engine}: {filter} measured nothing: {why:#}");
                        None
                    }
                };
            let vmaf = measure("libvmaf", "VMAF score:");
            let ssim = measure("ssim", "All:");
            eprintln!(
                "{engine}: {encoded} pictures, {keyframes} keyframes, {} (dropped {}), written to {}",
                if configuration.tuning.is_empty() {
                    "no settings reported"
                } else {
                    configuration.tuning.as_str()
                },
                if configuration.settings_dropped.is_empty() {
                    "none"
                } else {
                    configuration.settings_dropped.as_str()
                },
                stream.display()
            );
            rows.push((
                engine,
                kbit_s,
                vmaf,
                ssim,
                elapsed.as_secs_f64() * 1000.0 / frames,
            ));
        }

        let shown = |value: Option<f64>, places: usize| match value {
            Some(value) => format!("{:.places$}", value),
            None => "-".to_owned(),
        };
        eprintln!();
        eprintln!(
            "{:<24} {:>9} {:>7} {:>7} {:>9}",
            "engine", "kbit/s", "VMAF", "SSIM", "ms/frame"
        );
        for (engine, kbit_s, vmaf, ssim, ms) in &rows {
            eprintln!(
                "{:<24} {:>9.0} {:>7} {:>7} {:>9.2}",
                engine,
                kbit_s,
                shown(*vmaf, 2),
                shown(*ssim, 4),
                ms
            );
        }
        if rows.is_empty() {
            eprintln!("no encoder opened here, so there is nothing to measure");
        }

        /// What FFmpeg's `filter` makes of `stream` against `source`, read from
        /// the line the filter prints. FFmpeg decodes the stream itself, so a
        /// picture that only looks cheap because it is broken shows here.
        fn filter_score(
            stream: &std::path::Path,
            source: &std::path::Path,
            filter: &str,
            mark: &str,
        ) -> Result<f64> {
            let arguments: Vec<String> = vec![
                "-hide_banner".to_owned(),
                "-nostats".to_owned(),
                "-i".to_owned(),
                stream.display().to_string(),
                "-f".to_owned(),
                "rawvideo".to_owned(),
                "-pix_fmt".to_owned(),
                "yuv420p".to_owned(),
                "-s".to_owned(),
                format!("{}x{}", crate::camera::WIDTH, crate::camera::HEIGHT),
                "-framerate".to_owned(),
                crate::camera::FPS.to_string(),
                "-i".to_owned(),
                source.display().to_string(),
                "-lavfi".to_owned(),
                filter.to_owned(),
                "-f".to_owned(),
                "null".to_owned(),
                "-".to_owned(),
            ];
            let ran = match std::process::Command::new(FFMPEG).args(&arguments).output() {
                Ok(ran) => ran,
                Err(why) => bail!("{FFMPEG} couldn't run: {why}"),
            };
            let printed = String::from_utf8_lossy(&ran.stderr);
            let Some((_, after)) = printed.rsplit_once(mark) else {
                let tail: Vec<&str> = printed.lines().rev().take(3).collect();
                bail!("{filter} printed no \"{mark}\": {tail:?}");
            };
            let number: String = after
                .trim_start()
                .chars()
                .take_while(|letter| letter.is_ascii_digit() || *letter == '.' || *letter == '-')
                .collect();
            match number.parse() {
                Ok(found) => Ok(found),
                Err(_) => bail!("{filter} printed \"{after:.40}\" rather than a number"),
            }
        }
    }
}
