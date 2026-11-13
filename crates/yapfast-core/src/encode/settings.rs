//! `encode.rs`'s settings.

use super::*;

/// Whether video encodes on the GPU where it has an encoder: Settings' choice,
/// on until the UI says. One switch for a stream and for a call's camera, which
/// the same card encodes; an encoder that won't open falls back to the
/// processor by itself, so this is the person's answer and not the machine's.
pub(crate) static HARDWARE: AtomicBool = AtomicBool::new(true);

/// Encodes video started from now on with the GPU's encoder, or not.
pub fn set_hardware(on: bool) {
    HARDWARE.store(on, Ordering::Relaxed);
}

/// Whether video started from now on encodes on the GPU where it has an
/// encoder.
pub fn hardware() -> bool {
    HARDWARE.load(Ordering::Relaxed)
}

/// How much work an encoder puts into each frame, which Settings' share menu
/// ticks as "Higher quality": x264's superfast preset, or its slower one, for
/// a sharper picture at about twice the CPU; libvpx's faster preset, or its
/// slower one. VP8 takes no effort, and neither does a GPU's encoder: its one
/// measured tuning is [`Tuning`]'s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    #[default]
    Light,
    High,
}

/// The tuning the stream asks a GPU's encoder for: how hard it works per
/// frame, how its bitrate cap is spent, and how often a viewer waits for a
/// keyframe. The set itself is `NVENC_TUNINGS` in native/hwenc.c and
/// native/d3d11enc.c, which know what each setting needs and what it costs;
/// this names one row of it.
///
/// There is one row, because the others were measured and bought nothing: p7
/// is 0.18 dB of PSNR-Y for 46 % more wall time on screen content, and
/// adaptive quantization is worse than none on it (docs/measurements/README.md).
/// So the card's picture is its bitrate and its codec, and "Higher quality"
/// stays the processor's to answer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tuning {
    /// p4 at low-latency tuning, capped VBR, no frame delay and an IDR on
    /// demand: where screen recordings and games landed, and what the
    /// measurements leave standing.
    #[default]
    LowLatency,
}

impl Tuning {
    /// The row's name, as native/hwenc.c and native/d3d11enc.c read it.
    #[cfg(hwenc)]
    pub(crate) fn name(self) -> &'static std::ffi::CStr {
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
