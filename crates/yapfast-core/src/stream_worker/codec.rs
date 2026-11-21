//! `stream_worker`'s codec.

use super::*;

/// The codecs only a GPU encodes, most wanted first, offered before H264
/// where the GPU's encoder for them opens.
pub(crate) const GPU_CODECS: [Codec; 2] = [Codec::Av1, Codec::H265];

/// Which codec a share asks Discord to send first: Settings, Advanced's
/// "Stream codec", still experimental. [`Automatic`](Self::Automatic) is the
/// order [`GPU_CODECS`] gives, most wanted first; a codec names the one to put
/// in front of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StreamCodec {
    /// AV1, then H265, then H264, as each is better than the one after it at
    /// the same bitrate and every machine has H264.
    #[default]
    Automatic,
    Av1,
    H265,
    H264,
}

impl StreamCodec {
    /// The setting's choices, in the order Settings lists them.
    pub const ALL: [Self; 4] = [Self::Automatic, Self::Av1, Self::H265, Self::H264];

    /// The codec to put first, or `None` for [`Automatic`](Self::Automatic).
    pub(crate) const fn wanted(self) -> Option<Codec> {
        match self {
            Self::Automatic => None,
            Self::Av1 => Some(Codec::Av1),
            Self::H265 => Some(Codec::H265),
            Self::H264 => Some(Codec::H264),
        }
    }

    /// How the setting's choice is written in the log.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Av1 => "av1",
            Self::H265 => "h265",
            Self::H264 => "h264",
        }
    }
}

/// `available` in the order a share offers it, with the setting's codec first
/// where this machine encodes it: Discord streams the first every viewer
/// decodes, so the codec the setting names is what goes out wherever a viewer
/// takes it, and the rest stay behind it as its fallbacks.
pub(crate) fn preferred(available: Vec<Codec>, choice: StreamCodec) -> Vec<Codec> {
    let Some(wanted) = choice.wanted().filter(|wanted| available.contains(wanted)) else {
        return available;
    };
    let mut offers = available;
    offers.retain(|codec| *codec != wanted);
    offers.insert(0, wanted);
    offers
}

pub(crate) fn log_encoder(encoder: &Encoding) {
    let (width, height) = encoder.size();
    tracing::info!(
        engine = encoder.engine(),
        codec = ?encoder.codec(),
        width,
        height,
        "encoding the stream"
    );
}

pub(crate) fn kbps(width: u32, height: u32, fps: u32) -> u32 {
    stream_connection::max_bitrate(width, height, fps) / 1000
}

/// The bits per pixel a frame a share declares when its job names none, which
/// is what the menu offers by default; see [`crate::voice::Ceiling`].
pub(crate) fn worth_a_pixel() -> f64 {
    crate::voice::Ceiling::default().bits_per_pixel()
}

/// The codec Settings' experimental choice asks a share to offer first, as an
/// index into [`StreamCodec::ALL`]. It travels with the job because the worker
/// is a process of its own, and it is the worker that offers codecs to Discord.
pub(crate) static STREAM_CODEC: AtomicU8 = AtomicU8::new(0);

/// Puts the codec the settings chose in front of the next share's offers; see
/// [`StreamCodec`]. A share already running offered its codecs when it
/// started, and keeps them.
pub fn set_stream_codec(choice: StreamCodec) {
    let index = StreamCodec::ALL
        .iter()
        .position(|one| *one == choice)
        .unwrap_or(0);
    STREAM_CODEC.store(index as u8, Ordering::Relaxed);
}

/// The codec the settings chose, as the last [`set_stream_codec`] said.
pub(crate) fn stream_codec() -> StreamCodec {
    StreamCodec::ALL
        .get(usize::from(STREAM_CODEC.load(Ordering::Relaxed)))
        .copied()
        .unwrap_or_default()
}

/// Encodes streams started from now on with the GPU's encoder, or not: the
/// switch is [`crate::encode`]'s, which a call's camera reads too.
pub fn set_hardware_encoding(on: bool) {
    crate::encode::set_hardware(on);
}
