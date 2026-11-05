//! Go Live runs in a worker process, `yapfast --stream-worker`, so screen
//! capture, video codecs and stream connections take memory only while
//! sharing or watching: the process exits when that stops and the memory
//! goes with it. Each worker shares this account's screen or shows one
//! other person's stream.
//!
//! Yapfast starts the worker with one [`Job`] as a JSON line on its standard
//! input, and reads [`Report`]s back from its standard output. Once Discord
//! has started the stream, a [`Control::Send`] line hands the worker the
//! stream's server, and it sends the video there; a [`Control::Switch`] line
//! shares another screen, window or camera on the same stream, and a
//! [`Control::Preview`] line says whether the UI shows the preview, which
//! the worker makes only while one does. A [`Control::Shown`] line says
//! whether the UI shows a watched stream's pictures, which a hidden or
//! minimised window doesn't, and none is converted or written while it
//! doesn't. A [`Control::Size`] line says how large the tile showing a
//! watched stream is, which the UI sends when that changes and on start, so
//! the stream is scaled to what shows it: a small tile costs a small frame,
//! and the frames file grows to hold a larger one. A `stop` line, or closing
//! its input, ends the job. Frames go to the UI through [`crate::frames`].

use std::{
    ffi::c_void,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc as channel,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use yapfast_discord::{
    rtp::Codec,
    stream::StreamServer,
    stream_connection::{self, Link, ReceivedFrame, StreamReceiver, StreamSender},
};

use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoBuffer},
};

#[cfg(wgc)]
use crate::{d3d11, wgc};
use crate::{
    decode::{Frame, Live},
    encode::{self, Effort, Encoder},
    frames::{Frames, Writer},
    picture::{self, I420, Tone},
    stream_audio,
    voice::{Resolution, StreamQuality, StreamStats},
};

/// The argument that starts Yapfast as a stream worker.
pub const ARGUMENT: &str = "--stream-worker";

/// The environment variable that tells a stream worker how many threads its
/// decoder may use: the share of the machine the Yapfast that starts it works
/// out from the machine's cores and how many streams it's decoding at once.
const THREADS: &str = "YAPFAST_STREAM_THREADS";

/// The largest frame shared at 1080p. Discord's own client trials 1080p at
/// 30 fps for accounts without Nitro, up to 5 Mbit/s (its "frontier tuning"
/// experiment); larger screens are scaled down to it.
pub const SHARE_WIDTH: u32 = 1920;
pub const SHARE_HEIGHT: u32 = 1080;
/// The largest preview of the shared screen the UI shows.
pub const PREVIEW_WIDTH: u32 = 1280;
pub const PREVIEW_HEIGHT: u32 = 720;
/// The largest picture shown of someone else's stream; larger ones are
/// scaled down to it.
pub const WATCH_WIDTH: u32 = 1920;
pub const WATCH_HEIGHT: u32 = 1080;
/// The picture a watched stream starts at, until the UI says how large the
/// tile showing it is: a tile of the call's five-up grid, so a stream shown
/// in one costs what it shows from its first frame. The UI's next tick lifts
/// it to its tile's size, and the frames file grows once to hold that.
const WATCH_TILE_WIDTH: u32 = 480;
const WATCH_TILE_HEIGHT: u32 = 270;
/// The largest preview Discord shows of a stream, and how often its client
/// sends a new one.
const THUMBNAIL_WIDTH: u32 = 512;
const THUMBNAIL_HEIGHT: u32 = 288;
const THUMBNAIL_EVERY: Duration = Duration::from_secs(5 * 60);
/// How long a shared stream whose connection ended waits for a server
/// Discord moved it to.
const MOVE_GRACE: Duration = Duration::from_secs(5);
/// How often a shared screen reports how it goes out, for the call's details.
const SENDING_EVERY: Duration = Duration::from_secs(1);
/// How long a stopping worker has to end by itself before it's ended. A
/// capture stuck in a system call is what takes this long; the stream's
/// threads and memory go with the process.
const STOP_WAIT: Duration = Duration::from_secs(2);
/// The most a preview is taken of a screen whose frames are on the card. A
/// preview is the only thing a share still needs in the processor's memory,
/// and it's a tile a person glances at: ten a second is smoother than the eye
/// reads a thumbnail, and the picture it replaces is a tenth of a second old
/// rather than a frame's.
#[cfg(wgc)]
const PREVIEW_EVERY: Duration = Duration::from_millis(100);
/// How many times a share puts the card's session and its capture back
/// together before it stops trying. A driver reset is worth rebuilding both
/// for; a card that answers every frame with the same failure is one to stop
/// sharing on, not to keep asking.
#[cfg(wgc)]
const CARD_RESTARTS: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Job {
    /// Captures `source`, writing frames that fit `width` by `height` to the
    /// frames file at `frames`.
    Share {
        source: Source,
        frames: PathBuf,
        width: u32,
        height: u32,
        fps: u32,
        /// How hard the encoder works on each frame.
        #[serde(default)]
        effort: Effort,
        /// Encoding on the GPU's encoder, where it has one.
        #[serde(default)]
        hardware: bool,
    },
    /// Receives and decodes the stream at `server`, writing pictures that
    /// fit `width` by `height` to the frames file at `frames`.
    Watch {
        server: StreamServer,
        frames: PathBuf,
        width: u32,
        height: u32,
        /// Where the stream's sound plays: the call's speakers, or the
        /// default output.
        #[serde(default)]
        speakers: Option<String>,
        /// Deafened in the call, so the stream's sound stays quiet.
        #[serde(default)]
        deafened: bool,
        /// How loud its sound plays; see [`crate::volume`].
        #[serde(default = "as_it_comes")]
        volume: f32,
    },
    /// Plays the video file at `file`, which may still be downloading,
    /// writing pictures that fit `width` by `height` to the frames file at
    /// `frames`; see [`crate::video_player`].
    Play {
        file: PathBuf,
        /// Its size, when known before it's downloaded.
        size: Option<u64>,
        frames: PathBuf,
        width: u32,
        height: u32,
        speakers: Option<String>,
        volume: f32,
    },
    /// Takes a picture of each screen in `screens`, by the ids [`shareable`]
    /// listed them with, scaled down to fit `width` by `height`, for the
    /// share menu to show. Each comes as a [`Report::Screen`].
    Glance {
        screens: Vec<u64>,
        width: u32,
        height: u32,
    },
}

impl Job {
    /// Whether a worker doing this decodes pictures, and so wants a share of
    /// the machine's decoder threads: a watched stream and a played video do,
    /// where a shared screen only encodes and a glance only takes a picture.
    fn decodes(&self) -> bool {
        matches!(self, Job::Watch { .. } | Job::Play { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A screen or window the person picks in the system's dialog, or what
    /// they picked from [`shareable`] where there's none.
    Screen,
    /// A screen by the id [`shareable`] listed it with.
    Display(u64),
    /// A window by the id [`shareable`] listed it with.
    Window(u64),
    /// A camera by its id in [`crate::camera::list`], such as OBS's virtual
    /// camera.
    Camera(String),
    /// Moving colour bars, for testing without a screen to capture.
    TestPattern,
}

/// Whether Yapfast lists screens and windows to share itself, where the system
/// has no dialog to pick one: everywhere but Linux.
pub const LISTS_SOURCES: bool = cfg!(not(target_os = "linux"));

/// A screen, window or camera [`shareable`] lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shareable {
    pub source: Source,
    pub title: String,
}

/// What Yapfast can share: the screens, then the windows, when
/// [`LISTS_SOURCES`], then the cameras. Yapfast's own windows aren't among
/// them, nor its console's.
pub fn shareable() -> Vec<Shareable> {
    let mut listed = screens_and_windows();
    listed.extend(crate::camera::list().into_iter().map(|device| Shareable {
        source: Source::Camera(device.id),
        title: device.name,
    }));
    listed
}

fn screens_and_windows() -> Vec<Shareable> {
    #[cfg(target_os = "linux")]
    {
        Vec::new()
    }
    #[cfg(not(target_os = "linux"))]
    {
        use livekit::webrtc::desktop_capturer::DesktopCaptureSourceType;
        let screens = sources(DesktopCaptureSourceType::Screen);
        let count = screens.len();
        let screens = screens
            .into_iter()
            .enumerate()
            .map(|(index, screen)| Shareable {
                source: Source::Display(screen.id()),
                title: if count == 1 {
                    "Entire screen".to_owned()
                } else {
                    format!("Screen {}", index + 1)
                },
            });
        // The console belongs to its host, so libwebrtc lists it as another
        // application's window.
        #[cfg(windows)]
        // SAFETY: no arguments; it only reads this process's console.
        let console = unsafe { windows::Win32::System::Console::GetConsoleWindow() }.0 as u64;
        #[cfg(not(windows))]
        let console = 0;
        let windows = sources(DesktopCaptureSourceType::Window)
            .into_iter()
            .filter_map(|window| {
                let title = window.title();
                (!title.trim().is_empty() && window.id() != console).then(|| Shareable {
                    source: Source::Window(window.id()),
                    title,
                })
            });
        screens.chain(windows).collect()
    }
}

#[cfg(not(target_os = "linux"))]
fn sources(
    kind: livekit::webrtc::desktop_capturer::DesktopCaptureSourceType,
) -> Vec<livekit::webrtc::desktop_capturer::CaptureSource> {
    livekit::webrtc::desktop_capturer::DesktopCapturer::new(
        livekit::webrtc::desktop_capturer::DesktopCapturerOptions::new(kind),
    )
    .map(|capturer| capturer.get_source_list())
    .unwrap_or_default()
}

/// What [`Source::Screen`] shares: the pick from [`shareable`] in the share
/// menu, until another. On Linux the pick may be [`Source::Screen`] itself,
/// which the system's dialog picks.
static PICKED: Mutex<Source> = Mutex::new(Source::Screen);

/// Shares `source` from now on where [`Source::Screen`] is asked for.
pub fn pick(source: Source) {
    *PICKED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = source;
}

/// `source`, with [`Source::Screen`] as what was picked.
fn picked(source: Source) -> Source {
    match source {
        Source::Screen => PICKED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone(),
        source => source,
    }
}

/// What Yapfast tells a running job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    /// Send the video to the server Discord started the stream on.
    Send(StreamServer),
    /// Share this screen or window in place of the one shared, once it gives
    /// a picture; on Linux, what the system's dialog picks. The stream goes
    /// on with the one shared until then, and when the dialog is cancelled.
    Switch(Source),
    /// Play a watched stream's sound, or not, as being deafened changes.
    Hear(bool),
    /// Show the shared screen in the UI's preview, or not, as the UI shows
    /// it or stops: a preview isn't made while nothing displays it.
    Preview(bool),
    /// Show a watched stream's pictures, or not, as a window can: a hidden or
    /// minimised one shows none, so none is converted or written into the
    /// shared frames while it can't, though the stream is decoded all the
    /// same. This says nothing about the sound, which plays on.
    Shown(bool),
    /// Play a watched stream's or a video's sound this loud.
    Volume(f32),
    /// Play the video, from the start once it has ended.
    Play,
    Pause,
    /// Move to this many milliseconds into the video.
    Seek(u64),
    /// Draw the video's pictures to fit this, and a watched stream's too:
    /// each is scaled to the size it shows at, never up to it. A watched
    /// stream is told it when the tile showing it changes size, and once
    /// when it starts showing.
    Size {
        width: u32,
        height: u32,
    },
    /// The video's file finished downloading, with this size; none when the
    /// download failed.
    Downloaded {
        size: Option<u64>,
    },
}

fn as_it_comes() -> f32 {
    1.0
}

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
        #[serde(with = "base64_pixels")]
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
    fn replaceable(&self) -> bool {
        matches!(self, Report::Sending(_))
    }
}

/// Runs this process as a stream worker until its job ends.
pub fn run() -> Result<()> {
    // The stream connection uses TLS; nothing else here has started it.
    let _ = crate::init_tls();
    // The share of the machine's decoder threads Yapfast gave this worker.
    // Started by hand, it decodes on the most one decoder asks for.
    if let Some(threads) =
        std::env::var_os(THREADS).and_then(|threads| threads.to_str()?.trim().parse().ok())
    {
        crate::decode::allow(threads);
    }
    let mut output = std::io::stdout();
    serve(Box::new(BufReader::new(std::io::stdin())), &mut |report| {
        // Yapfast is gone if this fails; the job ends when its input closes.
        let _ = writeln!(
            output,
            "{}",
            serde_json::to_string(&report).unwrap_or_default()
        );
        let _ = output.flush();
    });
    Ok(())
}

fn serve(mut input: Box<dyn BufRead + Send>, report: &mut dyn FnMut(Report)) {
    let mut line = String::new();
    let job = match input.read_line(&mut line) {
        Ok(_) => {
            serde_json::from_str::<Job>(&line).map_err(|error| anyhow!("unknown job: {error}"))
        }
        Err(error) => Err(error.into()),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let stopper = stop.clone();
    let (controls, received) = channel::channel();
    // Reading stdin blocks, so a thread waits for controls, the stop line or the end.
    std::thread::spawn(move || {
        for line in input.lines() {
            let Ok(line) = line else { break };
            if line.trim() == "stop" {
                break;
            }
            match serde_json::from_str::<Control>(&line) {
                Ok(control) => {
                    let _ = controls.send(control);
                }
                Err(error) => tracing::warn!(%error, "unknown stream worker control"),
            }
        }
        stopper.store(true, Ordering::Relaxed);
    });
    let result = job.and_then(|job| match job {
        Job::Share {
            source,
            frames,
            width,
            height,
            fps,
            effort,
            hardware,
        } => {
            let fps = fps.clamp(1, 60);
            let sink = Sink {
                writer: Writer::open(&frames)?,
                preview: vec![],
                preview_shown: false,
                controls: received,
                server: None,
                sound: source != Source::TestPattern,
                video: None,
                fps,
                effort,
                hardware,
                // Changed by the capture before it starts, which is the only
                // thing that decides it: nothing here knows yet which screen or
                // window is wanted, or what can capture it.
                footage: Footage::InMemory,
                started: Instant::now(),
                streaming: false,
                preview_due: None,
                thumbnail: None,
                lost: None,
            };
            report(Report::Ready);
            match source {
                Source::TestPattern => test_pattern(sink, width, height, &stop, report),
                source => capture_screen(source, sink, width, height, &stop, report),
            }
        }
        Job::Watch {
            server,
            frames,
            width,
            height,
            speakers,
            deafened,
            volume,
        } => {
            let writer = Writer::open(&frames)?;
            report(Report::Ready);
            let hearing = Arc::new(AtomicBool::new(!deafened));
            let sound = Sound {
                speakers,
                hearing,
                volume: Arc::new(stream_audio::Volume::new(volume)),
                controls: received,
            };
            watch(server, writer, (width, height), sound, &stop, report)
        }
        Job::Play {
            file,
            size,
            frames,
            width,
            height,
            speakers,
            volume,
        } => {
            let writer = Writer::open(&frames)?;
            report(Report::Ready);
            let job = crate::video_player::Playing {
                path: file,
                size,
                width,
                height,
                sound: true,
                speakers,
                volume,
            };
            crate::video_player::play(job, writer, received, &stop, report)
        }
        Job::Glance {
            screens,
            width,
            height,
        } => {
            report(Report::Ready);
            glance(&screens, width, height, &stop, report);
            Ok(())
        }
    });
    report(Report::Ended {
        error: result.err().map(|error| format!("{error:#}")),
    });
}

/// Where each frame goes: the UI's preview, then Discord once the stream
/// has a server.
struct Sink {
    writer: Writer,
    /// The picture the preview is scaled into, kept from one frame to the
    /// next: at most [`PREVIEW_WIDTH`] by [`PREVIEW_HEIGHT`].
    preview: Vec<u8>,
    /// Whether the UI shows the preview, which it says as that changes: one
    /// isn't made while nothing displays it.
    preview_shown: bool,
    controls: channel::Receiver<Control>,
    /// The server to send to from the next frame, which gives the stream its size.
    server: Option<StreamServer>,
    /// Whether to share other applications' sound too: not for the test
    /// pattern.
    sound: bool,
    video: Option<Video>,
    fps: u32,
    effort: Effort,
    hardware: bool,
    /// Where this share's pictures come from, which decides what encodes
    /// them. Chosen once, before the capture starts: see `capture_screen`.
    footage: Footage,
    started: Instant,
    streaming: bool,
    /// When the stream's next preview is due: from when it starts.
    preview_due: Option<Instant>,
    /// A preview made but not reported yet.
    thumbnail: Option<String>,
    /// When the stream connection ended, while Discord may still name a
    /// server it moved the stream to.
    lost: Option<Instant>,
}

/// Where a share's pictures come from, which is what decides who encodes
/// them. It is chosen once, before the capture starts, and never changes
/// under a stream: a capture that has to change its size rebuilds its
/// encoder, but not its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Footage {
    /// A screen, window or camera libwebrtc captured and the processor
    /// converted to I420: a picture in memory, which FFmpeg's encoder reads
    /// from there.
    InMemory,
    /// A screen Windows Graphics Capture filled a texture with, which NVENC
    /// reads on the card that shows it. Nothing of a frame crosses the
    /// processor on its way to the encoder.
    #[cfg(wgc)]
    OnTheCard,
}

/// What turns a frame into a packet.
///
/// The two are the same stream from the packetizer's side: the bytes, their
/// time on the stream clock and whether the frame is whole all come out of
/// [`Encoding::encode_i420`] or [`d3d11::D3d11Encoder::encode_texture`] in
/// [`encode::Frame`], and go on to the sender as one shape.
enum Encoding {
    /// FFmpeg's encoder on the processor — x264, libvpx, or one of FFmpeg's
    /// hardware encoders — reading an I420 picture the capture converted.
    Picture(Encoder),
    /// NVENC on the card, reading the capture's texture where the frame
    /// already is. The one copy a frame still costs is its bitstream out of
    /// the driver, which [`d3d11::D3d11Encoder::copied`] says is the only one.
    #[cfg(wgc)]
    Texture(d3d11::D3d11Encoder),
}

impl Encoding {
    fn codec(&self) -> Codec {
        match self {
            Self::Picture(encoder) => encoder.codec(),
            #[cfg(wgc)]
            Self::Texture(encoder) => encoder.codec(),
        }
    }

    fn size(&self) -> (u32, u32) {
        match self {
            Self::Picture(encoder) => encoder.size(),
            #[cfg(wgc)]
            Self::Texture(encoder) => encoder.size(),
        }
    }

    fn bitrate(&self) -> u32 {
        match self {
            Self::Picture(encoder) => encoder.bitrate(),
            #[cfg(wgc)]
            Self::Texture(encoder) => encoder.bitrate(),
        }
    }

    fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        match self {
            Self::Picture(encoder) => encoder.set_bitrate(kbps),
            #[cfg(wgc)]
            Self::Texture(encoder) => encoder.set_bitrate(kbps),
        }
    }

    /// Which encoder runs, such as "x264" or "h264_nvenc/d3d11".
    fn engine(&self) -> &str {
        match self {
            Self::Picture(encoder) => encoder.engine(),
            #[cfg(wgc)]
            Self::Texture(encoder) => encoder.engine(),
        }
    }

    /// The encoder's side of a stream's report, which says what made the
    /// picture rather than what was meant to.
    fn report(&self) -> StreamStats {
        match self {
            Self::Picture(encoder) => encoder_report(encoder),
            #[cfg(wgc)]
            Self::Texture(encoder) => {
                let (width, height) = encoder.size();
                StreamStats {
                    engine: encoder.engine().to_owned(),
                    codec: encoder.codec().name().to_owned(),
                    width,
                    height,
                    // A session here opens every setting the stream needs or
                    // fails the open, so one that opened dropped nothing.
                    settings_dropped: String::new(),
                    ..StreamStats::default()
                }
            }
        }
    }

    /// An encoded frame from an I420 picture, for an encoder that reads
    /// pictures: `None` when this stream's frames are the card's, which are not
    /// pictures at all.
    fn encode_i420(
        &mut self,
        picture: &I420Buffer,
        pts: u64,
        keyframe: bool,
    ) -> Option<Result<Option<encode::Frame>>> {
        match self {
            Self::Picture(encoder) => Some(encoder.encode_i420(picture, pts, keyframe)),
            #[cfg(wgc)]
            Self::Texture(_) => None,
        }
    }
}

struct Video {
    sender: StreamSender,
    /// The encoder the stream sends through: none while the card's session is
    /// being rebuilt, since the device it read on may already have gone with
    /// the capture that made it, and the next frame opens another.
    encoder: Option<Encoding>,
    /// Other applications' sound, while sharing a screen.
    sound: Option<stream_audio::Capture>,
    /// Whether the encoder has to be opened again before the next frame: an
    /// encoder whose session the card took away mid-stream, which is a
    /// session to build again rather than one to keep asking.
    reopen: bool,
    /// Frames the send queue was too full to take, since the stream started:
    /// a sender that can't keep up shows here first.
    skipped: u64,
    /// When the stream last reported how it goes out, and the bytes it had
    /// sent then.
    measured: (Instant, u64),
}

/// The report's encoder side: the engine and codec it runs, the size it
/// sends, and the optional settings its driver dropped when it opened. Those
/// names are a short string the shim wrote once, so each report copies them
/// rather than growing with the frames.
pub(crate) fn encoder_report(encoder: &Encoder) -> StreamStats {
    let (width, height) = encoder.size();
    StreamStats {
        engine: encoder.engine().to_owned(),
        codec: encoder.codec().name().to_owned(),
        width,
        height,
        settings_dropped: encoder.configuration().settings_dropped.clone(),
        ..StreamStats::default()
    }
}
impl Video {
    /// How the stream goes out, with the kbit/s sent since it last said.
    fn sending(&mut self, fps: u32) -> StreamStats {
        let sent = self.sender.sent();
        let (since, before) = self.measured;
        let bits = sent.video_bytes.saturating_sub(before) * 8;
        let milliseconds = since.elapsed().as_millis().max(1) as u64;
        self.measured = (Instant::now(), sent.video_bytes);
        // The sound shared with the screen, while there is one: what waits to
        // be mixed is delay a viewer hears before the picture it came with.
        let sound = self
            .sound
            .as_ref()
            .map(stream_audio::Capture::stats)
            .unwrap_or_default();
        StreamStats {
            fps,
            target_kbps: self.encoder.as_ref().map_or(0, Encoding::bitrate),
            sent_kbps: (bits / milliseconds) as u32,
            keyframe_requests: sent.keyframe_requests,
            resent: sent.resent,
            skipped: self.skipped,
            sound_held_ms: sound.held_ms,
            sound_dropped_ms: sound.dropped_ms,
            ..self
                .encoder
                .as_ref()
                .map(Encoding::report)
                .unwrap_or_default()
        }
    }
}

/// The codecs only a GPU encodes, most wanted first, offered before H264
/// where the GPU's encoder for them opens.
const GPU_CODECS: [Codec; 2] = [Codec::Av1, Codec::H265];

fn log_encoder(encoder: &Encoding) {
    let (width, height) = encoder.size();
    tracing::info!(
        engine = encoder.engine(),
        codec = ?encoder.codec(),
        width,
        height,
        "encoding the stream"
    );
}

/// The kbit/s to aim for, as Discord's client does: its cap for frames of
/// `height` lines, or what its server estimates the path carries when
/// that's less, but at least the 500 kbit/s it gives a screen.
fn kbps(height: u32, estimate: Option<u64>) -> u32 {
    let cap = stream_connection::max_bitrate(height) / 1000;
    estimate.map_or(cap, |estimate| {
        (estimate / 1000).clamp(500, u64::from(cap)) as u32
    })
}

impl Sink {
    /// Takes a frame at the size it's sent: the preview shows it, scaled
    /// down to fit, while the UI shows a preview, and Discord gets it once
    /// the stream has a server.
    fn frame(&mut self, picture: &I420Buffer) -> Result<()> {
        self.preview(picture);
        // A share of the card's frames has no picture to send: they reach the
        // stream through `textured`, as the texture they already are. A picture
        // that arrives while that share runs is the UI's alone, and `preview`
        // has just had it.
        if self.footage != Footage::InMemory {
            return Ok(());
        }
        let Some((pts, keyframe)) = self.ready((picture.width(), picture.height()), None)? else {
            return Ok(());
        };
        let video = self.video.as_mut().expect("ready has opened the stream");
        let made = {
            let encoder = video
                .encoder
                .as_mut()
                .expect("ready has opened the stream's encoder");
            (encoder.codec(), encoder.encode_i420(picture, pts, keyframe))
        };
        let (codec, made) = made;
        let Some(made) = made else {
            return Ok(());
        };
        self.delivered(codec, made)
    }

    /// The same for a frame the card captured as a texture `content` big, at
    /// the size the stream sends, `size`: a screen larger than the stream is
    /// scaled to it on the card, in one pass through the driver's video
    /// processor or its shader, and its encoder reads the result where it
    /// lies. No pixel of the frame is converted, uploaded or copied by the
    /// processor; the one copy it still costs is its bitstream out of the
    /// driver.
    #[cfg(wgc)]
    fn textured(
        &mut self,
        texture: d3d11::Texture,
        content: (u32, u32),
        size: (u32, u32),
        device: *mut c_void,
    ) -> Result<()> {
        let Some((pts, keyframe)) = self.ready(size, Some(device))? else {
            return Ok(());
        };
        let video = self.video.as_mut().expect("ready has opened the stream");
        let made = {
            let Some(Encoding::Texture(encoder)) = video.encoder.as_mut() else {
                return Ok(());
            };
            let codec = encoder.codec();
            // A screen the stream's size is read where it lies, and one of
            // another size is scaled to the stream's on the card: the size a
            // capture comes at is the screen's, and the size a stream goes at
            // is Discord's to choose.
            let texture = if content == size {
                texture
            } else {
                encoder.scale(texture, size.0, size.1, d3d11::Resize::Shrink)?
            };
            // A failure here is the session's own, and a session the card took
            // away is one to open again rather than something to withdraw a
            // codec over: the capture that feeds it is the caller's to rebuild
            // with it, so this says nothing to Discord and leaves the codecs it
            // offers alone.
            (codec, encoder.encode_texture(texture, pts, keyframe))
        };
        let (codec, made) = made;
        if let Some(frame) = made? {
            video.sender.send(stream_connection::Frame {
                data: frame.data,
                codec,
                timestamp: frame.timestamp,
                keyframe: frame.keyframe,
            });
        }
        Ok(())
    }

    /// A preview of a frame the card captured, read back from the card rather
    /// than captured a second time: the picture the stream is sending is scaled
    /// on the GPU to fit the tile, copied into the one staging texture the
    /// preview's session keeps, and mapped once.
    ///
    /// What the card reads back is BGRA, so the tile is written to the UI as
    /// RGBA a pixel at a time, and Discord's thumbnail is made first, when one
    /// is due, from the same read: libyuv calls the card's order ARGB.
    #[cfg(wgc)]
    fn preview_from_card(
        &mut self,
        tile: &mut d3d11::D3d11Encoder,
        texture: d3d11::Texture,
        content: (u32, u32),
        pixels: &mut Vec<u8>,
    ) -> Result<()> {
        let due = self.preview_due.is_some_and(|due| due <= Instant::now());
        if !self.preview_shown && !due {
            return Ok(());
        }
        let (width, height) = fit(content.0, content.1, PREVIEW_WIDTH, PREVIEW_HEIGHT);
        let length = (width * height * 4) as usize;
        if pixels.len() < length {
            pixels.resize(length, 0);
        }
        let read = &mut pixels[..length];
        tile.readback(texture, width, height, read)?;
        if due {
            // The tile as a picture, once every five minutes rather than every
            // frame: a screen's colour as the card already shows it, so no tone
            // curve is asked of a texture the compositor has finished with.
            let mut picture = I420Buffer::new(width, height);
            convert(&mut picture, read, width * 4, &Tone::SDR);
            let shown = planes(&picture);
            if !is_black(shown.y) {
                match thumbnail(&shown) {
                    Ok(thumbnail) => self.thumbnail = Some(thumbnail),
                    Err(error) => {
                        tracing::warn!(error = format!("{error:#}"), "could not make a preview")
                    }
                }
                self.preview_due = Some(Instant::now() + THUMBNAIL_EVERY);
            }
        }
        let Sink { writer, .. } = self;
        writer.write(width, height, |rgba| {
            for (pixel, read) in rgba.chunks_exact_mut(4).zip(read.chunks_exact(4)) {
                pixel.copy_from_slice(&[read[2], read[1], read[0], 255]);
            }
        });
        Ok(())
    }

    /// The preview, and Discord's thumbnail when one is due: the picture scaled
    /// down to fit, written for the UI, and a JPEG of it when it isn't black.
    /// This is the frame's own work, and nothing makes one while the UI shows
    /// no preview and none is due for Discord.
    fn preview(&mut self, picture: &I420Buffer) {
        let (width, height) = (picture.width(), picture.height());
        let due = self.preview_due.is_some_and(|due| due <= Instant::now());
        if self.preview_shown || due {
            let preview = fit(width, height, PREVIEW_WIDTH, PREVIEW_HEIGHT);
            let shown = planes(picture);
            let Sink {
                writer,
                preview: scratch,
                ..
            } = self;
            writer.write(preview.0, preview.1, |pixels| {
                shown.to_rgba(&mut *scratch, pixels, preview.0, preview.1);
            });
            // What the preview shows: the picture itself when it's that size
            // already, or the one `to_rgba` scaled into the buffer kept for it.
            let shown = if preview == (width, height) {
                shown
            } else {
                previewed(&scratch[..], preview.0, preview.1)
            };
            if due && !is_black(shown.y) {
                match thumbnail(&shown) {
                    Ok(thumbnail) => self.thumbnail = Some(thumbnail),
                    Err(error) => {
                        tracing::warn!(error = format!("{error:#}"), "could not make a preview")
                    }
                }
                self.preview_due = Some(Instant::now() + THUMBNAIL_EVERY);
            }
        }
    }

    /// Everything a frame needs before it's encoded, and nothing of the encoding
    /// itself: the stream opened at this size and this codec, the rate Discord
    /// last estimated followed, and the frame's own time and whether it has to be
    /// whole. None when the frame is dropped instead — the queue is full, or
    /// nothing has said where the stream goes yet.
    fn ready(
        &mut self,
        (width, height): (u32, u32),
        device: Option<*mut c_void>,
    ) -> Result<Option<(u64, bool)>> {
        if let Some(server) = self.server.take() {
            let (offers, encoder) = self.offers(width, height, device)?;
            let sender = StreamSender::start(server, width, height, self.fps, offers)?;
            let sound = if self.sound {
                stream_audio::Capture::start(sender.sound())
                    .inspect_err(|error| {
                        tracing::warn!(error = format!("{error:#}"), "sharing without sound")
                    })
                    .ok()
            } else {
                None
            };
            self.video = Some(Video {
                sender,
                encoder: Some(encoder),
                sound,
                reopen: false,
                skipped: 0,
                measured: (Instant::now(), 0),
            });
        }
        let Some(video) = self.video.as_ref() else {
            return Ok(None);
        };
        // Discord's choice of codec changes as viewers come and go.
        let codec = video
            .sender
            .codec()
            .or_else(|| video.encoder.as_ref().map(Encoding::codec))
            .unwrap_or(stream_connection::SEND_CODEC);
        let resized = video
            .encoder
            .as_ref()
            .is_none_or(|encoder| encoder.size() != (width, height));
        // An encoder the card took away between frames is opened again at the
        // size the frames are now, whether or not that changed.
        if resized
            || video.reopen
            || video
                .encoder
                .as_ref()
                .is_none_or(|encoder| encoder.codec() != codec)
        {
            let opened = self.encoder(codec, width, height, device);
            let video = self.video.as_mut().expect("checked above");
            match opened {
                Ok(encoder) => {
                    video.encoder = Some(encoder);
                    video.reopen = false;
                    if resized {
                        video.sender.resize(width, height);
                    }
                }
                // A GPU codec that opened at the start may not open again,
                // or at another size: Discord then chooses another, which
                // the next frame opens.
                Err(error) if codec != stream_connection::SEND_CODEC => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        ?codec,
                        "the GPU's encoder didn't open again"
                    );
                    video.sender.withdraw(codec);
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
        }
        let Some(video) = self.video.as_mut() else {
            return Ok(None);
        };
        let aimed = video.encoder.as_ref().expect("opened above").bitrate();
        // A rate a tenth below the aim is followed at once, and one a quarter
        // above it to climb; between the two is a wobble, and an estimate that
        // stopped coming says nothing at all.
        if let Some(wanted) = video.sender.bitrate(height, aimed) {
            match video
                .encoder
                .as_mut()
                .expect("opened above")
                .set_bitrate(wanted)
            {
                Ok(()) => tracing::debug!(kbps = wanted, "following Discord's bandwidth estimate"),
                Err(error) => {
                    tracing::warn!(error = format!("{error:#}"), "could not change the bitrate")
                }
            }
        }
        // A full queue drops the frame anyway, so the encode it would throw
        // away is skipped; the request for a whole frame waits for the one
        // that goes.
        if video.sender.backlogged() {
            video.skipped = video.skipped.saturating_add(1);
            return Ok(None);
        }
        let pts = self.started.elapsed().as_micros() as u64 * encode::CLOCK / 1_000_000;
        let keyframe = video.sender.take_keyframe_request();
        Ok(Some((pts, keyframe)))
    }

    /// Puts an encoded frame on the stream, as the packetizer, its resend history
    /// and the ceiling on keyframe requests have always seen them: the same
    /// bytes, the same time on the stream clock and the same `keyframe` flag,
    /// whichever encoder made them.
    fn delivered(&mut self, codec: Codec, made: Result<Option<encode::Frame>>) -> Result<()> {
        let Some(video) = self.video.as_mut() else {
            return Ok(());
        };
        match made {
            Ok(Some(frame)) => video.sender.send(stream_connection::Frame {
                data: frame.data,
                codec,
                timestamp: frame.timestamp,
                keyframe: frame.keyframe,
            }),
            Ok(None) => {}
            // Only the GPU encodes AV1 and H265: Discord chooses another
            // codec instead, which the next frame encodes.
            Err(error) if codec != stream_connection::SEND_CODEC => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    ?codec,
                    "the GPU's encoder failed"
                );
                video.sender.withdraw(codec);
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    /// Whether a preview is wanted now: the UI shows one, or Discord's thumbnail
    /// is due. Nothing is made while neither is.
    #[cfg(wgc)]
    fn preview_wanted(&self) -> bool {
        self.preview_shown || self.preview_due.is_some_and(|due| due <= Instant::now())
    }

    /// Lets go of the stream's encoder, keeping the rest: the device the
    /// encoder read on is about to go with the capture that made it, and a
    /// session that outlived its device reads memory nobody owns. The next
    /// frame opens another on the device the capture has then.
    #[cfg(wgc)]
    fn let_go(&mut self) {
        if let Some(video) = self.video.as_mut() {
            video.encoder = None;
            video.reopen = true;
        }
    }

    /// The codecs to offer Discord, most wanted first, and an encoder for
    /// the first: AV1, then H265, where the GPU's encoder for each opens,
    /// then H264. Discord streams the first every viewer decodes. Each is
    /// opened to see that it opens, and only the first kept.
    fn offers(
        &self,
        width: u32,
        height: u32,
        device: Option<*mut c_void>,
    ) -> Result<(Vec<Codec>, Encoding)> {
        let mut offers = vec![];
        let mut first = None;
        for codec in GPU_CODECS.iter().copied().filter(|_| self.hardware) {
            if let Ok(encoder) = self.open(codec, width, height, device) {
                offers.push(codec);
                first.get_or_insert(encoder);
            }
        }
        offers.push(stream_connection::SEND_CODEC);
        let encoder = match first {
            Some(encoder) => encoder,
            None => self.open(stream_connection::SEND_CODEC, width, height, device)?,
        };
        log_encoder(&encoder);
        Ok((offers, encoder))
    }

    /// An encoder for the stream in `codec`, as [`Self::open`] opens it.
    fn encoder(
        &self,
        codec: Codec,
        width: u32,
        height: u32,
        device: Option<*mut c_void>,
    ) -> Result<Encoding> {
        let encoder = self.open(codec, width, height, device)?;
        log_encoder(&encoder);
        Ok(encoder)
    }

    /// An encoder in `codec`, at the bitrate Discord's cap for its height
    /// allows, reading the frames where this share's capture puts them: on the
    /// card, a session opens on the device the capture fills its textures on,
    /// which the stream hands over as `device`.
    fn open(
        &self,
        codec: Codec,
        width: u32,
        height: u32,
        _device: Option<*mut c_void>,
    ) -> Result<Encoding> {
        let aimed = kbps(height, None);
        match self.footage {
            Footage::InMemory => Ok(Encoding::Picture(Encoder::new(
                codec,
                width,
                height,
                self.fps,
                aimed,
                self.effort,
                self.hardware,
            )?)),
            // The card's session reads a texture only on the device it opened
            // on, so it opens on the capture's own, which is the whole of what
            // lets a frame reach it without being copied first.
            #[cfg(wgc)]
            Footage::OnTheCard => Ok(Encoding::Texture(d3d11::D3d11Encoder::open_on(
                _device.context("the card's encoder needs the capture's device")?,
                codec,
                width,
                height,
                self.fps,
                aimed,
            )?)),
        }
    }

    /// Takes Yapfast's controls, reports the stream starting, and fails once
    /// its connection ended. Gives the source a [`Control::Switch`] asked for.
    fn check(&mut self, report: &mut dyn FnMut(Report)) -> Result<Option<Source>> {
        let mut switch = None;
        while let Ok(control) = self.controls.try_recv() {
            match control {
                Control::Send(server) => self.server = Some(server),
                Control::Switch(source) => switch = Some(source),
                Control::Preview(shown) => self.preview_shown = shown,
                _ => {}
            }
        }
        match self.video.as_ref().map(|video| video.sender.link()) {
            Some(Link::Connected) if !self.streaming => {
                self.streaming = true;
                self.preview_due = Some(Instant::now());
                report(Report::Streaming);
            }
            // Discord ends the connection when it moves the stream to
            // another server, which then arrives as another Send.
            Some(Link::Ended(reason)) => {
                let lost = *self.lost.get_or_insert_with(Instant::now);
                if lost.elapsed() >= MOVE_GRACE {
                    bail!("Lost the stream connection: {reason}");
                }
            }
            _ => self.lost = None,
        }
        if let Some(thumbnail) = self.thumbnail.take() {
            report(Report::Preview { thumbnail });
        }
        if let Some(video) = self.video.as_mut()
            && self.streaming
            && video.measured.0.elapsed() >= SENDING_EVERY
        {
            report(Report::Sending(video.sending(self.fps)));
        }
        Ok(switch)
    }
}

/// Whether a picture's luma is black, as a screen is before what's shared
/// draws: Discord's client waits for one that isn't before sending a preview.
fn is_black(y: &[u8]) -> bool {
    // Black's luma is 16; some of a picture of anything is brighter.
    y.iter().step_by(7).all(|&luma| luma <= 24)
}

/// A preview of a picture as Discord's client makes one: scaled down to fit
/// 512 by 288, a JPEG as a data URI.
fn thumbnail(picture: &I420<'_>) -> Result<String> {
    use base64::Engine;
    use image::{ExtendedColorType, codecs::jpeg::JpegEncoder};
    let (width, height) = fit(
        picture.width,
        picture.height,
        THUMBNAIL_WIDTH,
        THUMBNAIL_HEIGHT,
    );
    let mut scratch = vec![];
    let mut rgba = vec![0; (width * height * 4) as usize];
    picture.to_rgba(&mut scratch, &mut rgba, width, height);
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let mut jpeg = vec![];
    JpegEncoder::new_with_quality(&mut jpeg, 92).encode(
        &rgb,
        width,
        height,
        ExtendedColorType::Rgb8,
    )?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(jpeg)
    ))
}

/// How a watched stream's sound plays, and Yapfast's word on hearing it.
struct Sound {
    speakers: Option<String>,
    hearing: Arc<AtomicBool>,
    volume: Arc<stream_audio::Volume>,
    controls: channel::Receiver<Control>,
}

/// What a watched stream's frames are decoded with, and whether the frame in
/// hand may reach it.
///
/// A decoder that has just failed, or just been replaced because the stream
/// turned to another codec, holds nothing the frames after it build on: each
/// references a picture it never decoded, and would fail too. So the watch
/// waits for a keyframe, as the assembler does when it had to drop frames,
/// instead of spending a decode on every delta of a chain already broken.
struct Decoding {
    decoder: Option<Live>,
    /// No frame reaches the decoder until a keyframe does.
    awaiting_keyframe: bool,
    /// The codec this build has no decoder for, so frames of it are dropped
    /// without trying again. A watch waits through them: the stream's sound
    /// plays on, and a streamer who turns to another codec gets pictures
    /// back.
    undecodable: Option<Codec>,
    /// Whether the machine's decoder gave up on this stream, which leaves the
    /// software one to take the next keyframe and every frame after it.
    broken: bool,
    /// How a decoder is made, which a test makes fail the way a build without
    /// a codec's decoder does.
    make: fn(Codec) -> Result<Live>,
}

/// What became of a frame handed to a [`Decoding`].
///
/// [`Decoded::Waiting`] and [`Decoded::Held`] are frames the watch drops as
/// they are: one before any work is done on it, the other after the decoder
/// took it and kept it.
enum Decoded<'a> {
    /// A picture to show.
    Picture(Frame<'a>),
    Held,
    Waiting,
    /// The decoder couldn't use the frame, so the frames after it wait.
    Failed(anyhow::Error),
}

impl Decoding {
    fn new() -> Self {
        Self {
            decoder: None,
            // The assembler starts a stream at a keyframe.
            awaiting_keyframe: false,
            undecodable: None,
            broken: false,
            make: Live::new,
        }
    }

    /// Decodes `frame` if it should be; [`Decoded::Waiting`] says it wasn't
    /// worth the work and the next frame may be.
    fn decode(&mut self, frame: &ReceivedFrame) -> Decoded<'_> {
        let codec = frame.codec;
        if self
            .decoder
            .as_ref()
            .is_some_and(|decoder| decoder.codec() != codec)
        {
            // Nothing one decoder held carries into another: the frames
            // after a codec change reference pictures only the new decoder
            // makes.
            self.decoder = None;
            self.awaiting_keyframe = true;
        }
        if self.decoder.is_none() {
            if self.undecodable == Some(codec) {
                return Decoded::Waiting;
            }
            let make = self.make;
            let made = if self.broken {
                Live::software(codec)
            } else {
                make(codec)
            };
            match made {
                Ok(decoder) => {
                    self.decoder = Some(decoder);
                    self.undecodable = None;
                }
                Err(error) => {
                    // A codec this build can't decode ends this stream's
                    // video, not the watch: its sound plays on, and a stream
                    // Discord moves to a codec this build has gets pictures
                    // again.
                    tracing::warn!(
                        error = format!("{error:#}"),
                        codec = ?codec,
                        "no decoder for a watched stream"
                    );
                    self.undecodable = Some(codec);
                    return Decoded::Waiting;
                }
            }
        }
        if self.awaiting_keyframe {
            if !frame.keyframe {
                return Decoded::Waiting;
            }
            self.awaiting_keyframe = false;
        }
        let Some(decoder) = self.decoder.as_mut() else {
            return Decoded::Waiting;
        };
        let on_the_card = decoder.on_the_card();
        match decoder.decode(&frame.data, frame.keyframe) {
            Ok(Some(picture)) => Decoded::Picture(picture),
            Ok(None) => Decoded::Held,
            Err(error) => {
                self.awaiting_keyframe = true;
                // A stream the machine's decoder gave up on is the software
                // decoder's from the next keyframe; one the software decoder
                // couldn't read is read again when the chain is whole.
                self.broken = on_the_card;
                Decoded::Failed(error)
            }
        }
    }

    /// The machine's decoder couldn't show a picture, so it stops being used
    /// for this stream: the frames after it wait for a keyframe, which the
    /// software decoder starts again from.
    fn abandon(&mut self) {
        self.decoder = None;
        self.broken = true;
        self.awaiting_keyframe = true;
    }
}

/// Shows someone's stream: whole frames from the connection, decoded, then
/// scaled down to the tile showing them, and plays its sound.
fn watch(
    server: StreamServer,
    mut writer: Writer,
    (mut max_width, mut max_height): (u32, u32),
    sound: Sound,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    let mut receiver = StreamReceiver::start(server, crate::decode::DECODES.to_vec())?;
    let _player = receiver.take_sounds().and_then(|sounds| {
        stream_audio::Player::start(
            sounds,
            sound.speakers,
            sound.hearing.clone(),
            sound.volume.clone(),
        )
        .inspect_err(|error| tracing::warn!(error = format!("{error:#}"), "watching without sound"))
        .ok()
    });
    let mut decoding = Decoding::new();
    // A picture that doesn't fit the tile showing it scales through this, at
    // the size it shows; one that does goes to the shared frame as it is.
    let mut scratch = vec![];
    // A whole frame taken off the stream only to learn that a newer picture
    // was already waiting: decoded on the next turn, in order, so taking it
    // never skips one.
    let mut ahead: Option<ReceivedFrame> = None;
    // When the last picture reached the shared frame, and how many a newer one
    // replaced before it could.
    let mut showing: Option<Instant> = None;
    let mut replaced = 0u64;
    let mut failures = 0u64;
    // Whether a window shows these pictures at all, as the UI says when that
    // changes: a hidden or minimised window shows none, and a picture nobody
    // can see isn't converted or written. The stream is decoded all the same.
    let mut visible = true;
    while !stop.load(Ordering::Relaxed) {
        while let Ok(control) = sound.controls.try_recv() {
            match control {
                Control::Hear(hearing) => sound.hearing.store(hearing, Ordering::Relaxed),
                Control::Volume(volume) => sound.volume.set(volume),
                Control::Shown(shown) => visible = shown,
                // The tile showing the stream says how large it is when that
                // changes, not with every frame: the stream is scaled to it
                // from then on, and the frames file is grown once to hold it.
                Control::Size { width, height } => {
                    if let Some(size) = tile_size((width, height))
                        && size != (max_width, max_height)
                    {
                        (max_width, max_height) = size;
                        if let Err(error) = writer.reserve(max_width, max_height) {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "could not make room in the frames for the tile"
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        if let Link::Ended(reason) = receiver.link() {
            bail!("Lost the stream: {reason}");
        }
        let Some(frame) = ahead
            .take()
            .or_else(|| receiver.next(Duration::from_millis(100)))
        else {
            continue;
        };
        let picture = match decoding.decode(&frame) {
            Decoded::Picture(picture) => picture,
            Decoded::Held | Decoded::Waiting => continue,
            Decoded::Failed(error) => {
                failures += 1;
                // The first of a run, and then now and again.
                if failures == 1 || failures.is_multiple_of(100) {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        failures,
                        keyframe = frame.keyframe,
                        size = frame.data.len(),
                        "a stream frame didn't decode"
                    );
                }
                receiver.request_keyframe();
                continue;
            }
        };
        // The whole frame that would replace this picture, if it is already
        // here. It waits for the next turn rather than being dropped, so
        // looking ahead never skips a frame the decoder needs.
        let newer = match receiver.next(Duration::ZERO) {
            Some(frame) => {
                ahead = Some(frame);
                true
            }
            None => false,
        };
        match worth_showing(visible, showing, Instant::now(), newer) {
            Worth::Show => {}
            Worth::Replaced => {
                replaced += 1;
                // The first of a run, and then now and again, as for a stream
                // frame that didn't decode: a watch that shows fewer pictures
                // than it decodes is one whose decode is outrunning its screen.
                if replaced == 1 || replaced.is_multiple_of(100) {
                    let (width, height) = picture.size();
                    tracing::debug!(
                        replaced,
                        width,
                        height,
                        "a decoded picture a newer one replaced wasn't shown"
                    );
                }
                continue;
            }
            // The frame was decoded — the pictures after it are made from it,
            // and waiting for a keyframe when the window came back would show
            // the still it stopped on until one came — but nothing is
            // converted or written for a window that shows no picture.
            Worth::Unseen => continue,
        }
        let mut abandoned = false;
        let (width, height) = match picture {
            Frame::Software(picture) => write_picture(
                &mut writer,
                &picture.i420(),
                &mut scratch,
                (max_width, max_height),
            ),
            // The machine's decoder left the picture on the graphics card, at
            // the size the stream was encoded at: the card scales it to the
            // tile in the same pass that reads its colour, and only the tile
            // comes back to the processor.
            #[cfg(d3d11dec)]
            Frame::Hardware(hardware) => {
                let (stream_width, stream_height) = hardware.size();
                let (width, height) = fit(stream_width, stream_height, max_width, max_height);
                let written = writer.write_rendered(width, height, |pixels| {
                    hardware.rgba(pixels, width, height).is_ok()
                });
                if !written {
                    abandoned = true;
                }
                (width, height)
            }
        };
        if abandoned {
            tracing::warn!("a picture the graphics card decoded wasn't shown");
            decoding.abandon();
            receiver.request_keyframe();
        }
        let first = showing.is_none();
        // Once the conversion is done, not when it began: what the next
        // picture is measured from is when this one reached the screen.
        showing = Some(Instant::now());
        if first {
            tracing::info!(width, height, codec = ?frame.codec, "showing the stream");
            report(Report::Streaming);
        }
    }
    Ok(())
}

/// Writes `picture` into the shared frames at the size the tile showing it
/// takes: its own when that fits inside `(max_width, max_height)`, and else
/// scaled down to fit, through `scratch`, which is sized once and kept. A
/// stream smaller than its tile keeps its own size — nothing is scaled up —
/// so a small stream costs no more than it is. Says the size it was written
/// at, which is the size it shows at.
fn write_picture(
    writer: &mut Writer,
    picture: &I420<'_>,
    scratch: &mut Vec<u8>,
    (max_width, max_height): (u32, u32),
) -> (u32, u32) {
    let (width, height) = fit(picture.width, picture.height, max_width, max_height);
    writer.write(width, height, |pixels| {
        picture.to_rgba(scratch, pixels, width, height)
    });
    (width, height)
}

/// What a stream is scaled to fit, given the size in pixels of the tile
/// showing it, as the UI says it: none while no tile shows it, which says
/// nothing about the size, and never more than a stream is shown at, however
/// large the window is.
fn tile_size((width, height): (u32, u32)) -> Option<(u32, u32)> {
    (width > 0 && height > 0).then(|| (width.min(WATCH_WIDTH), height.min(WATCH_HEIGHT)))
}

/// How soon after one picture another is worth converting and scaling: faster
/// than any screen a stream plays on draws, so a picture decoded within this
/// of the one before it is one the next replaces before it's seen.
const SHOW_EVERY: Duration = Duration::from_millis(8);

/// What a decoded picture is worth, as no screen takes every one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Worth {
    /// Convert it to RGBA and write it into the shared frames.
    Show,
    /// Let it go: a newer whole frame was already in hand, and the screen
    /// drew the picture before this within [`SHOW_EVERY`] of it, so no screen
    /// would draw it.
    Replaced,
    /// Let it go: no window shows a picture, being hidden to the tray or
    /// minimised. The frame was decoded all the same.
    Unseen,
}

/// What a picture decoded now is worth, given whether a window shows
/// pictures at all, when one last reached the screen, and whether a newer
/// whole frame was already in hand.
///
/// Decoding is never skipped this way — a picture the next one references has
/// to be decoded for the pictures after it, and the whole frame taken to find
/// a newer one is decoded on the next turn. What is skipped is the RGBA
/// conversion of a picture the screen would never show, which is the work the
/// picture the viewer is waiting for would otherwise pay for. A picture that
/// arrives a frame period after the last is always shown, whatever else is in
/// hand, so a watch that keeps up shows every frame and leaves nothing out —
/// and the first picture after a hidden window comes back is one of those,
/// the last shown being long enough ago.
fn worth_showing(visible: bool, shown: Option<Instant>, now: Instant, newer: bool) -> Worth {
    if !visible {
        return Worth::Unseen;
    }
    match shown {
        Some(shown) if newer && now.duration_since(shown) < SHOW_EVERY => Worth::Replaced,
        _ => Worth::Show,
    }
}

/// When frame `frame` of a loop begun at `started` is due at `fps`.
fn due_at(started: Instant, frame: u64, fps: u32) -> Option<Instant> {
    started.checked_add(Duration::from_secs(frame).checked_div(fps)?)
}

/// The frame a loop begun at `started` makes at `now`, and how long to wait for
/// it first.
///
/// On time that's the frame next in line; behind, it's the latest frame whose
/// deadline has passed, so a stall skips the frames it swallowed instead of
/// making all of them in a burst. Deadlines stay counted from `started`, so
/// elapsed time, and the timestamps made from it, run on through the stall.
fn pace(started: Instant, frame: u64, fps: u32, now: Instant) -> (u64, Option<Duration>) {
    let Some(due) = due_at(started, frame, fps) else {
        // No cadence to keep: a job's frame rate is clamped to 1–60, and zero
        // would divide by it. Make the frame asked for, at once.
        return (frame, None);
    };
    if let Some(wait) = due.checked_duration_since(now) {
        return (frame, Some(wait));
    }
    // The latest frame whose deadline is at or before `now`, counted in the
    // same whole nanoseconds `due_at` divides by.
    let elapsed = now.saturating_duration_since(started);
    let expired = elapsed.as_nanos() * u128::from(fps) / 1_000_000_000;
    (frame.max(u64::try_from(expired).unwrap_or(u64::MAX)), None)
}

/// Sleeps until the next frame is due and gives it, or returns `None` once told
/// to stop; the frame after it waits its turn, so a stall costs one late frame
/// rather than a burst of the frames it swallowed.
fn next_frame(started: Instant, frame: &mut u64, fps: u32, stop: &AtomicBool) -> Option<u64> {
    let (due, wait) = pace(started, *frame, fps, Instant::now());
    if let Some(wait) = wait {
        std::thread::sleep(wait);
    }
    if stop.load(Ordering::Relaxed) {
        return None;
    }
    *frame = due.saturating_add(1);
    Some(due)
}

fn test_pattern(
    mut sink: Sink,
    width: u32,
    height: u32,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    let (width, height) = (width.min(640), height.min(360));
    report(Report::Sharing { width, height });
    let started = Instant::now();
    let mut slot = 0u64;
    let mut rgba = vec![0; (width * height * 4) as usize];
    let mut picture = I420Buffer::new(width, height);
    while let Some(frame) = next_frame(started, &mut slot, sink.fps, stop) {
        let shift = (frame * 4) as u32;
        for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
            let (x, y) = (index as u32 % width, index as u32 / width);
            let bar = ((x + shift) * 8 / width) % 8;
            let level = 255 - (y * 128 / height) as u8;
            let on = |bit: u32| if bar & bit != 0 { level } else { 32 };
            pixel.copy_from_slice(&[on(4), on(2), on(1), 255]);
        }
        let (y_stride, u_stride, v_stride) = picture.strides();
        let (y, u, v) = picture.data_mut();
        // libyuv names formats by their order in a little-endian word: its
        // ABGR is R, G, B, A in memory.
        yuv_helper::abgr_to_i420(
            &rgba,
            width * 4,
            y,
            y_stride,
            u,
            u_stride,
            v,
            v_stride,
            width as i32,
            height as i32,
        );
        sink.frame(&picture)?;
        // Colour bars are all it shares.
        let _switch = sink.check(report)?;
    }
    Ok(())
}

/// The largest frame shared at `resolution`; screens of another shape fit
/// inside it.
fn bounds(resolution: Resolution) -> (u32, u32) {
    match resolution {
        Resolution::P720 => (1280, 720),
        Resolution::P1080 => (SHARE_WIDTH, SHARE_HEIGHT),
        Resolution::Source => (3840, 2160),
    }
}

/// The size `width` by `height` takes when scaled down to fit `max_width`
/// by `max_height`, keeping its shape. Codecs want even sizes.
pub fn fit(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(max_width) / f64::from(width.max(1)),
            f64::from(max_height) / f64::from(height.max(1)),
        ),
    );
    let even = |value: f64| ((value as u32) & !1).max(2);
    (
        even(f64::from(width) * scale),
        even(f64::from(height) * scale),
    )
}

/// Converts an I420 picture into RGBA pixels of its size.
fn to_rgba(picture: &I420Buffer, pixels: &mut [u8]) {
    let (y, u, v) = picture.data();
    let (y_stride, u_stride, v_stride) = picture.strides();
    let (width, height) = (picture.width(), picture.height());
    yuv_helper::i420_to_abgr(
        y,
        y_stride,
        u,
        u_stride,
        v,
        v_stride,
        pixels,
        width * 4,
        width as i32,
        height as i32,
    );
}

// libwebrtc asks the screen sharing portal over D-Bus through GIO, but
// webrtc-sys only compiles against its headers.
#[cfg(target_os = "linux")]
#[link(name = "gio-2.0")]
#[link(name = "gobject-2.0")]
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_main_context_iteration(
        context: *mut std::ffi::c_void,
        may_block: std::ffi::c_int,
    ) -> std::ffi::c_int;
}

/// Runs what's due on GLib's main context. The portal's D-Bus replies arrive
/// there, and nothing else runs it in this process: without this the
/// dialog never opens.
fn run_glib() {
    #[cfg(target_os = "linux")]
    // SAFETY: the default context; only the capture thread iterates it.
    while unsafe { g_main_context_iteration(std::ptr::null_mut(), 0) } != 0 {}
}

#[derive(Default)]
struct Captured {
    size: Option<(u32, u32)>,
    /// Capture stopped for good, or a frame couldn't be sent.
    failed: Option<String>,
}

fn capture_screen(
    source: Source,
    sink: Sink,
    max_width: u32,
    max_height: u32,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    // Where a share's frames come from is decided here, and nowhere else: from
    // the card, as the texture Windows Graphics Capture fills, when this build
    // has that capture and the machine can encode what it fills; from the
    // processor's memory, as the I420 libwebrtc's capturer hands over,
    // everywhere else, which is every machine without NVENC. Everything that
    // keeps a share on the processor is named in the line this logs, so a
    // stream that costs more than it should says why.
    #[cfg(wgc)]
    match card_capture(&source, &sink, (max_width, max_height)) {
        Ok((capture, monitor)) => {
            return capture_textures(
                capture,
                monitor,
                sink,
                (max_width, max_height),
                stop,
                report,
            );
        }
        Err(why) => tracing::info!(why = format!("{why:#}"), "sharing from the processor"),
    }
    let fps = sink.fps;
    let sink = Arc::new(Mutex::new(sink));
    let mut capture = Capture::start(source, &sink, max_width, max_height)?;
    // A screen, window or camera to share in its place, until it gives a
    // picture.
    let mut switching: Option<Capture> = None;
    let started = Instant::now();
    // Only the pace of the loop matters here: frames a stall swallowed are
    // skipped, not captured in a burst.
    let mut slot = 0u64;
    let mut reported = None;
    while next_frame(started, &mut slot, fps, stop).is_some() {
        run_glib();
        capture.capture_frame();
        if let Some(next) = &mut switching {
            next.capture_frame();
        }
        // The capture's status, read and let go of before anything else
        // runs: a camera writes it from its own thread, and a report isn't
        // its work.
        let (size, failure) = {
            let seen = capture.seen();
            (seen.size, seen.failed.clone())
        };
        if let Some(failure) = failure {
            bail!(if reported.is_none() {
                "Screen sharing was cancelled, or this desktop has no screen sharing dialog."
                    .to_owned()
            } else {
                failure
            });
        }
        if size != reported
            && let Some((width, height)) = size
        {
            reported = size;
            report(Report::Sharing { width, height });
        }
        if let Some(next) = &switching {
            let seen = next.seen();
            if let Some(failure) = &seen.failed {
                tracing::info!(failure, "sharing the same screen or window");
                drop(seen);
                switching = None;
            } else if seen.size.is_some() {
                drop(seen);
                capture = switching.take().expect("switching");
                // Discord shows what's shared now.
                let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if sink.streaming {
                    sink.preview_due = Some(Instant::now());
                }
            }
        }
        let switch = sink
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .check(report)?;
        if let Some(source) = switch {
            // A switch asked for while another waits replaces it.
            switching = None;
            match Capture::start(source, &sink, max_width, max_height) {
                Ok(next) => switching = Some(next),
                Err(error) => tracing::warn!(
                    error = format!("{error:#}"),
                    "sharing the same screen or window"
                ),
            }
        }
    }
    Ok(())
}

/// Which screen `source` is, when it is one: the display's own handle, or the
/// one the desktop starts on when nothing has been picked yet. A window or a
/// camera is not a screen, and saying so is the reason a share of one stays on
/// the processor.
#[cfg(wgc)]
fn screen_of(source: &Source) -> Result<wgc::Monitor> {
    match source {
        Source::Display(screen) => Ok(wgc::Monitor::new(*screen as usize as *mut std::ffi::c_void)),
        Source::Screen => Ok(wgc::Monitor::PRIMARY),
        Source::Window(_) => bail!(
            "a window is captured on the processor: what Windows Graphics Capture \
             fills a texture from here is a screen"
        ),
        Source::Camera(_) => bail!("a camera isn't a screen"),
        Source::TestPattern => bail!("a test pattern is drawn, not captured"),
    }
}

/// A capture of `source` from the card, or the reason this share stays on the
/// processor.
///
/// Each thing that stops the card's path is worth saying rather than guessing
/// at, because each one leaves the share doing what it has always done: a
/// machine whose hardware setting is off, a display that isn't a screen, and a
/// card NVENC won't open a session on. A screen larger than the stream is no
/// longer one of them: the session that reads the capture's texture scales it
/// to the stream's size on the card.
#[cfg(wgc)]
fn card_capture(
    source: &Source,
    sink: &Sink,
    (max_width, max_height): (u32, u32),
) -> Result<(wgc::Capture, wgc::Monitor)> {
    if !sink.hardware {
        bail!("the stream is set to encode on the processor");
    }
    let monitor = screen_of(source)?;
    let capture = wgc::Capture::open(monitor)?;
    let (width, height) = capture.content_size();
    if width == 0 || height == 0 {
        bail!("the screen is {width}x{height}, which is nothing to capture");
    }
    // The stream's own session opens here and is let go: what it asks is
    // whether NVENC opens on the device the capture fills its textures on,
    // which is the one thing that decides whether these halves fit together.
    // A session reads a texture only on the device it opened on, so asked
    // before the loop starts, a card that can't answer leaves the share on the
    // processor instead of failing its first frame.
    let (sent_width, sent_height) = fit(width, height, max_width, max_height);
    d3d11::D3d11Encoder::open_on(
        capture.device(),
        stream_connection::SEND_CODEC,
        sent_width,
        sent_height,
        sink.fps,
        kbps(sent_height, None),
    )
    .context("the card won't open an encoder on the device it captures with")?;
    Ok((capture, monitor))
}

/// Shares a screen from the card: each frame the compositor makes goes to the
/// encoder as the texture it filled it in — scaled on the card to the stream's
/// size where the screen is larger — and the bitstream out of the driver is the
/// only time a frame's bytes touch the processor.
///
/// Nothing here keeps a frame. The texture is the capture's pool's, borrowed
/// until the next [`wgc::Capture::frame`]; the encoder's buffers come back to it
/// as frames go out; and the one picture held in memory is the preview's tile,
/// scaled on the card and read back at no more than [`PREVIEW_WIDTH`] by
/// [`PREVIEW_HEIGHT`].
#[cfg(wgc)]
fn capture_textures(
    mut capture: wgc::Capture,
    monitor: wgc::Monitor,
    mut sink: Sink,
    (max_width, max_height): (u32, u32),
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    let (screen_width, screen_height) = capture.content_size();
    let (sent_width, sent_height) = fit(screen_width, screen_height, max_width, max_height);
    tracing::info!(
        capture = capture.describe().as_str(),
        screen = format!("{screen_width}x{screen_height}"),
        stream = format!("{sent_width}x{sent_height}"),
        "sharing from the card"
    );
    let fps = sink.fps;
    sink.footage = Footage::OnTheCard;
    let sink = Arc::new(Mutex::new(sink));
    let started = Instant::now();
    // Only the pace of the loop matters here: a screen that drew nothing is a
    // frame skipped, not one captured twice.
    let mut slot = 0u64;
    let mut reported = None;
    // The session a preview's tile is read back through, opened on the capture's
    // device when something first wants a preview and let go when nothing does.
    // A session of its own, because the encoder scales into one texture it keeps
    // for each size, and a tile sharing the stream's session would remake that
    // texture every frame, alternating the stream's size with the tile's.
    let mut tile: Option<d3d11::D3d11Encoder> = None;
    // The tile's pixels, in the card's order: BGRA, one row after another.
    let mut pixels: Vec<u8> = vec![];
    let mut read: Option<Instant> = None;
    // How many times the session and the capture have been put back together
    // since a frame last went out.
    let mut restarts = 0u32;
    while next_frame(started, &mut slot, fps, stop).is_some() {
        let arrival = capture.frame()?;
        // Everything the sink has to say about this pass, under its lock once:
        // the controls, which is also where its reports go, whether a preview is
        // wanted, and the frame itself when one arrived. No lock is held while
        // the capture, the encoder or the preview work.
        let mut taken = Ok(());
        let mut size = None;
        let (switched, wanted) = {
            let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let switched = sink.check(report)?;
            let wanted = sink.preview_wanted();
            match arrival {
                wgc::Arrival::Frame => {
                    let content = capture.content_size();
                    let sent = fit(content.0, content.1, max_width, max_height);
                    taken = sink.textured(
                        d3d11::Texture::new(capture.texture()),
                        content,
                        sent,
                        capture.device(),
                    );
                    size = Some(sent);
                }
                // The screen changed size and the capture's pool with it, so the
                // next frame is at the new size: `ready` opens an encoder for it
                // and tells the sender, and a session's first frame is whole.
                wgc::Arrival::Resized => tracing::info!("the screen changed size"),
                // Nothing has been drawn since the last frame, which is the
                // compositor's own rate: the next pass is the next slot.
                wgc::Arrival::Idle => {}
                // What was being captured closed, or its session ended. This
                // capture hands out no more frames, so the share ends rather than
                // waits on one that won't come.
                wgc::Arrival::Closed => bail!("the capture of the screen ended"),
            };
            (switched, wanted)
        };
        if let Some((width, height)) = size
            && reported != size
        {
            reported = size;
            report(Report::Sharing { width, height });
        }
        if let Some(source) = switched {
            // Another screen is another capture and another encoder, and this
            // loop holds one of each; the stream ends rather than showing a
            // picture nobody sends, and the next share picks the other screen
            // from the start.
            bail!("can't switch to {source:?} while the screen is shared from the card");
        }
        match taken {
            // A session the card took away, which a driver reset is the usual
            // cause of. Both halves are built again, since the device that
            // filled the textures is the one that went, and a card that answers
            // every frame with the same failure is one to stop sharing on.
            Err(error) => {
                restarts = restarts.saturating_add(1);
                if restarts > CARD_RESTARTS {
                    return Err(error.context("the card's encoder stopped"));
                }
                tracing::warn!(
                    error = format!("{error:#}"),
                    "the card's encoder stopped; opening it and the capture again"
                );
                // Nothing that reads the old device may outlive it: the tile's
                // session goes now, and the stream's before the capture that
                // made the device is let go of.
                tile = None;
                sink.lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .let_go();
                capture = wgc::Capture::open(monitor)?;
            }
            Ok(()) => restarts = 0,
        }
        // A preview is the one thing a texture can't be made into: the UI's tile
        // and Discord's thumbnail are pictures in memory, and this frame is on
        // the card. So the frame is scaled on the card to fit the tile and read
        // back into the pixels kept for it, ten times a second at most and only
        // while something shows one.
        match (wanted, tile.is_some()) {
            (true, false) => {
                let content = capture.content_size();
                let preview = fit(content.0, content.1, PREVIEW_WIDTH, PREVIEW_HEIGHT);
                match d3d11::D3d11Encoder::open_on(
                    capture.device(),
                    stream_connection::SEND_CODEC,
                    preview.0,
                    preview.1,
                    fps,
                    kbps(preview.1, None),
                ) {
                    Ok(opened) => {
                        tile = Some(opened);
                        read = Some(Instant::now());
                    }
                    Err(error) => tracing::warn!(
                        error = format!("{error:#}"),
                        "sharing without a preview: the card won't open a session for one"
                    ),
                }
            }
            // Nobody is looking at one and none is due, so it goes, and its
            // session goes with it.
            (false, true) => {
                tile = None;
                read = None;
            }
            _ => {}
        }
        if let Some(tile) = &mut tile
            && arrival == wgc::Arrival::Frame
            && read.is_none_or(|last| last.elapsed() >= PREVIEW_EVERY)
        {
            read = Some(Instant::now());
            let taken = {
                let mut sink = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                sink.preview_from_card(
                    tile,
                    d3d11::Texture::new(capture.texture()),
                    capture.content_size(),
                    &mut pixels,
                )
            };
            if let Err(error) = taken {
                tracing::warn!(error = format!("{error:#}"), "sharing without a preview");
            }
        }
    }
    Ok(())
}

/// A capturer sending a screen's, window's or camera's frames to the sink.
struct Capture {
    /// A screen's or window's, which gives a frame when asked.
    capturer: Option<livekit::webrtc::desktop_capturer::DesktopCapturer>,
    /// A camera's, which sends its frames from its own thread.
    _camera: Option<crate::camera::Camera>,
    captured: Arc<Mutex<Captured>>,
}

impl Capture {
    /// Starts capturing `source`, fitting its frames in `max_width` by
    /// `max_height`; on Linux, a screen or window the system's dialog picks.
    fn start(
        source: Source,
        sink: &Arc<Mutex<Sink>>,
        max_width: u32,
        max_height: u32,
    ) -> Result<Self> {
        if let Source::Camera(id) = &source {
            return Self::camera(id, sink, max_width, max_height);
        }
        use livekit::webrtc::desktop_capturer::{
            CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
        };
        // On Linux the system's screen sharing dialog picks a screen or window.
        #[cfg(target_os = "linux")]
        let kind = DesktopCaptureSourceType::Generic;
        #[cfg(not(target_os = "linux"))]
        let kind = match source {
            Source::Window(_) => DesktopCaptureSourceType::Window,
            _ => DesktopCaptureSourceType::Screen,
        };
        let mut options = DesktopCapturerOptions::new(kind);
        options.set_include_cursor(true);
        let mut capturer = DesktopCapturer::new(options)
            .context("Screen sharing isn't available on this system.")?;
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (into, seen) = (sink.clone(), captured.clone());
        // The frames this capture converts and scales, kept for the next.
        let mut scratch = Scratch::default();
        // A screen showing HDR hands its frames over as HDR's codes, so they
        // come through a tonemap on the way to I420; a screen showing SDR hands
        // over what it shows, which needs none. Asked once, here: what a screen
        // shows doesn't change under a share, and the frames say nothing of it
        // themselves.
        let tone = tone_of(&source);
        #[cfg(target_os = "linux")]
        let source = {
            let _ = source;
            None
        };
        #[cfg(not(target_os = "linux"))]
        let source = {
            let mut listed = capturer.get_source_list().into_iter();
            let found = match source {
                Source::Display(id) | Source::Window(id) => listed.find(|listed| listed.id() == id),
                _ => listed.next(),
            };
            Some(found.context("That screen or window is gone.")?)
        };
        capturer.start_capture(source, move |result| match result {
            Ok(frame) => {
                let (width, height) = (frame.width().max(1) as u32, frame.height().max(1) as u32);
                let size = fit(width, height, max_width, max_height);
                to_sink(
                    &into,
                    &seen,
                    &mut scratch,
                    frame.data(),
                    frame.stride(),
                    ((width, height), size),
                    &tone,
                );
            }
            // While the dialog is open, or between frames.
            Err(CaptureError::Temporary) => {}
            Err(CaptureError::Permanent) => {
                seen.lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .failed
                    .get_or_insert_with(|| "Screen sharing stopped.".into());
            }
        });
        Ok(Self {
            capturer: Some(capturer),
            _camera: None,
            captured,
        })
    }

    /// Films with the camera `id` into the sink, at the stream's size and
    /// frame rate at most. Each picture goes into a buffer kept for the next.
    fn camera(id: &str, sink: &Arc<Mutex<Sink>>, max_width: u32, max_height: u32) -> Result<Self> {
        let fps = sink
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .fps;
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (into, seen) = (sink.clone(), captured.clone());
        let mut kept: Option<I420Buffer> = None;
        // The picture scaled for an odd size, kept for the frame after it.
        let mut shared: Option<I420Buffer> = None;
        let camera = crate::camera::Camera::open_at(
            Some(id),
            (max_width, max_height, fps),
            Box::new(move |frame| {
                let size = (frame.width, frame.height);
                let picture = buffer(&mut kept, size.0, size.1);
                copy_planes(frame, picture);
                // The camera fits the stream already; an odd size, which
                // codecs can't take, loses a row or a column. Scaling into
                // the buffer kept for it comes before the sink is locked.
                let fitted = fit(size.0, size.1, max_width, max_height);
                let taken = if fitted == size {
                    into.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .frame(picture)
                } else {
                    let scaled = buffer(&mut shared, fitted.0, fitted.1);
                    scale_into(&planes(picture), scaled, fitted.0, fitted.1);
                    into.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .frame(scaled)
                };
                let mut seen = seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                match taken {
                    Ok(()) => seen.size = Some(fitted),
                    Err(error) => seen.failed = Some(format!("{error:#}")),
                }
            }),
        )?;
        Ok(Self {
            capturer: None,
            _camera: Some(camera),
            captured,
        })
    }

    /// Asks a screen's or window's capturer for a frame; a camera sends its
    /// own.
    fn capture_frame(&mut self) {
        if let Some(capturer) = &mut self.capturer {
            capturer.capture_frame();
        }
    }

    fn seen(&self) -> std::sync::MutexGuard<'_, Captured> {
        self.captured
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Copies a camera's picture into `picture`, of the same size.
fn copy_planes(frame: &crate::picture::I420<'_>, picture: &mut I420Buffer) {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
    let (y_stride, u_stride, v_stride) = picture.strides();
    let into_strides = [y_stride, u_stride, v_stride];
    let (y, u, v) = picture.data_mut();
    for (plane, (from, into)) in [(frame.y, y), (frame.u, u), (frame.v, v)]
        .into_iter()
        .enumerate()
    {
        let (width, height) = if plane == 0 {
            (width, height)
        } else {
            (chroma_width, chroma_height)
        };
        let rows = from
            .chunks(frame.strides[plane] as usize)
            .zip(into.chunks_mut(into_strides[plane] as usize));
        for (from, into) in rows.take(height) {
            into[..width].copy_from_slice(&from[..width]);
        }
    }
}

/// The buffers a capture keeps from one frame to the next: the frame as it
/// came, converted to I420, and the picture it's scaled to for sharing. Each is
/// made again when a frame comes at another size, so a capture holds one
/// frame's worth of each rather than every size it has shown, and lets go of
/// both when sharing stops.
#[derive(Default)]
struct Scratch {
    full: Option<I420Buffer>,
    shared: Option<I420Buffer>,
}

/// The tone a capture of `source` comes through: HDR's when the screen it's of
/// shows HDR, and SDR's otherwise, which is every frame as it came. A camera
/// and a test pattern have no screen to ask about.
fn tone_of(source: &Source) -> Tone {
    match source {
        Source::Display(monitor) => picture::tone_of_screen(*monitor),
        Source::Window(window) => picture::tone_of_window(*window),
        _ => Tone::SDR,
    }
}

/// A captured frame on its way to the sink: converted and scaled in the
/// buffers this capture keeps, then handed over under the sink's lock. The
/// capture's status is noted afterwards, under its own lock: no lock is held
/// while a frame is made, and no frame is sent while the status is held. The
/// two sizes are the frame's own, and the one it's shared at.
fn to_sink(
    sink: &Mutex<Sink>,
    captured: &Mutex<Captured>,
    scratch: &mut Scratch,
    argb: &[u8],
    stride: u32,
    (size, shared): ((u32, u32), (u32, u32)),
    tone: &Tone,
) {
    let full = buffer(&mut scratch.full, size.0, size.1);
    convert(full, argb, stride, tone);
    let taken = if shared == size {
        sink.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .frame(full)
    } else {
        let scaled = buffer(&mut scratch.shared, shared.0, shared.1);
        scale_into(&planes(full), scaled, shared.0, shared.1);
        sink.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .frame(scaled)
    };
    let mut seen = captured
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match taken {
        Ok(()) => seen.size = Some(shared),
        Err(error) => seen.failed = Some(format!("{error:#}")),
    }
}

/// The I420 buffer `kept` holds for pictures `width` by `height`, made in
/// place of one of another size.
fn buffer(kept: &mut Option<I420Buffer>, width: u32, height: u32) -> &mut I420Buffer {
    let picture = kept.get_or_insert_with(|| I420Buffer::new(width, height));
    if (picture.width(), picture.height()) != (width, height) {
        *picture = I420Buffer::new(width, height);
    }
    picture
}

/// `argb` pixels, a row every `stride` bytes, as I420 in `picture`, which is
/// `width` by `height`, through `tone` where the screen they came from shows
/// HDR and as they came where it doesn't.
fn convert(picture: &mut I420Buffer, argb: &[u8], stride: u32, tone: &Tone) {
    if let Some(table) = tone.table() {
        picture::toned_into(picture, argb, stride, table);
        return;
    }
    let (width, height) = (picture.width(), picture.height());
    let (y_stride, u_stride, v_stride) = picture.strides();
    let (y, u, v) = picture.data_mut();
    // libyuv's ARGB is B, G, R, A in memory, as the capturer's frames are.
    yuv_helper::argb_to_i420(
        argb,
        stride,
        y,
        y_stride,
        u,
        u_stride,
        v,
        v_stride,
        width as i32,
        height as i32,
    );
}

/// `picture`'s planes, as the libyuv helpers take them.
fn planes(picture: &I420Buffer) -> I420<'_> {
    let (y, u, v) = picture.data();
    let (y_stride, u_stride, v_stride) = picture.strides();
    I420 {
        width: picture.width(),
        height: picture.height(),
        y,
        u,
        v,
        strides: [y_stride, u_stride, v_stride],
    }
}

/// `picture` scaled to `width` by `height` in `into`, which is that size: a
/// frame scaled down for sharing needs no picture of its own.
fn scale_into(picture: &I420<'_>, into: &mut I420Buffer, width: u32, height: u32) {
    let strides = into.strides();
    let (y, u, v) = into.data_mut();
    picture.scale_into((y, u, v), strides, width, height);
}

/// The I420 picture `to_rgba` scaled into `scratch`, which is `width` by
/// `height`'s planes.
fn previewed(scratch: &[u8], width: u32, height: u32) -> I420<'_> {
    let (w, h) = (width as usize, height as usize);
    let chroma = (w.div_ceil(2), h.div_ceil(2));
    let (y, planes) = scratch.split_at(w * h);
    let (u, v) = planes.split_at(chroma.0 * chroma.1);
    I420 {
        width,
        height,
        y,
        u,
        v: &v[..chroma.0 * chroma.1],
        strides: [width, chroma.0 as u32, chroma.0 as u32],
    }
}

/// A captured frame as an I420 picture of its size.
fn picture_of(frame: &livekit::webrtc::desktop_capturer::DesktopFrame, tone: &Tone) -> I420Buffer {
    let (width, height) = (frame.width().max(1) as u32, frame.height().max(1) as u32);
    let mut picture = I420Buffer::new(width, height);
    convert(&mut picture, frame.data(), frame.stride(), tone);
    picture
}

/// How long [`glance`] waits for a picture of a screen that isn't black,
/// before showing the last it took, or none.
#[cfg(not(target_os = "linux"))]
const GLANCE_WAIT: Duration = Duration::from_millis(1500);

/// Takes a picture of each of `screens`, as [`Job::Glance`] asks. A screen
/// that gives none goes without, and the rest still come.
fn glance(
    screens: &[u64],
    max_width: u32,
    max_height: u32,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) {
    // On Linux the system's dialog shows what there is to share.
    #[cfg(target_os = "linux")]
    let _ = (screens, max_width, max_height, stop, report);
    #[cfg(not(target_os = "linux"))]
    for &id in screens {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match glance_at(id, stop) {
            Ok(Some(mut picture)) => {
                let (width, height) = fit(picture.width(), picture.height(), max_width, max_height);
                if (width, height) != (picture.width(), picture.height()) {
                    picture = picture.scale(width as i32, height as i32);
                }
                let mut rgba = vec![0; (width * height * 4) as usize];
                to_rgba(&picture, &mut rgba);
                report(Report::Screen {
                    id,
                    width,
                    height,
                    rgba,
                });
            }
            Ok(None) => tracing::info!(id, "no picture of the screen came"),
            Err(error) => tracing::warn!(id, "no picture of the screen: {error:#}"),
        }
    }
}

/// A picture of the screen listed by `id`, at its own size.
#[cfg(not(target_os = "linux"))]
fn glance_at(id: u64, stop: &AtomicBool) -> Result<Option<I420Buffer>> {
    use livekit::webrtc::desktop_capturer::{
        CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
    };
    let mut capturer = DesktopCapturer::new(DesktopCapturerOptions::new(
        DesktopCaptureSourceType::Screen,
    ))
    .context("Screen sharing isn't available on this system.")?;
    let source = capturer
        .get_source_list()
        .into_iter()
        .find(|listed| listed.id() == id)
        .context("That screen is gone.")?;
    // What the preview of it shows: the screen's own brightness, whether it's
    // showing HDR or not.
    let tone = picture::tone_of_screen(id);
    // The last picture taken, and whether capture stopped for good.
    let taken = Arc::new(Mutex::new((None::<I420Buffer>, false)));
    let into = taken.clone();
    capturer.start_capture(Some(source), move |result| {
        let mut taken = into.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match result {
            Ok(frame) => taken.0 = Some(picture_of(&frame, &tone)),
            Err(CaptureError::Temporary) => {}
            Err(CaptureError::Permanent) => taken.1 = true,
        }
    });
    // A screen's first picture can come black, before the system has
    // drawn it for the capturer.
    let deadline = Instant::now() + GLANCE_WAIT;
    loop {
        capturer.capture_frame();
        let mut taken = taken
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let lit = taken
            .0
            .as_ref()
            .is_some_and(|picture| !is_black(picture.data().0));
        if lit || taken.1 || Instant::now() >= deadline || stop.load(Ordering::Relaxed) {
            return Ok(taken.0.take());
        }
        drop(taken);
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Pixels in a report as base64: a third bigger than they are, where JSON's
/// array of numbers would be up to four times.
mod base64_pixels {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(pixels: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(pixels))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        // base64 has nothing JSON escapes, so the line lends it as it is.
        let text = <&str>::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}

/// Whether streams encode on the GPU where it has an encoder: Settings'
/// choice, on until the UI says.
static HARDWARE: AtomicBool = AtomicBool::new(true);

/// Encodes streams started from now on with the GPU's encoder, or not.
pub fn set_hardware_encoding(on: bool) {
    HARDWARE.store(on, Ordering::Relaxed);
}

/// Yapfast's handle on a running worker. Dropping it stops the worker.
pub struct Worker {
    /// Which ends the worker; taken when this handle goes, so nothing waits
    /// on it here.
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    /// The place this worker takes among those decoding, given back when it
    /// stops. A worker that decodes no picture has none.
    _place: Option<Place>,
}

/// The workers this Yapfast has decoding pictures: streams watched and videos
/// played. Each is a process of its own, so the number of them is only known
/// here, in the Yapfast that started them, and it's from that number and the
/// machine's cores that a new worker's decoder learns how much it may take.
static DECODERS: AtomicUsize = AtomicUsize::new(0);

/// A worker's place among those decoding, given back when the worker goes.
struct Place {
    /// The threads its decoder may use.
    threads: u32,
}

impl Place {
    /// Takes a place for a worker starting on a machine with `cores`, and
    /// works out the share of its decoder threads that place leaves.
    fn claim(cores: usize) -> Self {
        let streams = DECODERS.fetch_add(1, Ordering::AcqRel) + 1;
        Self {
            threads: crate::decode::share(cores, streams) as u32,
        }
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        DECODERS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// How many reports may wait for the UI: a share sends a status a second, so
/// 16 seconds of a window that stopped reading still fit, and one that stays
/// stopped never grows the queue past this.
const REPORTS_WAITING: usize = 16;

/// Hands `report` to the UI, and says what's left to do with it: a status a
/// later one replaces is dropped when there's no room, which keeps the
/// worker's reader moving and its child from blocking on a full standard
/// output. Everything else waits for room, however long the UI takes over
/// it, so the end of a job is never the report that goes missing.
fn deliver(reports: &mpsc::Sender<Report>, report: Report) -> Delivery {
    match reports.try_send(report) {
        Ok(()) => Delivery::Taken,
        Err(mpsc::error::TrySendError::Full(report)) if report.replaceable() => Delivery::Taken,
        Err(mpsc::error::TrySendError::Full(report)) => Delivery::Waiting(report),
        Err(mpsc::error::TrySendError::Closed(_)) => Delivery::Gone,
    }
}

/// What [`deliver`] left for the caller to do.
enum Delivery {
    /// The UI has it, or a later report says the same.
    Taken,
    /// The queue was full: the caller waits for room and hands it over.
    Waiting(Report),
    /// The UI stopped reading, so there's nobody left to report to.
    Gone,
}

impl Worker {
    /// Starts sharing `source`, with frames for the UI in a new [`Frames`].
    pub fn share(
        source: Source,
        quality: StreamQuality,
    ) -> Result<(Self, Frames, mpsc::Receiver<Report>)> {
        let source = picked(source);
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT)?;
        let (width, height) = bounds(quality.resolution);
        let job = Job::Share {
            source,
            frames: frames.path().context("the video frames file has no name")?,
            width,
            height,
            fps: quality.fps,
            effort: if quality.high_quality {
                Effort::High
            } else {
                Effort::Light
            },
            hardware: HARDWARE.load(Ordering::Relaxed),
        };
        let (worker, reports) = Self::start(&job)?;
        Ok((worker, frames, reports))
    }

    /// Starts taking pictures of `screens` for the share menu, each fitting
    /// `width` by `height`. They come as [`Report::Screen`]s, then
    /// [`Report::Ended`].
    pub fn glance(
        screens: Vec<u64>,
        width: u32,
        height: u32,
    ) -> Result<(Self, mpsc::Receiver<Report>)> {
        Self::start(&Job::Glance {
            screens,
            width,
            height,
        })
    }

    /// Starts watching the stream at `server`, with pictures for the UI in a
    /// new [`Frames`].
    pub fn watch(
        server: &StreamServer,
        speakers: Option<String>,
        deafened: bool,
        volume: f32,
    ) -> Result<(Self, Frames, mpsc::Receiver<Report>)> {
        let frames = Frames::create(WATCH_TILE_WIDTH, WATCH_TILE_HEIGHT)?;
        let job = Job::Watch {
            server: server.clone(),
            frames: frames.path().context("the video frames file has no name")?,
            width: WATCH_TILE_WIDTH,
            height: WATCH_TILE_HEIGHT,
            speakers,
            deafened,
            volume,
        };
        let (worker, reports) = Self::start(&job)?;
        Ok((worker, frames, reports))
    }

    /// Starts playing the video file at `file`, which may still be
    /// downloading, with pictures that fit `width` by `height` for the UI in
    /// a new [`Frames`].
    pub fn play(
        file: PathBuf,
        size: Option<u64>,
        (width, height): (u32, u32),
        speakers: Option<String>,
        volume: f32,
    ) -> Result<(Self, Frames, mpsc::Receiver<Report>)> {
        let frames = Frames::create(WATCH_WIDTH, WATCH_HEIGHT)?;
        let job = Job::Play {
            file,
            size,
            frames: frames.path().context("the video frames file has no name")?,
            width: width.min(WATCH_WIDTH),
            height: height.min(WATCH_HEIGHT),
            speakers,
            volume,
        };
        let (worker, reports) = Self::start(&job)?;
        Ok((worker, frames, reports))
    }

    /// Tells a playing video worker what to do.
    pub fn tell(&mut self, control: &Control) -> Result<()> {
        self.control(control)
    }

    /// Tells a watching worker whether to play the stream's sound.
    pub fn hear(&mut self, hearing: bool) -> Result<()> {
        self.control(&Control::Hear(hearing))
    }

    /// Tells a sharing worker whether the UI shows the preview of what's
    /// shared, which it makes only while one does.
    pub fn preview(&mut self, shown: bool) -> Result<()> {
        self.control(&Control::Preview(shown))
    }

    /// Tells a watching worker whether a window shows its pictures, which it
    /// converts and writes only while one does.
    pub fn shown(&mut self, shown: bool) -> Result<()> {
        self.control(&Control::Shown(shown))
    }

    /// Tells a watching worker the size in pixels of the tile showing its
    /// pictures, which it scales them to; see [`Control::Size`].
    pub fn size(&mut self, width: u32, height: u32) -> Result<()> {
        self.control(&Control::Size { width, height })
    }

    /// Tells a watching worker how loud to play the stream's sound.
    pub fn volume(&mut self, volume: f32) -> Result<()> {
        self.control(&Control::Volume(volume))
    }

    /// Hands the worker the server Discord started the stream on.
    pub fn send(&mut self, server: &StreamServer) -> Result<()> {
        self.control(&Control::Send(server.clone()))
    }

    /// Tells a sharing worker to share `source` in place of what it shares.
    pub fn switch(&mut self, source: Source) -> Result<()> {
        self.control(&Control::Switch(picked(source)))
    }

    fn control(&mut self, control: &Control) -> Result<()> {
        let line = serde_json::to_string(control)?;
        let stdin = self
            .stdin
            .as_mut()
            .context("the stream worker has stopped")?;
        writeln!(stdin, "{line}").context("could not reach the stream worker")?;
        Ok(())
    }

    fn start(job: &Job) -> Result<(Self, mpsc::Receiver<Report>)> {
        // Tests run from their own binary, so they name Yapfast's.
        let program = match std::env::var_os("YAPFAST_STREAM_WORKER") {
            Some(program) => PathBuf::from(program),
            None => running_yapfast()?,
        };
        // A worker that decodes takes its share of the machine, out of the
        // streams this Yapfast already has decoding.
        let place = job.decodes().then(|| {
            let cores = std::thread::available_parallelism().map_or(1, |cores| cores.get());
            Place::claim(cores)
        });
        let mut command = Command::new(program);
        command.arg(ARGUMENT);
        if let Some(place) = &place {
            command.env(THREADS, place.threads.to_string());
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(crate::logging::for_worker())
            .spawn()
            .context("could not start the stream worker")?;
        let mut stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let line = serde_json::to_string(job)?;
        if let Some(stdin) = stdin.as_mut() {
            writeln!(stdin, "{line}").context("could not reach the stream worker")?;
        }
        let (reports, receiver) = mpsc::channel(REPORTS_WAITING);
        let unexpected = match job {
            Job::Play { .. } => "The video stopped unexpectedly.",
            _ => "Screen sharing stopped unexpectedly.",
        };
        std::thread::Builder::new()
            .name("yapfast-stream-reports".into())
            .spawn(move || {
                let mut ended = false;
                for line in stdout
                    .into_iter()
                    .flat_map(|stdout| BufReader::new(stdout).lines().map_while(Result::ok))
                {
                    match serde_json::from_str::<Report>(&line) {
                        Ok(report) => {
                            ended |= matches!(report, Report::Ended { .. });
                            match deliver(&reports, report) {
                                Delivery::Taken => {}
                                Delivery::Waiting(report) => {
                                    if reports.blocking_send(report).is_err() {
                                        return;
                                    }
                                }
                                Delivery::Gone => return,
                            }
                        }
                        Err(error) => tracing::warn!(%error, "unknown stream worker report"),
                    }
                }
                if !ended {
                    // The UI hears of a worker gone quietly: its end is
                    // worth waiting for room for, like any other.
                    let _ = reports.blocking_send(Report::Ended {
                        error: Some(unexpected.into()),
                    });
                }
            })
            .context("could not start the stream worker")?;
        Ok((
            Self {
                child: Some(child),
                stdin,
                _place: place,
            },
            receiver,
        ))
    }
}

/// The Yapfast that's running, to start as the worker. On Linux that's
/// `/proc/self/exe`, which still opens after a rebuild or an update replaced
/// Yapfast's file, where its path names a deleted file; it also keeps the worker
/// the same version as the window it talks to.
fn running_yapfast() -> Result<PathBuf> {
    if cfg!(target_os = "linux") {
        return Ok(PathBuf::from("/proc/self/exe"));
    }
    std::env::current_exe().context("could not find Yapfast to start streaming")
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(mut stdin) = self.stdin.take() {
            let _ = writeln!(stdin, "stop");
        }
        // The worker checks for the stop between frames, so this usually ends
        // it at once; a capture stuck in a system call takes until the wait
        // runs out. Waiting here would hold the call's own loop, which is
        // where a stream stops from, so a thread of its own does the waiting.
        // Closing the worker's input above ends it even if that thread can't
        // start: it reads the stop, then the end of its input.
        let Some(mut child) = self.child.take() else {
            return;
        };
        let stopping = std::thread::Builder::new()
            .name("yapfast-stream-stop".into())
            .spawn(move || {
                let deadline = Instant::now() + STOP_WAIT;
                while Instant::now() < deadline {
                    if !matches!(child.try_wait(), Ok(None)) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                tracing::warn!("the stream worker didn't stop; ending it");
                let _ = child.kill();
                let _ = child.wait();
            });
        if let Err(error) = stopping {
            tracing::warn!(%error, "could not wait for the stream worker to stop");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worker_is_the_running_yapfast() {
        // Here that's the test binary, which lists its tests and exits.
        let program = running_yapfast().unwrap();
        let status = std::process::Command::new(program)
            .args(["--list", "--format", "terse"])
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn a_full_queue_drops_a_status_and_keeps_the_end() {
        let (reports, mut ui) = mpsc::channel(1);
        let first = Report::Sending(StreamStats::default());
        assert!(matches!(deliver(&reports, first), Delivery::Taken));
        let status = deliver(&reports, Report::Sending(StreamStats::default()));
        assert!(
            matches!(status, Delivery::Taken),
            "a status is dropped rather than wait: a later one says the same"
        );
        let ended = deliver(&reports, Report::Ended { error: None });
        assert!(
            matches!(ended, Delivery::Waiting(Report::Ended { .. })),
            "the end waits for room, so a full queue can't hide it"
        );
        assert!(matches!(ui.try_recv(), Ok(Report::Sending(_))), "the first");
        assert!(ui.try_recv().is_err(), "and only the first");
        drop(ui);
        assert!(
            matches!(deliver(&reports, Report::Streaming), Delivery::Gone),
            "and nothing is left to report to"
        );
    }

    #[test]
    fn fits_frames_with_even_sizes() {
        assert_eq!(fit(1920, 1080, 1280, 720), (1280, 720));
        assert_eq!(fit(3440, 1440, 1280, 720), (1280, 534));
        assert_eq!(fit(801, 601, 1280, 720), (800, 600), "never larger");
    }

    /// Only a worker that decodes pictures takes a share of the machine's
    /// decoder threads: a watched stream and a played video do, where a shared
    /// screen only encodes and a glance only takes a picture.
    #[test]
    fn only_a_worker_that_decodes_takes_a_share() {
        let frames = PathBuf::from("frames");
        let server = StreamServer {
            stream_key: "key".into(),
            user_id: "1".into(),
            session_id: "2".into(),
            rtc_server_id: "3".into(),
            token: "token".into(),
            endpoint: "wss://discord.test".into(),
        };
        let watch = Job::Watch {
            server,
            frames: frames.clone(),
            width: WATCH_WIDTH,
            height: WATCH_HEIGHT,
            speakers: None,
            deafened: false,
            volume: 1.0,
        };
        assert!(watch.decodes(), "a watched stream decodes");
        let played = Job::Play {
            file: PathBuf::from("video"),
            size: None,
            frames: frames.clone(),
            width: WATCH_WIDTH,
            height: WATCH_HEIGHT,
            speakers: None,
            volume: 1.0,
        };
        assert!(played.decodes(), "a played video decodes");
        let shared = Job::Share {
            source: Source::Screen,
            frames: frames.clone(),
            width: SHARE_WIDTH,
            height: SHARE_HEIGHT,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
        };
        assert!(!shared.decodes(), "a shared screen only encodes");
        let glance = Job::Glance {
            screens: vec![1],
            width: 320,
            height: 180,
        };
        assert!(!glance.decodes(), "a glance only takes a picture");
    }

    /// A picture a newer one replaces before the screen could draw it isn't
    /// converted, scaled or shown: the watch drains what queued and shows the
    /// newest of it. A watch that keeps up shows every frame, and a watch
    /// that hasn't shown anything yet always shows the first.
    #[test]
    fn a_picture_a_newer_one_replaces_isnt_shown() {
        let millis = Duration::from_millis;
        let shown = Instant::now();
        // Nothing shown yet: the first picture reaches the screen whatever
        // came in behind it, or a stream that outruns its decoder would show
        // nothing at all.
        assert_eq!(worth_showing(true, None, shown, false), Worth::Show);
        assert_eq!(worth_showing(true, None, shown, true), Worth::Show);
        // Pictures decoded back to back — what queued while the decoder was
        // slow — are pictures no screen draws: the last of them is shown.
        assert_eq!(
            worth_showing(true, Some(shown), shown + millis(1), true),
            Worth::Replaced
        );
        assert_eq!(
            worth_showing(true, Some(shown), shown + millis(7), true),
            Worth::Replaced
        );
        assert_eq!(
            worth_showing(true, Some(shown), shown + millis(8), true),
            Worth::Show
        );
        // A frame period later — 30 frames a second — it's the newest
        // picture there could be, so it's shown.
        assert_eq!(
            worth_showing(true, Some(shown), shown + millis(33), true),
            Worth::Show
        );
        // With nothing newer in hand the decoder is keeping up: every frame
        // is shown, however fast they come.
        assert_eq!(worth_showing(true, Some(shown), shown, false), Worth::Show);
    }

    /// A hidden or minimised window shows no picture, so none is converted or
    /// written while it's hidden, however long the decoder has been running;
    /// the frames themselves are decoded all the same. When the window comes
    /// back, the picture decoded next is the one shown, at once: the last one
    /// that reached the screen was long enough ago to be replaced by any.
    #[test]
    fn a_hidden_window_shows_no_picture() {
        let millis = Duration::from_millis;
        let shown = Instant::now();
        // Hidden, nothing at all is made of a picture: not the first, not one
        // with nothing behind it, and not the newest of a queue either.
        assert_eq!(worth_showing(false, None, shown, false), Worth::Unseen);
        assert_eq!(
            worth_showing(false, Some(shown), shown + millis(33), false),
            Worth::Unseen,
        );
        assert_eq!(
            worth_showing(false, Some(shown), shown + millis(33), true),
            Worth::Unseen,
        );
        // The frame is decoded before this is asked, in the watch's loop: what
        // a hidden window skips is the conversion and the write, not the
        // decode. So the pictures after the window comes back are made from a
        // chain the decoder kept, and the next one decoded is shown without
        // waiting for anything: the screen drew its last a minute ago.
        assert_eq!(
            worth_showing(true, Some(shown), shown + Duration::from_secs(60), true),
            Worth::Show,
        );
    }

    /// What a watched stream is written at: a picture larger than the tile
    /// showing it is scaled down to the tile, and one smaller keeps its own
    /// size, so a stream shown in a small tile costs a small frame and a
    /// small stream no more than it is.
    #[test]
    fn a_stream_is_written_at_the_size_its_tile_shows_it() {
        let frames = Frames::create(32, 18).unwrap();
        let mut writer = Writer::open(&frames.path().unwrap()).unwrap();
        frames.unlink();
        let mut scratch = vec![];
        let picture = |width, height| {
            let mut picture = I420Buffer::new(width, height);
            let (y, u, v) = picture.data_mut();
            y.fill(126);
            u.fill(128);
            v.fill(128);
            picture
        };
        // A 64 by 36 picture in a 32 by 18 tile is scaled down to the tile.
        let large = picture(64, 36);
        let shown = planes(&large);
        let size = write_picture(&mut writer, &shown, &mut scratch, (32, 18));
        assert_eq!(size, (32, 18));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.len())),
            Some((32, 18, 32 * 18 * 4)),
            "the tile's RGBA, row for row"
        );
        // A 16 by 10 picture in that tile keeps its own size: nothing scales
        // a stream up to the tile it shows in.
        let small = picture(16, 10);
        let shown = planes(&small);
        let size = write_picture(&mut writer, &shown, &mut scratch, (32, 18));
        assert_eq!(size, (16, 10));
        assert_eq!(
            frames.take(|width, height, pixels| (width, height, pixels.len())),
            Some((16, 10, 16 * 10 * 4))
        );
    }

    /// A tile's size says what size a stream shows at: none while no tile
    /// shows it, and never more than a stream is shown at, however large the
    /// window is, so a 4K tile doesn't grow the frames for a 1080p stream.
    #[test]
    fn a_tile_says_how_large_a_stream_shows() {
        assert_eq!(tile_size((0, 0)), None, "no tile shows it");
        assert_eq!(tile_size((0, 270)), None, "half a size says nothing");
        assert_eq!(tile_size((480, 270)), Some((480, 270)));
        assert_eq!(
            tile_size((3840, 2160)),
            Some((WATCH_WIDTH, WATCH_HEIGHT)),
            "no stream is shown larger than it is"
        );
    }

    /// Frames at 30 fps are due every 33⅓ ms: on time, a loop waits for the
    /// frame in line; behind, it makes the latest frame due and skips the rest.
    #[test]
    fn skips_the_frames_a_stall_swallowed() {
        let started = Instant::now();
        let millis = Duration::from_millis;
        // The first frame goes at once, so a stream shows without the wait.
        assert_eq!(pace(started, 0, 30, started), (0, Some(Duration::ZERO)));
        // On time, it waits out the rest of the frame's interval.
        assert_eq!(
            pace(started, 2, 30, started + millis(40)),
            (2, Some(Duration::from_secs(2) / 30 - millis(40)))
        );
        // A little late still makes the frame that was due.
        assert_eq!(pace(started, 2, 30, started + millis(70)), (2, None));
        // A quarter of a second late makes frame 7, not frames 1 to 7.
        assert_eq!(pace(started, 1, 30, started + millis(250)), (7, None));
        // After a skipped burst the cadence carries on from the clock.
        assert_eq!(
            pace(started, 8, 30, started + millis(250)),
            (8, Some(Duration::from_secs(8) / 30 - millis(250)))
        );
    }

    /// 60 fps is a frame every 16⅔ ms, which no whole millisecond marks.
    #[test]
    fn paces_a_frame_rate_that_no_millisecond_marks() {
        let started = Instant::now();
        let millis = Duration::from_millis;
        assert_eq!(pace(started, 0, 60, started + millis(16)), (0, None));
        assert_eq!(
            pace(started, 1, 60, started + millis(16)),
            (1, Some(Duration::from_secs(1) / 60 - millis(16)))
        );
        // Frame 4 came due at 66⅔ ms; frames 1 to 3 were missed.
        assert_eq!(pace(started, 1, 60, started + millis(67)), (4, None));
    }

    /// Whatever the clock says, a paced loop never makes a frame early, never
    /// goes back to one it skipped, and hands out the whole cadence in order.
    #[test]
    fn frames_follow_the_clock() {
        let started = Instant::now();
        for fps in [1u32, 30, 60] {
            let mut slot = 0u64;
            for millis in (0..1_100u64).step_by(7) {
                let now = started + Duration::from_millis(millis);
                let (due, wait) = pace(started, slot, fps, now);
                let deadline = started + Duration::from_secs(due) / fps;
                assert!(due >= slot, "{fps} fps went back to {due} at {millis} ms");
                match wait {
                    // On time, it waits for the frame in line to be due.
                    Some(wait) => {
                        assert_eq!(due, slot);
                        assert_eq!(now + wait, deadline, "{fps} fps at {millis} ms");
                    }
                    // Behind, it makes the latest frame due, and not before.
                    None => {
                        assert!(deadline <= now, "{fps} fps early at {millis} ms");
                        // Deadlines are whole nanoseconds, so rounding can leave
                        // a frame due at this very instant for the next call.
                        assert!(
                            started + Duration::from_secs(due + 2) / fps > now,
                            "{fps} fps left two frames due at {millis} ms"
                        );
                    }
                }
                slot = due + 1;
            }
        }
    }

    #[test]
    fn keeps_the_edges_of_a_frame_rate() {
        let started = Instant::now();
        // A job's rate is clamped to 1–60; without one there's no cadence.
        assert_eq!(pace(started, 3, 0, started), (3, None));
        // One frame a second starts at once and waits its second after.
        assert_eq!(pace(started, 0, 1, started), (0, Some(Duration::ZERO)));
        assert_eq!(
            pace(started, 1, 1, started + Duration::from_millis(200)),
            (1, Some(Duration::from_millis(800)))
        );
    }

    #[test]
    fn stops_a_paced_loop() {
        // Begun two seconds ago, so every frame it should have made is due and
        // the loop has no waiting to do.
        let started = Instant::now() - Duration::from_secs(2);
        let stop = AtomicBool::new(true);
        let mut slot = 0u64;
        assert_eq!(next_frame(started, &mut slot, 30, &stop), None);
        assert_eq!(slot, 0, "a stopped loop keeps its place");
        stop.store(false, Ordering::Relaxed);
        let frame = next_frame(started, &mut slot, 30, &stop).expect("not stopped");
        assert!(frame >= 60, "the 2 seconds it missed are skipped");
        assert_eq!(slot, frame + 1, "and the loop goes on from there");
    }

    #[test]
    fn aims_for_discords_bitrates() {
        assert_eq!(kbps(720, None), 3500);
        assert_eq!(kbps(1080, None), 9000);
        assert_eq!(kbps(1080, Some(4_000_000)), 4000, "what the path carries");
        assert_eq!(kbps(720, Some(9_000_000)), 3500, "never past the cap");
        assert_eq!(kbps(1080, Some(100_000)), 500, "nor below a screen's floor");
    }

    /// With the GPU, AV1 and H265 before H264 where they open, and an
    /// encoder for the first; without it, H264 alone, on x264.
    #[test]
    fn offers_the_gpus_codecs_first() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_shown: false,
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: true,
            footage: Footage::InMemory,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
        };
        let (offers, encoder) = sink.offers(320, 180, None).unwrap();
        eprintln!("offering {offers:?}, encoding with {}", encoder.engine());
        assert_eq!(offers.last(), Some(&Codec::H264));
        let mut order = [Codec::Av1, Codec::H265, Codec::H264].iter();
        assert!(
            offers.iter().all(|codec| order.any(|next| next == codec)),
            "in order"
        );
        assert_eq!(encoder.codec(), offers[0]);
        sink.hardware = false;
        let (offers, encoder) = sink.offers(320, 180, None).unwrap();
        assert_eq!((offers, encoder.engine()), (vec![Codec::H264], "x264"));
    }

    /// A share whose frames come from the card encodes with the card's encoder,
    /// on the device the capture fills its textures on: what reads a texture is
    /// NVENC on one D3D11 device, which is another encoder of FFmpeg's, reached
    /// another way, and the device it reads on is the capture's own.
    #[cfg(wgc)]
    #[test]
    fn a_share_from_the_card_encodes_on_the_card() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_shown: false,
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: true,
            footage: Footage::OnTheCard,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
        };
        // The device the stream's own session opens on is the capture's, which
        // a test can only ask for by capturing something. A machine with no
        // screen has no card's device to check, and says so.
        let Ok(capture) = wgc::Capture::open(wgc::Monitor::PRIMARY) else {
            eprintln!("no screen here, so there is no capture to read from");
            return;
        };
        // A machine without NVENC on that device has nothing to check here,
        // and says so the way a stream on it would: this is the error that
        // keeps that machine sharing from the processor.
        let Ok((offers, encoder)) = sink.offers(320, 180, Some(capture.device())) else {
            eprintln!("no NVENC here, so there is no card's encoder to check");
            return;
        };
        eprintln!("offering {offers:?}, encoding with {}", encoder.engine());
        let Encoding::Texture(encoder) = encoder else {
            panic!("{} doesn't read textures", encoder.engine());
        };
        assert!(encoder.engine().ends_with("/d3d11"), "{}", encoder.engine());
        assert_eq!(encoder.size(), (320, 180));
        assert_eq!(
            encoder.device(),
            capture.device(),
            "the session reads on the device the capture fills its textures on"
        );
    }

    /// Everything that keeps a share on the processor says so, and names which
    /// one it was: a window and a camera aren't screens, a test pattern isn't
    /// captured at all, and a stream set to encode on the processor doesn't look
    /// at the card.
    #[cfg(wgc)]
    #[test]
    fn a_share_the_card_cannot_capture_says_why() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_shown: false,
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: true,
            footage: Footage::InMemory,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
        };
        let bounds = (SHARE_WIDTH, SHARE_HEIGHT);
        for (source, why) in [
            (Source::Window(7), "a window"),
            (Source::Camera("none".into()), "a camera"),
            (Source::TestPattern, "a test pattern"),
        ] {
            let Err(error) = card_capture(&source, &sink, bounds) else {
                panic!("{source:?} isn't captured from the card");
            };
            let said = format!("{error:#}");
            assert!(said.contains(why), "{said}");
        }
        sink.hardware = false;
        let Err(error) = card_capture(&Source::Display(0), &sink, bounds) else {
            panic!("a stream set to the processor still captured from the card");
        };
        assert!(
            format!("{error:#}").contains("processor"),
            "the setting itself keeps it there"
        );
    }

    /// Whether a screen's texture and an NVENC session can be on one device, as
    /// the share from the card needs them to be. It is a fact about the machine
    /// and not about this code, so it is asked here and printed: a session reads
    /// a texture only on the device it was opened on, and this one is opened on
    /// the capture's.
    ///
    /// The session is opened at the size the stream sends rather than at the
    /// screen's, since that is the size the stream's own session opens at, and
    /// the point is the device rather than the size.
    #[cfg(wgc)]
    #[test]
    #[ignore = "needs a screen and an NVIDIA card"]
    fn the_capture_and_the_encoder_are_on_one_device() {
        let capture = wgc::Capture::open(wgc::Monitor::PRIMARY).expect("a capture of the screen");
        eprintln!("{}", capture.describe());
        let (width, height) = capture.content_size();
        eprintln!("the screen is {width}x{height}");
        let (sent_width, sent_height) = fit(width, height, SHARE_WIDTH, SHARE_HEIGHT);
        eprintln!("a stream of it sends {sent_width}x{sent_height}");
        let Ok(encoder) = d3d11::D3d11Encoder::open_on(
            capture.device(),
            stream_connection::SEND_CODEC,
            sent_width,
            sent_height,
            30,
            kbps(sent_height, None),
        ) else {
            eprintln!(
                "and this machine has no H264 NVENC session on the device the capture fills its \
                 textures on"
            );
            return;
        };
        eprintln!(
            "capture fills textures on {:p}, the session reads them on {:p}",
            capture.device(),
            encoder.device()
        );
        assert_eq!(
            capture.device(),
            encoder.device(),
            "a share from the card needs the capture's device to be the session's: NVENC reads a \
             texture only on the one it opened on"
        );
        assert_eq!(encoder.size(), (sent_width, sent_height));
    }

    /// The fake Discord's fixture: Alice, in the guild, in the Lounge. The ids
    /// are `testing/fake_discord.ts`'s, which is where a stream's key and the
    /// account watching it come from.
    #[cfg(wgc)]
    const GUILD: &str = "1400000000000000100";
    #[cfg(wgc)]
    const LOUNGE: &str = "1400000000000000103";
    #[cfg(wgc)]
    const ALICE: &str = "1400000000000000001";

    /// What a real share costs the processor a frame, measured on the machine
    /// that runs it: the primary display captured, encoded at 1080p30 and sent
    /// to the fake Discord, once from the card and once through libwebrtc's
    /// capturer and x264. The process's own kernel and user time is counted
    /// across each leg and divided by the frames the fake says arrived, so
    /// waiting on the compositor costs nothing and nothing that didn't arrive is
    /// counted.
    ///
    /// It prints the numbers rather than asserting them: what a card or a
    /// processor costs is the machine's answer, and this is here to be run on
    /// the machine whose answer it is.
    #[cfg(wgc)]
    #[test]
    #[ignore = "needs a screen, an NVIDIA card, bun and a python3 with tkinter"]
    fn a_real_share_costs_what_it_costs_a_frame() {
        // The worker's own log line says which path a share took and why, which
        // is the half of this a number can't say.
        crate::logging::init_to_stderr();
        let fake = Fake::start();
        let _drawing = Animated::start();
        // The window takes a moment to appear, and a leg measured before it does
        // sees a screen that isn't drawing.
        std::thread::sleep(Duration::from_millis(1500));
        for (hardware, path) in [(true, "the card"), (false, "the processor")] {
            let (cpu, frames, stats) = share_for(&fake, hardware, Duration::from_secs(20));
            assert!(
                frames >= 100,
                "{path} sent {frames} frames while the screen was measured: the screen wasn't \
                 drawing, so there is no number for this path here"
            );
            // The card's path is the one whose encoder reads the capture's
            // texture, which it says by naming the D3D11 backend; anything else
            // is the processor's path, and isn't the number this test is for.
            if hardware {
                assert!(
                    stats
                        .as_ref()
                        .is_some_and(|stats| stats.engine.ends_with("/d3d11")),
                    "the card's path didn't engage: the stream says {:?}, and the log above says \
                     why",
                    stats.map(|stats| stats.engine)
                );
            }
            eprintln!(
                "{path}: {:.3} ms of processor a frame over {frames} frames{}",
                cpu / frames.max(1) as f64,
                stats.map_or(String::new(), |stats| format!(
                    ", by {} ({} {}x{} at {} fps, {} kbit/s sent)",
                    stats.engine,
                    stats.codec,
                    stats.width,
                    stats.height,
                    stats.fps,
                    stats.sent_kbps
                ))
            );
        }
    }

    /// Runs one share of the primary display against `fake` for `measure`, and
    /// answers the processor time it took in milliseconds, the frames the fake
    /// says arrived in that time, and the last statistics the worker reported.
    #[cfg(wgc)]
    fn share_for(
        fake: &Fake,
        hardware: bool,
        measure: Duration,
    ) -> (f64, u64, Option<StreamStats>) {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (input, mut feed) = std::io::pipe().unwrap();
        let (reports, heard) = channel::channel();
        let serving = std::thread::spawn(move || {
            serve(Box::new(BufReader::new(input)), &mut |report| {
                let _ = reports.send(report);
            });
        });
        let job = Job::Share {
            source: Source::Screen,
            frames: frames.path().unwrap(),
            width: SHARE_WIDTH,
            height: SHARE_HEIGHT,
            fps: 30,
            effort: Effort::Light,
            hardware,
        };
        writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
        assert_eq!(
            heard
                .recv_timeout(Duration::from_secs(10))
                .expect("the worker's first report"),
            Report::Ready
        );
        // The UI shows the share's preview from the moment sharing starts, which
        // is the card's own session for a tile, so a measurement has it on
        // rather than measuring a share nobody is looking at.
        writeln!(
            feed,
            "{}",
            serde_json::to_string(&Control::Preview(true)).unwrap()
        )
        .unwrap();
        let key = format!("guild:{GUILD}:{LOUNGE}:{ALICE}");
        let server = StreamServer {
            stream_key: key.clone(),
            user_id: ALICE.to_owned(),
            session_id: "measure".to_owned(),
            rtc_server_id: "fake-rtc".to_owned(),
            token: format!("stream-token-{key}"),
            endpoint: fake.voice(),
        };
        writeln!(
            feed,
            "{}",
            serde_json::to_string(&Control::Send(server)).unwrap()
        )
        .unwrap();
        // Nothing is encoded before the stream is live, which the worker reports
        // once the sender has connected: only the measuring waits for it.
        loop {
            match heard.recv_timeout(Duration::from_secs(30)) {
                Ok(Report::Streaming) => break,
                Ok(Report::Ended { error }) => {
                    panic!("the share ended before it went live: {error:?}")
                }
                Ok(_) => {}
                Err(error) => panic!("the stream never went live: {error}"),
            }
        }
        let started = (cpu_millis(), fake.frames(&key));
        std::thread::sleep(measure);
        let (cpu, sent) = (cpu_millis(), fake.frames(&key));
        // What the worker says about the leg it just ran: the engine that made
        // the picture is what says which path engaged.
        let mut stats = None;
        while let Ok(report) = heard.try_recv() {
            if let Report::Sending(sending) = report {
                stats = Some(sending);
            }
        }
        writeln!(feed, "stop").unwrap();
        serving.join().unwrap();
        (cpu - started.0, sent.saturating_sub(started.1), stats)
    }

    /// The fake Discord, on a port of its own, with the counters a measurement
    /// reads: the only Discord a measurement here may talk to.
    #[cfg(wgc)]
    struct Fake {
        child: std::process::Child,
        address: String,
    }

    #[cfg(wgc)]
    impl Fake {
        fn start() -> Self {
            let port = std::net::TcpListener::bind("127.0.0.1:0")
                .expect("a free port")
                .local_addr()
                .unwrap()
                .port();
            let log = std::fs::File::create(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../target/fake-discord-measure.log"
            ))
            .unwrap();
            let child = std::process::Command::new("bun")
                .arg("testing/fake_discord.ts")
                .args(["--port", &port.to_string()])
                .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .expect("bun, which runs the fake Discord");
            let fake = Self {
                child,
                address: format!("http://127.0.0.1:{port}"),
            };
            for _ in 0..100 {
                if get(&format!("{}/_control/health", fake.address)).is_some() {
                    return fake;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            panic!("the fake Discord didn't answer; see target/fake-discord-measure.log");
        }

        /// Where a stream's connection reaches the fake's voice server.
        fn voice(&self) -> String {
            format!("{}/voice", self.address.replacen("http", "ws", 1))
        }

        /// How many frames reached the fake on `key` since it started.
        fn frames(&self, key: &str) -> u64 {
            let answer = get(&format!("{}/_control/video", self.address))
                .expect("the fake's video counters");
            let body = answer
                .split_once("\r\n\r\n")
                .map_or(answer.as_str(), |(_, body)| body);
            let counters: serde_json::Value =
                serde_json::from_str(body).expect("what the fake received");
            counters["received"]
                .as_array()
                .into_iter()
                .flatten()
                // The fake keeps a record for every connection, and a leg's
                // own is the last one on its key: an earlier leg's is still
                // here.
                .rev()
                .find(|received| received["stream"].as_str() == Some(key))
                .map_or(0, |received| received["frames"].as_u64().unwrap_or(0))
        }
    }

    #[cfg(wgc)]
    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// A GET of an `http://` URL, answered whole: what the fake's control
    /// endpoints need, without an HTTP client in a test's dependencies.
    #[cfg(wgc)]
    fn get(url: &str) -> Option<String> {
        let rest = url.strip_prefix("http://")?;
        let (host, path) = rest.split_once('/')?;
        let mut stream = std::net::TcpStream::connect(host).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
        write!(
            stream,
            "GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        )
        .ok()?;
        let mut answer = String::new();
        std::io::Read::read_to_string(&mut stream, &mut answer).ok()?;
        // A connection refused while the fake starts is an answer without a
        // status, which isn't one.
        answer.starts_with("HTTP/1.1 200").then_some(answer)
    }

    /// A window that keeps drawing while a leg runs: Windows Graphics Capture
    /// and libwebrtc's capturer both hand over a frame only when the screen
    /// changed, so a still desktop would measure how often nothing happened.
    #[cfg(wgc)]
    struct Animated {
        child: std::process::Child,
    }

    #[cfg(wgc)]
    impl Animated {
        fn start() -> Self {
            let script = r##"
import tkinter as tk
root = tk.Tk()
root.overrideredirect(True)
root.attributes("-topmost", True)
root.geometry("640x360+20+20")
canvas = tk.Canvas(root, bg="black", highlightthickness=0)
canvas.pack(fill="both", expand=True)
step = [0]
def tick():
    step[0] = (step[0] + 9) % 580
    canvas.delete("all")
    canvas.create_rectangle(step[0], 20, step[0] + 60, 340, fill="#ff00ff", outline="")
    canvas.create_rectangle(580 - step[0], 20, 640 - step[0], 340, fill="#00ff88", outline="")
    root.after(16, tick)
tick()
root.mainloop()
"##;
            Self {
                child: std::process::Command::new("python3")
                    .args(["-c", script])
                    .spawn()
                    .expect("python3 with tkinter, which draws while a leg is measured"),
            }
        }
    }

    #[cfg(wgc)]
    impl Drop for Animated {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// The process's own kernel and user time, in milliseconds, as the
    /// measurements in `docs/measurements` count it: Windows charges it in
    /// 15.6 ms ticks, so a leg has to run long enough for the ticks to add up.
    #[cfg(wgc)]
    fn cpu_millis() -> f64 {
        #[repr(C)]
        struct FileTime {
            low: u32,
            high: u32,
        }
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut c_void;
            fn GetProcessTimes(
                process: *mut c_void,
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

    #[test]
    fn previews_what_it_sends_at_a_smaller_size() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_shown: true,
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
            footage: Footage::InMemory,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
        };
        // A 1080p picture, mid-grey: luma and chroma halfway.
        let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
        let (y, u, v) = picture.data_mut();
        y.fill(126);
        u.fill(128);
        v.fill(128);
        sink.frame(&picture).unwrap();
        let (width, height, first) = frames
            .take(|width, height, pixels| (width, height, pixels[..4].to_vec()))
            .unwrap();
        assert_eq!((width, height), (PREVIEW_WIDTH, PREVIEW_HEIGHT));
        assert!(
            first[..3].iter().all(|&level| level.abs_diff(128) < 4),
            "{first:?}"
        );
        assert_eq!(first[3], 255);
    }

    /// A frame is scaled and converted for the UI only while the UI shows the
    /// preview, and the first frame after it says so has one.
    #[test]
    fn previews_only_while_the_ui_shows_one() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_shown: false,
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
            footage: Footage::InMemory,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
        };
        let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
        let (y, u, v) = picture.data_mut();
        y.fill(126);
        u.fill(128);
        v.fill(128);
        sink.frame(&picture).unwrap();
        assert_eq!(frames.take(|_, _, _| ()), None, "nothing shows it");
        let mut report = |_| {};
        // A share started while the worker thought nothing watched it: the
        // control comes, and the next frame has a picture for the UI.
        controls.send(Control::Preview(true)).unwrap();
        sink.check(&mut report).unwrap();
        sink.frame(&picture).unwrap();
        let (width, height) = frames
            .take(|width, height, _| (width, height))
            .expect("the frame after the control");
        assert_eq!((width, height), (PREVIEW_WIDTH, PREVIEW_HEIGHT));
        controls.send(Control::Preview(false)).unwrap();
        sink.check(&mut report).unwrap();
        sink.frame(&picture).unwrap();
        assert_eq!(
            frames.take(|_, _, _| ()),
            None,
            "and stops once nothing does"
        );
    }

    /// Discord's preview of the stream is still made, and its clock kept, while
    /// the UI shows no preview of it.
    #[test]
    fn makes_discords_preview_while_the_ui_shows_none() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_shown: false,
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
            footage: Footage::InMemory,
            started: Instant::now(),
            streaming: false,
            preview_due: Some(Instant::now()),
            thumbnail: None,
            lost: None,
        };
        let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
        let (y, u, v) = picture.data_mut();
        y.fill(126);
        u.fill(128);
        v.fill(128);
        sink.frame(&picture).unwrap();
        assert!(sink.thumbnail.take().is_some(), "a preview for Discord");
        assert!(
            sink.preview_due.is_some_and(|due| due > Instant::now()),
            "the next one isn't due for minutes"
        );
    }

    #[test]
    fn switches_to_the_last_source_asked_for() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_shown: false,
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
            footage: Footage::InMemory,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
        };
        let mut report = |_| {};
        assert_eq!(sink.check(&mut report).unwrap(), None);
        controls.send(Control::Switch(Source::Display(1))).unwrap();
        controls.send(Control::Switch(Source::Window(2))).unwrap();
        assert_eq!(sink.check(&mut report).unwrap(), Some(Source::Window(2)));
        assert_eq!(sink.check(&mut report).unwrap(), None);
    }

    #[test]
    fn previews_a_stream_once_its_picture_shows() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_shown: false,
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
            footage: Footage::InMemory,
            started: Instant::now(),
            streaming: false,
            preview_due: Some(Instant::now()),
            thumbnail: None,
            lost: None,
        };
        let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
        let (y, u, v) = picture.data_mut();
        y.fill(16);
        u.fill(128);
        v.fill(128);
        sink.frame(&picture).unwrap();
        assert_eq!(sink.thumbnail, None, "not while the screen is black");
        picture.data_mut().0.fill(126);
        sink.frame(&picture).unwrap();
        let thumbnail = sink.thumbnail.take().expect("a preview");
        let jpeg = thumbnail
            .strip_prefix("data:image/jpeg;base64,")
            .map(|data| {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .unwrap()
            })
            .unwrap();
        let shown = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((shown.width(), shown.height()), (512, 288));
        sink.frame(&picture).unwrap();
        assert_eq!(sink.thumbnail, None, "the next one comes minutes later");
    }

    /// A capture's kept buffer holds the frame just converted, not the one
    /// before it, and is the same buffer: a frame costs no picture of its own.
    #[test]
    fn a_kept_picture_holds_the_new_frame() {
        // Two pixels by two, a row every eight bytes: black, then white.
        let mut argb = vec![0; 2 * 2 * 4];
        let mut kept = None;
        let picture = buffer(&mut kept, 2, 2);
        convert(picture, &argb, 8, &Tone::SDR);
        let (dark, address) = {
            let held = kept.as_ref().expect("made above");
            (held.data().0[0], held.data().0.as_ptr())
        };
        argb.fill(255);
        let picture = buffer(&mut kept, 2, 2);
        convert(picture, &argb, 8, &Tone::SDR);
        let held = kept.as_ref().expect("made above");
        assert!(dark <= 20, "black's luma is {dark}");
        assert!(
            held.data().0.iter().all(|&luma| luma >= 230),
            "white's luma is {:?}",
            held.data().0
        );
        assert_eq!(address, held.data().0.as_ptr(), "the same buffer");
    }

    /// A frame from a screen showing SDR reaches the encoder as libyuv made
    /// it, byte for byte: a share of an SDR screen costs what it did, whatever
    /// the curve for an HDR screen's frames would have been.
    #[test]
    fn an_sdr_frame_is_converted_as_libyuv_makes_it() {
        // Eight pixels by four, a row every thirty-two bytes, of every colour.
        let argb: Vec<u8> = (0..8 * 4 * 4)
            .map(|index| (index * 7 % 251) as u8)
            .collect();
        let mut expected = I420Buffer::new(8, 4);
        {
            let (y_stride, u_stride, v_stride) = expected.strides();
            let (y, u, v) = expected.data_mut();
            yuv_helper::argb_to_i420(&argb, 32, y, y_stride, u, u_stride, v, v_stride, 8, 4);
        }
        let mut converted = I420Buffer::new(8, 4);
        convert(&mut converted, &argb, 32, &Tone::SDR);
        assert_eq!(converted.data().0, expected.data().0, "luma");
        assert_eq!(converted.data().1, expected.data().1, "chroma");
        assert_eq!(converted.data().2, expected.data().2, "chroma");
    }

    /// A frame from a screen showing HDR comes through the curve into the same
    /// buffer and planes, with no picture of its own: the code a screen showing
    /// HDR writes SDR's white as shows as the white an SDR screen has.
    #[test]
    fn an_hdr_frame_comes_through_the_curve() {
        // Four pixels by two, a row every sixteen bytes.
        let argb = [149u8; 4 * 2 * 4];
        let mut picture = I420Buffer::new(4, 2);
        convert(&mut picture, &argb, 16, &Tone::pq());
        let (y, u, v) = picture.data();
        assert!(
            y.iter().all(|&luma| luma.abs_diff(209) <= 6),
            "white's luma is {y:?}"
        );
        assert!(
            u.iter().chain(v).all(|&level| level.abs_diff(128) <= 2),
            "white has no tint: {u:?}, {v:?}"
        );
    }

    /// A picture scaled down for sharing lands in the buffer kept for it, at
    /// the size shared, frame after frame, and a stream of another size gets a
    /// buffer of its own.
    #[test]
    fn a_kept_scale_holds_each_new_frame() {
        // Five by three, so both chroma planes are odd too.
        let mut source = I420Buffer::new(5, 3);
        let fill = |source: &mut I420Buffer, luma: u8| {
            let (y, u, v) = source.data_mut();
            y.fill(luma);
            u.fill(128);
            v.fill(128);
        };
        fill(&mut source, 16);
        let mut kept = None;
        let into = buffer(&mut kept, 4, 2);
        scale_into(&planes(&source), into, 4, 2);
        assert_eq!((into.width(), into.height()), (4, 2));
        assert!(into.data().0.iter().all(|&luma| luma <= 20), "black");
        let address = into.data().0.as_ptr();
        fill(&mut source, 235);
        let into = buffer(&mut kept, 4, 2);
        scale_into(&planes(&source), into, 4, 2);
        assert!(into.data().0.iter().all(|&luma| luma >= 230), "white");
        assert_eq!(address, into.data().0.as_ptr(), "the same buffer");
        let into = buffer(&mut kept, 6, 2);
        assert_eq!((into.width(), into.height()), (6, 2));
        assert_ne!(address, into.data().0.as_ptr(), "another buffer");
    }

    /// The preview is made anew for each frame, in the buffer kept for it: the
    /// second frame shows, not the first.
    #[test]
    fn previews_each_frame_as_it_comes() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_shown: true,
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
            footage: Footage::InMemory,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
        };
        let mut picture = I420Buffer::new(SHARE_WIDTH, SHARE_HEIGHT);
        {
            let (y, u, v) = picture.data_mut();
            y.fill(126);
            u.fill(128);
            v.fill(128);
        }
        sink.frame(&picture).unwrap();
        let first = frames.take(|_, _, pixels| pixels[..4].to_vec()).unwrap();
        picture.data_mut().0.fill(200);
        sink.frame(&picture).unwrap();
        let second = frames.take(|_, _, pixels| pixels[..4].to_vec()).unwrap();
        assert!(
            first[..3].iter().all(|&level| level.abs_diff(128) < 4),
            "mid-grey: {first:?}"
        );
        assert!(
            second[..3].iter().all(|&level| level > 180),
            "the frame after it: {second:?}"
        );
    }

    /// A frame reaches the sink while the capture's status is held elsewhere:
    /// that status isn't what a frame waits for.
    #[test]
    fn a_frame_reaches_the_sink_while_its_status_is_held() {
        let frames = Frames::create(4, 2).unwrap();
        let (_controls, received) = channel::channel();
        let sink = Arc::new(Mutex::new(Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_shown: true,
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
            footage: Footage::InMemory,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
        }));
        let captured = Arc::new(Mutex::new(Captured::default()));
        // Held as the loop holds it while it reads the capture's status.
        let held = captured.lock().unwrap();
        let (into, seen) = (sink.clone(), captured.clone());
        // Four pixels by two, a row every sixteen bytes, mid-grey.
        let argb = [128u8; 4 * 2 * 4];
        let taking = std::thread::spawn(move || {
            let mut scratch = Scratch::default();
            to_sink(
                &into,
                &seen,
                &mut scratch,
                &argb,
                16,
                ((4, 2), (4, 2)),
                &Tone::SDR,
            );
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while frames.take(|_, _, _| ()).is_none() {
            assert!(
                Instant::now() < deadline,
                "a frame waited on the capture's status"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(held);
        taking.join().unwrap();
        let captured = captured.lock().unwrap();
        assert_eq!(captured.size, Some((4, 2)));
    }

    #[test]
    fn serves_a_test_pattern_until_stopped() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let job = Job::Share {
            source: Source::TestPattern,
            frames: frames.path().unwrap(),
            width: PREVIEW_WIDTH,
            height: PREVIEW_HEIGHT,
            fps: 60,
            effort: Effort::Light,
            hardware: false,
        };
        let (input, mut feed) = std::io::pipe().unwrap();
        writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
        // The UI shows the preview, as the app tells the worker it does.
        writeln!(
            feed,
            "{}",
            serde_json::to_string(&Control::Preview(true)).unwrap()
        )
        .unwrap();
        let (sender, reports) = std::sync::mpsc::channel();
        let serving = std::thread::spawn(move || {
            serve(Box::new(BufReader::new(input)), &mut |report| {
                sender.send(report).unwrap();
            });
        });
        assert_eq!(reports.recv().unwrap(), Report::Ready);
        assert_eq!(
            reports.recv().unwrap(),
            Report::Sharing {
                width: 640,
                height: 360
            }
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let size = loop {
            if let Some(size) = frames.take(|width, height, pixels| (width, height, pixels.len())) {
                break size;
            }
            assert!(Instant::now() < deadline, "no frame arrived");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(size, (640, 360, 640 * 360 * 4));
        writeln!(feed, "stop").unwrap();
        serving.join().unwrap();
        assert_eq!(reports.recv().unwrap(), Report::Ended { error: None });
    }

    #[test]
    fn shares_a_camera_until_stopped() {
        // SAFETY: tests that read this variable all set it the same way.
        unsafe { std::env::set_var("YAPFAST_CAMERA_TEST_PATTERN", "1") };
        let listed = shareable();
        let camera = listed
            .iter()
            .find(|listed| listed.title == "Test pattern")
            .expect("the test pattern camera is listed");
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let job = Job::Share {
            source: camera.source.clone(),
            frames: frames.path().unwrap(),
            width: PREVIEW_WIDTH,
            height: PREVIEW_HEIGHT,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
        };
        let (input, mut feed) = std::io::pipe().unwrap();
        writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
        // The UI shows the preview, as the app tells the worker it does.
        writeln!(
            feed,
            "{}",
            serde_json::to_string(&Control::Preview(true)).unwrap()
        )
        .unwrap();
        let (sender, reports) = std::sync::mpsc::channel();
        let serving = std::thread::spawn(move || {
            serve(Box::new(BufReader::new(input)), &mut |report| {
                sender.send(report).unwrap();
            });
        });
        assert_eq!(reports.recv().unwrap(), Report::Ready);
        assert_eq!(
            reports.recv_timeout(Duration::from_secs(5)).unwrap(),
            Report::Sharing {
                width: 640,
                height: 360
            }
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let size = loop {
            if let Some(size) = frames.take(|width, height, pixels| (width, height, pixels.len())) {
                break size;
            }
            assert!(Instant::now() < deadline, "no frame arrived");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(size, (640, 360, 640 * 360 * 4));
        writeln!(feed, "stop").unwrap();
        serving.join().unwrap();
        assert_eq!(reports.recv().unwrap(), Report::Ended { error: None });
    }

    #[test]
    fn a_screen_picture_survives_its_report_line() {
        let report = Report::Screen {
            id: 7,
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
        };
        let line = serde_json::to_string(&report).unwrap();
        assert!(line.contains(r#""rgba":"/wAA/wAA//8=""#), "{line}");
        assert_eq!(serde_json::from_str::<Report>(&line).unwrap(), report);
    }

    /// Takes a picture of each listed screen, as the share menu shows them.
    #[test]
    #[ignore = "needs a desktop where Yapfast lists what to share"]
    fn glances_at_the_listed_screens() {
        let screens: Vec<u64> = shareable()
            .into_iter()
            .filter_map(|listed| match listed.source {
                Source::Display(id) => Some(id),
                _ => None,
            })
            .collect();
        assert!(!screens.is_empty(), "no screen");
        let job = Job::Glance {
            screens: screens.clone(),
            width: 320,
            height: 180,
        };
        let (input, mut feed) = std::io::pipe().unwrap();
        writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
        let mut reports = vec![];
        serve(Box::new(BufReader::new(input)), &mut |report| {
            reports.push(report)
        });
        let pictured: Vec<u64> = reports
            .iter()
            .filter_map(|report| match report {
                Report::Screen {
                    id,
                    width,
                    height,
                    rgba,
                } => {
                    eprintln!("screen {id}: {width}x{height}");
                    assert!(*width <= 320 && *height <= 180);
                    assert_eq!(rgba.len(), (width * height * 4) as usize);
                    Some(*id)
                }
                _ => None,
            })
            .collect();
        assert_eq!(pictured, screens);
        assert_eq!(reports.last(), Some(&Report::Ended { error: None }));
    }

    /// Lists what there is to share, and shares the first screen and the
    /// first window for a moment each.
    #[test]
    #[ignore = "needs a desktop where Yapfast lists what to share"]
    fn shares_a_listed_screen_and_window() {
        let listed = shareable();
        eprintln!("{listed:#?}");
        let first = |window: bool| {
            listed
                .iter()
                .find(|listed| matches!(listed.source, Source::Window(_)) == window)
                .map(|listed| listed.source.clone())
        };
        let sources = [
            first(false).expect("no screen"),
            first(true).expect("no window"),
        ];
        for source in sources {
            let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
            let job = Job::Share {
                source: source.clone(),
                frames: frames.path().unwrap(),
                width: PREVIEW_WIDTH,
                height: PREVIEW_HEIGHT,
                fps: 30,
                effort: Effort::Light,
                hardware: false,
            };
            let (input, mut feed) = std::io::pipe().unwrap();
            writeln!(feed, "{}", serde_json::to_string(&job).unwrap()).unwrap();
            let (sender, reports) = std::sync::mpsc::channel();
            let serving = std::thread::spawn(move || {
                serve(Box::new(BufReader::new(input)), &mut |report| {
                    sender.send(report).unwrap();
                });
            });
            assert_eq!(reports.recv().unwrap(), Report::Ready);
            let sharing = reports.recv_timeout(Duration::from_secs(10)).unwrap();
            eprintln!("{source:?}: {sharing:?}");
            assert!(matches!(sharing, Report::Sharing { .. }), "{sharing:?}");
            // How much of the last picture isn't black, a second and a half
            // in: what's shared may still be drawing at first.
            let deadline = Instant::now() + Duration::from_millis(1500);
            let mut last = None;
            while Instant::now() < deadline {
                let taken = frames.take(|width, height, pixels| {
                    let lit = pixels.chunks_exact(4).filter(|pixel| pixel[..3] != [0; 3]);
                    (
                        width,
                        height,
                        lit.count() as f32 / (pixels.len() / 4).max(1) as f32,
                    )
                });
                last = taken.or(last);
                std::thread::sleep(Duration::from_millis(5));
            }
            let (width, height, lit) = last.expect("no frame arrived");
            eprintln!(
                "{source:?}: {width}x{height}, {:.0}% of it isn't black",
                lit * 100.0
            );
            writeln!(feed, "stop").unwrap();
            serving.join().unwrap();
            // The size may change as capture settles.
            let ended = reports
                .iter()
                .find(|report| !matches!(report, Report::Sharing { .. }));
            assert_eq!(ended, Some(Report::Ended { error: None }));
        }
        // A window closed since it was listed.
        let frames = Frames::create(16, 16).unwrap();
        let gone = Job::Share {
            source: Source::Window(u64::MAX),
            frames: frames.path().unwrap(),
            width: 16,
            height: 16,
            fps: 1,
            effort: Effort::Light,
            hardware: false,
        };
        let mut reports = Vec::new();
        let input = format!("{}\n", serde_json::to_string(&gone).unwrap());
        serve(Box::new(std::io::Cursor::new(input)), &mut |report| {
            reports.push(report);
        });
        assert_eq!(
            reports,
            [
                Report::Ready,
                Report::Ended {
                    error: Some("That screen or window is gone.".into())
                }
            ]
        );
    }

    /// A frame that didn't decode leaves every delta after it describing a
    /// picture the decoder never had, so the watch waits for a keyframe
    /// instead of handing them over.
    #[test]
    fn a_frame_that_didnt_decode_waits_for_a_keyframe() {
        let junk = |keyframe| ReceivedFrame {
            codec: Codec::Vp8,
            data: b"not video".to_vec(),
            keyframe,
        };
        let mut decoding = Decoding::new();
        // libvpx says so, and the watch is left waiting.
        assert!(matches!(decoding.decode(&junk(false)), Decoded::Failed(_)));
        // The deltas after it reference the frame that didn't decode.
        assert!(matches!(decoding.decode(&junk(false)), Decoded::Waiting));
        // A keyframe is what the decoder can start again from, whatever it
        // holds, and the deltas after it are its own.
        assert!(matches!(decoding.decode(&junk(true)), Decoded::Failed(_)));
        assert!(matches!(decoding.decode(&junk(false)), Decoded::Waiting));
    }

    /// A stream that turns to another codec starts a decoder afresh, which
    /// knows nothing of the pictures the one before it decoded.
    #[test]
    fn a_codec_change_waits_for_a_keyframe() {
        let frame = |codec, keyframe| ReceivedFrame {
            codec,
            data: b"not video".to_vec(),
            keyframe,
        };
        let mut decoding = Decoding::new();
        assert!(matches!(
            decoding.decode(&frame(Codec::Vp8, false)),
            Decoded::Failed(_)
        ));
        assert!(matches!(
            decoding.decode(&frame(Codec::H264, false)),
            Decoded::Waiting
        ));
        assert!(
            !matches!(decoding.decode(&frame(Codec::H264, true)), Decoded::Waiting),
            "a keyframe reaches the new decoder"
        );
    }

    /// A codec this build has no decoder for doesn't end the watch: the
    /// stream's sound plays on, no frame of that codec costs another attempt,
    /// and a codec this build decodes gets pictures back.
    #[test]
    fn a_codec_the_build_cant_decode_waits_for_one_it_can() {
        use std::sync::atomic::AtomicUsize;
        static MADE: AtomicUsize = AtomicUsize::new(0);
        fn no_h265(codec: Codec) -> Result<Live> {
            MADE.fetch_add(1, Ordering::Relaxed);
            match codec {
                Codec::H265 => bail!("this build of Yapfast can't decode H265"),
                codec => Live::new(codec),
            }
        }
        let frame = |codec, keyframe| ReceivedFrame {
            codec,
            data: b"not video".to_vec(),
            keyframe,
        };
        let mut decoding = Decoding {
            make: no_h265,
            ..Decoding::new()
        };
        // The watch waits through the codec it can't decode, however many
        // frames of it come, and does nothing with them.
        assert!(matches!(
            decoding.decode(&frame(Codec::H265, true)),
            Decoded::Waiting
        ));
        assert!(matches!(
            decoding.decode(&frame(Codec::H265, false)),
            Decoded::Waiting
        ));
        assert_eq!(MADE.load(Ordering::Relaxed), 1, "no attempt a frame");
        // A codec it does decode reaches a decoder of its own.
        assert!(matches!(
            decoding.decode(&frame(Codec::Vp8, false)),
            Decoded::Failed(_)
        ));
        assert_eq!(MADE.load(Ordering::Relaxed), 2);
    }
}
