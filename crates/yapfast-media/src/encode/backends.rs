//! `encode.rs`'s backends.

use super::*;

#[repr(C)]
pub(crate) struct Raw {
    pub(crate) _private: [u8; 0],
}

pub(crate) type Encode = unsafe extern "C" fn(
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
pub(crate) type Drain = unsafe extern "C" fn(*mut Raw, *mut c_uchar, usize) -> c_long;
/// The bytes an encoder keeps for the unit it produces, reused each frame.
pub(crate) type Staging = unsafe extern "C" fn(*mut Raw) -> usize;
pub(crate) type SetBitrate = unsafe extern "C" fn(*mut Raw, c_uint) -> c_int;
pub(crate) type Free = unsafe extern "C" fn(*mut Raw);

/// What native/hwenc.c is asked for, laid out as its `struct yapfast_hwenc_open`.
#[cfg(hwenc)]
#[repr(C)]
pub(crate) struct OpenRequest {
    pub(crate) codec: c_uint,
    pub(crate) width: c_uint,
    pub(crate) height: c_uint,
    pub(crate) fps: c_uint,
    pub(crate) kbps: c_uint,
    /// The tuning asked for by name, or null for the shipped one.
    pub(crate) tuning: *const c_char,
    /// FFmpeg's encoders to try, best first and separated by commas: how the
    /// adapter preference reaches the device choice. Null for this build's own
    /// order, which is what a platform that can't list its adapters gets.
    pub(crate) encoders: *const c_char,
    /// "dedicated" or "integrated", for the render nodes VA-API chooses
    /// between itself; null when nothing is preferred.
    pub(crate) preference: *const c_char,
    /// The device a capture's surfaces live on — an `ID3D11Device` on Windows —
    /// which only Quick Sync's shared path reads. Null where the stream has no
    /// surfaces of its own to hand over, which leaves that path closed and the
    /// encoder reading memory as it always did.
    pub(crate) device: *mut std::ffi::c_void,
}

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
pub(crate) fn takes_dmabuf(name: &str) -> bool {
    name.ends_with("_vaapi")
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
pub(crate) fn vaapi_device(report: &str) -> Option<(&str, &str)> {
    let (_, inside) = report.split_once(" (")?;
    let (path, driver) = inside.strip_suffix(')')?.split_once(", ")?;
    (!path.is_empty() && !driver.is_empty()).then_some((path, driver))
}

/// The glibc this process runs on, as libc names it: `"2.40"`.
#[cfg(target_os = "linux")]
pub(crate) fn glibc_here() -> String {
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
pub(crate) fn wanted_glibc(why: &str) -> Option<&str> {
    let rest = &why[why.find("GLIBC_")? + "GLIBC_".len()..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// An opened encoder's handle, its functions, and what it was opened with.
pub(crate) struct Opened {
    pub(crate) raw: *mut Raw,
    pub(crate) encode: Encode,
    pub(crate) drain: Drain,
    pub(crate) staging: Staging,
    pub(crate) set_bitrate: SetBitrate,
    pub(crate) free: Free,
    pub(crate) configuration: Configuration,
    /// FFmpeg's own name for it ("h264_nvenc"), where
    /// [`Configuration::engine`] names VA-API's device as well.
    #[cfg_attr(not(hwenc), allow(dead_code))]
    pub(crate) name: String,
    /// The candidate that opened: which backend, on which adapter, and what a
    /// captured frame costs to reach it. `None` for an encoder on the
    /// processor, and for a hardware one opened where this machine's adapters
    /// couldn't be listed.
    #[cfg_attr(not(hwenc), allow(dead_code))]
    pub(crate) candidate: Option<Candidate>,
}

/// What the card's video processor takes of the pictures a descriptor can name:
/// how many planes the format has in the one buffer, and what it is called in a
/// log. `None` for a picture of a format no processor here converts, which is a
/// picture for the processor's path. native/hwenc_dmabuf.c's `source_format`
/// names the same list, so the two change together, as its VA-API settings are
/// held together.
#[cfg(all(target_os = "linux", vaapivpp))]
pub(crate) fn vpp_format(fourcc: u32) -> Option<(usize, &'static str)> {
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
pub(crate) fn vpp_filter(fourcc: u32, (width, height): (u32, u32)) -> String {
    let mut args = format!("w={width}:h={height}:format=nv12");
    if fourcc != NV12 {
        args.push_str(":colour_matrix=smpte170m:range=limited");
    }
    args
}

/// libvpx for VP8, x264 for H264.
pub(crate) fn open_software(
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
pub(crate) fn software_configuration(engine: &str, effort: Effort) -> Configuration {
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
pub(crate) fn engine_name(backend: gpu::Backend, codec: Codec) -> Option<&'static str> {
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
pub(crate) fn has(name: &str) -> bool {
    let Ok(name) = std::ffi::CString::new(name) else {
        return false;
    };
    // SAFETY: a name with a zero of its own, which the shim only asks FFmpeg
    // about.
    unsafe { yapfast_hwenc_has(name.as_ptr()) != 0 }
}

#[cfg(not(hwenc))]
pub(crate) fn has(_: &str) -> bool {
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
pub(crate) struct Plan {
    /// What to open, best first: the encoder on the adapter the preference
    /// picks, then the one on the adapter the capture filled its frames on,
    /// then the rest of the machine.
    pub(crate) tried: Vec<(Candidate, &'static str)>,
    /// The encoders that order wanted and this FFmpeg wasn't built with, by
    /// name: what a machine with Intel's GPU is told when the build has no
    /// Quick Sync.
    pub(crate) missing: Vec<&'static str>,
}

/// The encoders to try for `codec` and the ones this build hasn't got.
///
/// Empty when this machine's adapters couldn't be listed, which leaves
/// native/hwenc.c's own order — the encoder FFmpeg finds first — exactly as it
/// was before the adapter choice existed.
#[cfg(hwenc)]
pub(crate) fn plan(codec: Codec, capture: Option<Luid>) -> Plan {
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
pub(crate) fn open_hardware(
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
pub(crate) fn read_notes(
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
pub(crate) fn c_text(bytes: &[u8]) -> String {
    std::ffi::CStr::from_bytes_until_nul(bytes).map_or_else(
        |_| String::new(),
        |text| text.to_string_lossy().into_owned(),
    )
}

#[cfg(not(hwenc))]
#[allow(clippy::too_many_arguments)]
pub(crate) fn open_hardware(
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
