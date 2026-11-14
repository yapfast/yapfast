//! `stream_worker`'s report.

use super::*;

// A report is one line of the worker's output, read once and matched once, so
// its biggest variant being much bigger than the rest costs nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Report {
    /// The worker opened the frames file.
    Ready,
    /// Frames are flowing, at this size after scaling.
    Sharing { width: u32, height: u32 },
    /// Sharing: the video reaches the stream server. Watching: the first
    /// picture is on screen.
    Streaming,
    /// Sharing: how the stream goes out, once a second while it's live.
    Sending(StreamStats),
    /// Sharing: a preview of the stream for Discord to show, a JPEG as a
    /// data URI. The first comes once a picture isn't black, then one every
    /// 5 minutes, as Discord's client sends them.
    Preview { thumbnail: String },
    /// The video file opened: how long it plays, when it says, its
    /// pictures' size, and whether its sound plays.
    Opened {
        duration_ms: Option<u64>,
        width: u32,
        height: u32,
        sound: bool,
    },
    /// The video played to its end, and waits there.
    Finished,
    /// A picture of the screen [`shareable`] listed by `id`, for
    /// [`Job::Glance`]: RGBA pixels, `width` by `height`.
    Screen {
        id: u64,
        width: u32,
        height: u32,
        #[serde(with = "super::base64_pixels")]
        rgba: Vec<u8>,
    },
    /// The job ended: without an error when it was told to stop.
    Ended { error: Option<String> },
}

impl Report {
    /// Whether a later one of these says everything this one would, so a
    /// queue with no room can drop it and still tell the truth. Only the
    /// statistics are like that: every other report is a change the UI acts
    /// on, which losing would leave it saying something that isn't so.
    pub(crate) fn replaceable(&self) -> bool {
        matches!(self, Report::Sending(_))
    }
}

/// What a share can say about itself that the picture can't: how its frames
/// reach the window, on which adapter each step ran, why it isn't on the path
/// it asked for, how long a frame waited, and what the machine's memory has it
/// give back.
#[derive(Default)]
pub(crate) struct Diagnosis {
    /// How the pictures reach the window, once one has: the worker's own
    /// texture when the window took one, a copy of the pixels otherwise.
    /// Unknown until a picture has gone one way or the other.
    pub(crate) presentation: Option<Presentation>,
    /// The adapter the card drew this share on, when it drew one: the capture
    /// and the encoder are the same device there, which is what makes a frame
    /// cost no copy.
    pub(crate) gpu_luid: Option<u64>,
    /// Why the share isn't on the path it asked for, as the code that fell
    /// back said it. One line, kept rather than grown.
    pub(crate) fallback: Option<String>,
    /// How long the last picture waited between the capture having it and the
    /// encoder being given it, in milliseconds.
    pub(crate) queue_delay_ms: Option<u32>,
    /// When the capture handed over the picture the encoder is about to read.
    pub(crate) captured_at: Option<Instant>,
    /// What a machine short of memory has the share give back.
    pub(crate) memory: crate::pressure::Policy,
}

/// What became of a picture the desktop captured, at the encoder.
#[cfg(target_os = "linux")]
pub(crate) enum Sent {
    /// The encoder made this of it. `frame` is `None` for a session that answers
    /// nothing for a picture — a hardware encoder's first frame often does — and
    /// the picture after it is what makes one.
    Frame {
        frame: Option<encode::Frame>,
        /// Why the picture was converted before the encoder saw it, when it was:
        /// the one copy this frame cost. `None` when the encoder read the
        /// descriptor where the compositor drew it, which is this path's point.
        converted: Option<encode::Refused>,
    },
    /// The card's own path couldn't take the picture — a tiled buffer the driver
    /// only gives back as an array, a picture on another device — in its
    /// driver's words. No driver that can't take one takes the next, so the path
    /// is given up for the rest of the stream: this picture, like the ones after
    /// it, goes the processor's way.
    #[cfg(nvcuda)]
    Card(String),
    /// Nothing here can send it: the picture isn't one the encoder takes where it
    /// lies, and it can't be read back on the processor either, which `why` says
    /// in the read's own words. A share of such pictures belongs on the capturer
    /// that has pixels.
    Unreadable(String),
}

/// The report's encoder side: the engine and codec it runs, the size it
/// sends, and the optional settings its driver dropped when it opened. Those
/// names are a short string the shim wrote once, so each report copies them
/// rather than growing with the frames.
pub(crate) fn encoder_report(encoder: &Encoder) -> StreamStats {
    let (width, height) = encoder.size();
    let (pool_live, pool_peak) = encoder.pool_bytes();
    StreamStats {
        engine: encoder.engine().to_owned(),
        codec: encoder.codec().name().to_owned(),
        width,
        height,
        settings_dropped: encoder.configuration().settings_dropped.clone(),
        pool_live: Some(pool_live),
        pool_peak: Some(pool_peak),
        ..StreamStats::default()
    }
}
