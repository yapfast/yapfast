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

#[cfg(target_os = "linux")]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::{
    ffi::c_void,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc as channel,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use yapfast_discord::{
    congestion,
    rtp::Codec,
    stream::StreamServer,
    stream_connection::{self, Link, ReceivedFrame, StreamReceiver, StreamSender},
};

use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoBuffer},
};

use crate::capture;
#[cfg(target_os = "linux")]
use crate::capture::linear::{self, Order};
#[cfg(nvcuda)]
use crate::cuda;
#[cfg(any(wgc, dup))]
use crate::d3d11;
#[cfg(target_os = "linux")]
use crate::dmabuf;
#[cfg(dup)]
use crate::dup;
#[cfg(any(wgc, dup))]
use crate::frames::Drawn;
#[cfg(wgc)]
use crate::wgc;
use crate::{
    av_timing,
    decode::{Frame, Live},
    encode::{self, Effort, Encoder},
    frames::{Frames, Writer},
    picture::{self, I420, Tone},
    pressure, stream_audio,
    voice::{Presentation, Resolution, StreamQuality, StreamStats},
};

/// The argument that starts Yapfast as a stream worker.
pub const ARGUMENT: &str = "--stream-worker";

/// The environment variable that tells a stream worker how many threads its
/// decoder may use: the share of the machine the Yapfast that starts it works
/// out from the machine's cores and how many streams it's decoding at once.
const THREADS: &str = "YAPFAST_STREAM_THREADS";

/// The largest frame shared at 1080p: the box a stream's picture is scaled down
/// to fit, which turns with a screen on its side — see [`fit`]. Discord's own
/// client trials 1080p at 30 fps for accounts without Nitro, up to 5 Mbit/s (its
/// "frontier tuning" experiment); larger screens are scaled down to it.
pub const SHARE_WIDTH: u32 = 1920;
pub const SHARE_HEIGHT: u32 = 1080;
/// The largest preview of the shared screen the UI shows.
pub const PREVIEW_WIDTH: u32 = 1280;
pub const PREVIEW_HEIGHT: u32 = 720;
/// The longest edge a preview drawn on the card is scaled to. It is the
/// stream's own picture, shown where the card drew it — the only copy that
/// costs nothing a frame — so it is worth the stream's pixels: 1920 is the
/// largest a 1080p share sends, and a portrait screen's 1080x1920 fits the
/// same box stood up. A share of a larger screen sends its picture at this
/// edge too, so the preview and the stream agree.
#[cfg(any(wgc, dup))]
const PREVIEW_LONG_EDGE: u32 = 1920;
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
/// How long a still that couldn't be read back waits before it is asked for
/// again.
#[cfg(target_os = "linux")]
const STILL_RETRY: Duration = Duration::from_secs(5);

/// How often a share that has produced no picture says what its capture is
/// waiting for: a stream nothing negotiates with is otherwise a long silence.
#[cfg(target_os = "linux")]
const WAITING_EVERY: Duration = Duration::from_secs(5);
/// How many pictures in a row the card's own path may refuse before the share
/// gives it up for the processor's capturer: half a second of a 60 fps screen,
/// which rides out the one buffer a compositor draws in another layout while
/// catching a layout that has changed for good.
#[cfg(nvcuda)]
const REFUSED_PICTURES: u32 = 30;
/// How long a desktop that stopped drawing goes unsaid before the share says so:
/// a compositor sends a picture when it has one, so a share of a screen nobody is
/// drawing to sends nothing, which is worth a line rather than being taken for a
/// stream that stalled.
#[cfg(target_os = "linux")]
const IDLE_EVERY: Duration = Duration::from_secs(5);
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
#[cfg(any(wgc, dup))]
const PREVIEW_EVERY: Duration = Duration::from_millis(100);
/// How many times a share puts the card's session and its capture back
/// together before it stops trying. A driver reset is worth rebuilding both
/// for; a card that answers every frame with the same failure is one to stop
/// sharing on, not to keep asking.
#[cfg(any(wgc, dup))]
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
        /// Whether the stream may declare more than 9 Mbit/s for its picture,
        /// Settings' experimental switch. It travels with the job for the same
        /// reason the capture's preference does: the worker is a process of its
        /// own, and it is where the stream is declared.
        #[serde(default)]
        high_bitrate: bool,
        /// The codec Settings' experimental choice asks Discord for first; see
        /// [`StreamCodec`]. It travels with the job for the same reason again:
        /// the worker is a process of its own, and it is where the codecs are
        /// offered.
        #[serde(default)]
        codec: StreamCodec,
        /// How a screen is captured — the setting's choice, or Auto; see
        /// [`crate::capture`]. It travels with the job because the worker is a
        /// process of its own: a choice made in Yapfast's process means nothing
        /// in the one that captures.
        #[serde(default)]
        capture: capture::Preference,
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

    /// What the job is, for the log.
    fn kind(&self) -> &'static str {
        match self {
            Self::Share { .. } => "share",
            Self::Watch { .. } => "watch",
            Self::Play { .. } => "play",
            Self::Glance { .. } => "glance",
        }
    }

    /// The size its pictures fit in.
    fn bounds(&self) -> (u32, u32) {
        match *self {
            Self::Share { width, height, .. }
            | Self::Watch { width, height, .. }
            | Self::Play { width, height, .. }
            | Self::Glance { width, height, .. } => (width, height),
        }
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

/// Whether the system's dialog, the screen sharing portal, picks the screen or
/// window to share, rather than Yapfast listing them: in a Wayland session.
/// libwebrtc captures through the portal only there, and through X11 in any
/// other Linux session, as it decides by the same variables; its X11
/// capturers list what there is, as Windows' do. XFCE, for one, has no
/// screen sharing portal on X11.
pub fn portal_picks() -> bool {
    cfg!(target_os = "linux")
        && wayland_session(
            std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
            std::env::var_os("WAYLAND_DISPLAY").is_some(),
        )
}

/// Whether a session of this `kind`, in `XDG_SESSION_TYPE`, with a Wayland
/// display named or not, is one libwebrtc captures through the portal in.
fn wayland_session(kind: Option<&str>, wayland_display: bool) -> bool {
    kind.is_some_and(|kind| kind.starts_with("wayland")) && wayland_display
}

/// A screen, window or camera [`shareable`] lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shareable {
    pub source: Source,
    pub title: String,
}

/// What Yapfast can share: the screens, then the windows, unless
/// [`portal_picks`], then the cameras. On Windows, Yapfast's own windows
/// aren't among them, nor its console's.
pub fn shareable() -> Vec<Shareable> {
    let mut listed = screens_and_windows();
    listed.extend(crate::camera::list().into_iter().map(|device| Shareable {
        source: Source::Camera(device.id),
        title: device.name,
    }));
    listed
}

fn screens_and_windows() -> Vec<Shareable> {
    if portal_picks() {
        return Vec::new();
    }
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
/// menu, until another. Where [`portal_picks`], the pick may be
/// [`Source::Screen`] itself, which the system's dialog picks.
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
    /// a picture; where [`portal_picks`], what the system's dialog picks. The
    /// stream goes on with the one shared until then, and when the dialog is
    /// cancelled.
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
    // A stalled capture must not accumulate an unbounded backlog of resize,
    // preview and source-switch controls. Backpressure preserves their order.
    let (controls, received) = channel::sync_channel(64);
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
            high_bitrate,
            codec,
            capture: preference,
        } => {
            crate::capture::set(preference);
            // The stream is declared from here, so the ceiling comes with the
            // job: op 12 goes out once, with the rate this process was told to
            // allow, and the controller is seeded from it.
            stream_connection::set_high_bitrate(high_bitrate);
            // The codecs are offered from here too, so the setting's choice
            // comes with the job as well: it is this process that offers them.
            set_stream_codec(codec);
            // The interface offers 15, 30 and 60; a measurement may ask for
            // more, and a stream of its own is the only thing that suffers.
            let fps = fps.clamp(1, 240);
            let sink = Sink {
                writer: Writer::open(&frames)?,
                preview: vec![],
                preview_tile: None,
                capture: String::new(),
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
                diagnosis: Diagnosis::default(),
                #[cfg(nvcuda)]
                refused_pictures: 0,
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
    /// What is captured, in the words the call's Stream tab shows: which of
    /// the ways of capturing ran and the size of what it captures. Set once,
    /// by the capture before it starts.
    capture: String,
    /// Whether the UI shows the preview, which it says as that changes: one
    /// isn't made while nothing displays it.
    preview_shown: bool,
    /// The size in pixels of the tile showing the preview, once the UI has
    /// said it: what the preview is written at, rather than a fixed box.
    preview_tile: Option<(u32, u32)>,
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
    /// Where the pictures come from and how they reach the window, which is
    /// the half of a share no packet shows.
    diagnosis: Diagnosis,
    /// How many pictures in a row the card's own path refused, which is how a
    /// share tells a compositor handing one buffer of another layout from a
    /// picture path that has gone: see [`Sink::dmabuf`].
    #[cfg(nvcuda)]
    refused_pictures: u32,
}

/// What a share can say about itself that the picture can't: how its frames
/// reach the window, on which adapter each step ran, why it isn't on the path
/// it asked for, how long a frame waited, and what the machine's memory has it
/// give back.
#[derive(Default)]
struct Diagnosis {
    /// How the pictures reach the window, once one has: the worker's own
    /// texture when the window took one, a copy of the pixels otherwise.
    /// Unknown until a picture has gone one way or the other.
    presentation: Option<Presentation>,
    /// The adapter the card drew this share on, when it drew one: the capture
    /// and the encoder are the same device there, which is what makes a frame
    /// cost no copy.
    gpu_luid: Option<u64>,
    /// Why the share isn't on the path it asked for, as the code that fell
    /// back said it. One line, kept rather than grown.
    fallback: Option<String>,
    /// How long the last picture waited between the capture having it and the
    /// encoder being given it, in milliseconds.
    queue_delay_ms: Option<u32>,
    /// When the capture handed over the picture the encoder is about to read.
    captured_at: Option<Instant>,
    /// What a machine short of memory has the share give back.
    memory: crate::pressure::Policy,
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
    /// A screen Windows Graphics Capture or DXGI's Desktop Duplication filled
    /// a texture with, which NVENC reads on the card that shows it. Nothing of
    /// a frame crosses the processor on its way to the encoder.
    #[cfg(any(wgc, dup))]
    OnTheCard,
    /// A screen or window the desktop's portal or the X server handed over as
    /// the DMA-BUFs its compositor drew. The window imports them and draws them
    /// where they lie; the encoder reads the descriptor itself where it can, and
    /// the same picture converted on the processor where it can't — which is
    /// the one copy a frame costs here, and [`Sink::dmabuf`] says when it takes
    /// it.
    #[cfg(target_os = "linux")]
    Dmabuf,
}

/// What turns a frame into a packet.
///
/// All three are the same stream from the packetizer's side: the bytes, their
/// time on the stream clock and whether the frame is whole all come out of
/// [`Encoding::encode_i420`], [`Encoding::encode_dmabuf`] or
/// [`d3d11::D3d11Encoder::encode_texture`] in [`encode::Frame`], and go on to
/// the sender as one shape.
// One per stream, held for its length: its size is no cost, and boxing the
// encoder would add a pointer to every frame's call instead.
#[allow(clippy::large_enum_variant)]
enum Encoding {
    /// FFmpeg's encoder on the processor — x264, libvpx, or one of FFmpeg's
    /// hardware encoders — reading an I420 picture the capture converted.
    Picture(Encoder),
    /// NVENC on the card, reading the capture's texture where the frame
    /// already is. The one copy a frame still costs is its bitstream out of
    /// the driver, which [`d3d11::D3d11Encoder::copied`] says is the only one.
    #[cfg(any(wgc, dup))]
    Texture(d3d11::D3d11Encoder),
    /// NVENC on the card, reading the compositor's picture after the card has
    /// imported and converted it in its own memory: the Linux counterpart of
    /// [`Self::Texture`], for a capture whose frames are DMA-BUFs rather than
    /// textures. No pixel of a frame crosses to the processor here either.
    #[cfg(nvcuda)]
    Cuda(Cuda),
    /// A capture's own texture, encoded where it lies by an FFmpeg session that
    /// reads surfaces — Quick Sync and AMF on Windows. The session was opened on
    /// the capture's device and is handed the same texture the window draws, so
    /// no frame is converted or uploaded first; [`Self::Texture`] is NVENC's own
    /// session, which FFmpeg has no path for.
    #[cfg(all(target_os = "windows", hwenc))]
    QsvTextures(encode::Encoder),
}

/// NVIDIA's card as the way a captured picture reaches its encoder: the
/// compositor's DMA-BUF is imported through EGL, scaled and converted to NV12
/// by [`cuda::Convert`]'s kernels on the card, and read there by
/// [`cuda::Encoder`]. What crosses the bus is the access unit the session
/// answers with, and nothing else.
#[cfg(nvcuda)]
struct Cuda {
    /// The import, the kernels and the one buffer they write frame after frame.
    convert: cuda::Convert,
    /// The session opened on that buffer, which reads it where it lies.
    encoder: cuda::Encoder,
    codec: Codec,
    size: (u32, u32),
    /// The kbit/s the session aims for now, which follows the sender's target as
    /// the session fed from memory does.
    kbps: u32,
    /// The line the log and the Stream tab show: which backend runs, the card it
    /// runs on, and that its pictures never come back to the processor.
    engine: String,
}

#[cfg(nvcuda)]
impl Cuda {
    /// The card's own path for a stream of this size in `codec`, or `None` with
    /// the reason in the log: the share then encodes through the encoder FFmpeg
    /// opens. Said every time it is asked for, since a share asks again when its
    /// codec or its size changes, and a share that dropped to an encoder that
    /// uploads each frame has to say why it did.
    fn open(codec: Codec, width: u32, height: u32, fps: u32, kbps: u32) -> Option<Self> {
        let opened = cuda::Convert::open(width, height).and_then(|convert| {
            let encoder = cuda::Encoder::open(codec, width, height, fps, kbps, &convert)?;
            Ok((convert, encoder))
        });
        let (convert, encoder) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                tracing::info!(
                    why = format!("{error:#}").as_str(),
                    "converting every picture: the card can't take them where they lie"
                );
                return None;
            }
        };
        // The line the report carries from here on: the session, by FFmpeg's
        // name for it, then the card and its driver, and where its pictures
        // come from. The name comes first because the Stream tab reads where
        // the encoding runs from it, as it does for the encoders FFmpeg opens.
        // Said once, at the open, with what the session actually took, as the
        // VA-API path says its own.
        let name = encode::nvenc_name(codec).expect("NVENC opened a session for this codec");
        let engine = format!(
            "{name} ({}), encoding DMA-BUFs where they lie",
            convert.describe()
        );
        let (config, tuning) = (encoder.config(), encoder.tuning());
        tracing::info!(
            engine = engine.as_str(),
            config = config.as_str(),
            tuning = tuning.as_str(),
            "encoding the card's own pictures: the compositor's DMA-BUF is imported through EGL \
             and scaled and converted to NV12 on the card, and the session reads it there"
        );
        Some(Self {
            convert,
            encoder,
            codec,
            size: (width, height),
            kbps,
            engine,
        })
    }

    /// The captured picture, imported and converted on the card and then
    /// submitted to the session that reads it there, at `pts` on the stream
    /// clock. `keyframe` asks for an IDR, and the session makes one.
    ///
    /// `Err` is a picture the card can't take — a tiled buffer its driver only
    /// gives back as an array, a picture on a device that isn't the one the
    /// import opened — which is the path's reason to be given up rather than
    /// the frame's failure. An encode that fails after the convert is the
    /// session's own failure, and is the frame's.
    fn encode_dmabuf(
        &mut self,
        shot: &Shot,
        pts: u64,
        keyframe: bool,
    ) -> Result<std::result::Result<Option<encode::Frame>, String>> {
        if let Err(error) = self.convert.convert(&shot.descriptor()) {
            return Ok(Err(format!("{error:#}")));
        }
        let packet = self.encoder.encode(pts, keyframe)?;
        // The picture was encoded where it lies, so a frame the session says it
        // copied would be one the processor paid for, which is the thing this
        // path is here to keep out of the stream.
        debug_assert_eq!(
            self.encoder.copied(),
            0,
            "the card copied a picture's pixels"
        );
        Ok(Ok(packet.map(|packet| encode::Frame {
            // The unit is the pool's buffer the sender takes, not a copy of it.
            data: packet.data,
            keyframe: packet.keyframe,
            // The one picture this submission made, at the time it was made: as
            // the encoder fed from memory stamps each of its own.
            timestamp: pts as u32,
        })))
    }
}

impl Encoding {
    fn codec(&self) -> Codec {
        match self {
            Self::Picture(encoder) => encoder.codec(),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => encoder.codec(),
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => cuda.codec,
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.codec(),
        }
    }

    fn size(&self) -> (u32, u32) {
        match self {
            Self::Picture(encoder) => encoder.size(),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => encoder.size(),
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => cuda.size,
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.size(),
        }
    }

    fn bitrate(&self) -> u32 {
        match self {
            Self::Picture(encoder) => encoder.bitrate(),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => encoder.bitrate(),
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => cuda.kbps,
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.bitrate(),
        }
    }

    fn set_bitrate(&mut self, kbps: u32) -> Result<()> {
        match self {
            Self::Picture(encoder) => encoder.set_bitrate(kbps),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => encoder.set_bitrate(kbps),
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => {
                cuda.encoder.set_bitrate(kbps)?;
                cuda.kbps = kbps;
                Ok(())
            }
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.set_bitrate(kbps),
        }
    }

    /// Gives back the output buffers lying idle, for a machine short of
    /// memory: a buffer being encoded into or waiting to be sent is untouched.
    fn trim_pool(&mut self) {
        match self {
            Self::Picture(encoder) => encoder.trim_pool(),
            // The card's session keeps its surfaces with the capture that
            // made them, and gives them up with it rather than here.
            #[cfg(any(wgc, dup))]
            Self::Texture(_) => {}
            // The convert's buffer and the session's output buffer are the
            // card's and the convert's own, and go with the share rather than
            // back to anything here.
            #[cfg(nvcuda)]
            Self::Cuda(_) => {}
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.trim_pool(),
        }
    }

    /// Which encoder runs, such as "x264" or "h264_nvenc/d3d11".
    fn engine(&self) -> &str {
        match self {
            Self::Picture(encoder) => encoder.engine(),
            #[cfg(any(wgc, dup))]
            Self::Texture(encoder) => encoder.engine(),
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => cuda.engine.as_str(),
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder.engine(),
        }
    }

    /// The encoder's side of a stream's report, which says what made the
    /// picture rather than what was meant to.
    fn report(&self) -> StreamStats {
        match self {
            Self::Picture(encoder) => encoder_report(encoder),
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(encoder) => encoder_report(encoder),
            #[cfg(any(wgc, dup))]
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
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => StreamStats {
                engine: cuda.engine.clone(),
                codec: cuda.codec.name().to_owned(),
                width: cuda.size.0,
                height: cuda.size.1,
                // As above: what this session asks for it gets, or it doesn't
                // open at all.
                settings_dropped: String::new(),
                ..StreamStats::default()
            },
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
            #[cfg(any(wgc, dup))]
            Self::Texture(_) => None,
            // This stream's frames are the card's, made from a descriptor; a
            // picture in memory is the UI's preview and nothing this sends.
            #[cfg(nvcuda)]
            Self::Cuda(_) => None,
            // Read where the capture drew it: a picture in memory is the UI's
            // preview, not something this stream sends.
            #[cfg(all(target_os = "windows", hwenc))]
            Self::QsvTextures(_) => None,
        }
    }

    /// Encodes the picture the desktop captured as `shot`, at `sent` — the size
    /// the stream sends — by handing the descriptor itself to the encoder where
    /// it reads one, so the card imports the picture and no pixel of it comes
    /// back to the processor.
    ///
    /// An encoder that reads memory answers with the reason it won't take a
    /// descriptor ([`encode::Refused`]) — x264 does, and so does a VA-API session
    /// handed a screen, which is BGRA where a VA-API surface is made of NV12 —
    /// and is then given the same picture converted into `scratch`. That copy is
    /// this path's one cost of a frame, it is the reason the share reports, and
    /// it lasts until an encoder here reads a compositor's picture where it lies.
    ///
    /// Where [`Self::Cuda`] carries the stream, the card has already imported and
    /// converted the picture in its own memory, and no pixel of it comes back to
    /// the processor: the conversion on the processor is what a picture that path
    /// can't take gets, from the frame it can't on.
    #[cfg(target_os = "linux")]
    fn encode_dmabuf(
        &mut self,
        shot: &Shot,
        scratch: &mut Scratch,
        sent: (u32, u32),
        pts: u64,
        keyframe: bool,
        time_ms: u64,
    ) -> Result<Sent> {
        match self {
            Self::Picture(encoder) => {
                pictured(encoder, shot, scratch, sent, pts, keyframe, time_ms)
            }
            #[cfg(any(wgc, dup))]
            Self::Texture(_) => bail!("the card's encoder reads textures, not a desktop's DMA-BUF"),
            // The picture is imported and converted on the card, and the session
            // reads it there: what a frame costs is its bitstream. A picture this
            // path can't take gives the path up rather than the frame, and the
            // caller hands the same picture to the processor's copy.
            #[cfg(nvcuda)]
            Self::Cuda(cuda) => match cuda.encode_dmabuf(shot, pts, keyframe)? {
                Ok(frame) => Ok(Sent::Frame {
                    frame,
                    converted: None,
                }),
                Err(why) => Ok(Sent::Card(why)),
            },
        }
    }
}

/// A picture handed to the encoder FFmpeg opened, at `sent`: read where it lies
/// where that encoder takes a descriptor, and converted into `scratch` where it
/// doesn't — which is the one copy a picture of that path costs, and
/// [`Sent::Frame::converted`] says why it was made.
#[cfg(target_os = "linux")]
fn pictured(
    encoder: &mut Encoder,
    shot: &Shot,
    scratch: &mut Scratch,
    sent: (u32, u32),
    pts: u64,
    keyframe: bool,
    time_ms: u64,
) -> Result<Sent> {
    match encoder.encode_dmabuf(&shot.descriptor(), keyframe, time_ms) {
        Ok(frame) => Ok(Sent::Frame {
            frame: frame?,
            converted: None,
        }),
        Err(refused) => match shot.converted(scratch, sent) {
            Ok(picture) => Ok(Sent::Frame {
                frame: encoder.encode_i420(picture, pts, keyframe)?,
                converted: Some(refused),
            }),
            Err(why) => Ok(Sent::Unreadable(format!("{why:#}"))),
        },
    }
}

/// What became of a picture the desktop captured, at the encoder.
#[cfg(target_os = "linux")]
enum Sent {
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
    /// The rate the encoder refused, until the sender's target moves: asked
    /// again with every frame, a refusal would be a warning a frame.
    refused: Option<u32>,
    /// The card's video processor between a capture's BGRA picture at its own
    /// size and the NV12 surface at the stream's that a session reading surfaces
    /// takes: made on the first frame, and again when a size changes.
    #[cfg(all(target_os = "windows", hwenc, d3d11conv))]
    converter: Option<d3d11::Converter>,
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
impl Video {
    /// Gives back what the encoder can spare, for a machine short of memory.
    fn trim_pool(&mut self) {
        if let Some(encoder) = self.encoder.as_mut() {
            encoder.trim_pool();
        }
    }

    /// How the stream goes out, with the kbit/s sent since it last said.
    fn sending(&mut self, fps: u32) -> StreamStats {
        let sent = self.sender.sent();
        let (since, before) = self.measured;
        let bits = sent.video_bytes.saturating_sub(before) * 8;
        let milliseconds = since.elapsed().as_millis().max(1) as u64;
        self.measured = (Instant::now(), sent.video_bytes);
        // The sound shared with the screen, while there is one: what waits to
        // be mixed is delay a viewer hears before the picture it came with. A
        // share with no sound says unknown rather than nothing waited.
        let sound = self.sound.as_ref().map(stream_audio::Capture::stats);
        StreamStats {
            fps,
            target_kbps: (self.sender.target() / 1000) as u32,
            sent_kbps: (bits / milliseconds) as u32,
            keyframe_requests: sent.keyframe_requests,
            resent: sent.resent,
            skipped: self.skipped,
            sound_held_ms: sound.map(|sound| sound.held_ms),
            sound_dropped_ms: sound.map(|sound| sound.dropped_ms),
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
    const fn wanted(self) -> Option<Codec> {
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
fn preferred(available: Vec<Codec>, choice: StreamCodec) -> Vec<Codec> {
    let Some(wanted) = choice.wanted().filter(|wanted| available.contains(wanted)) else {
        return available;
    };
    let mut offers = available;
    offers.retain(|codec| *codec != wanted);
    offers.insert(0, wanted);
    offers
}

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

/// The kbit/s a stream of `width` by `height` at `fps` declares it is worth,
/// which is what an encoder is opened at before the stream has a target of its
/// own: a card's encoder opened to capture with, and the preview's, run for
/// nothing but a picture and a thumbnail. See [`stream_connection::max_bitrate`].
/// Whether a share may hand its session the capture's own surface, through the
/// card's video processor: the capture's BGRA picture at its own size becomes the
/// NV12 surface at the stream's, which is what Quick Sync and AMF read. On by
/// default, since it is the card doing that work either way; the variable turns
/// it off for a comparison against the upload path.
#[cfg(all(target_os = "windows", hwenc))]
fn surfaces_offered() -> bool {
    std::env::var_os("YAPFAST_STREAM_SURFACES").is_none_or(|asked| asked != "0")
}

fn kbps(width: u32, height: u32, fps: u32) -> u32 {
    stream_connection::max_bitrate(width, height, fps) / 1000
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
        self.delivered(codec, made, keyframe)
    }

    /// Takes a picture the desktop captured as the DMA-BUF the compositor drew
    /// it in, `shot`, at the size the stream sends, `sent`.
    ///
    /// On NVIDIA's card the picture goes to the encoder where it lies: the card
    /// imports it, scales and converts it there, and the session reads it in its
    /// own memory, so no pixel of it comes back to the processor. A picture that
    /// path can't take gives the path up and is converted on the processor, as
    /// every picture of the share is from then on.
    ///
    /// The encoder is otherwise handed the descriptor itself where it reads one: a
    /// VA-API session imports the picture on the card and no pixel of it comes
    /// back to the processor. Every other encoder reads memory — and so does a
    /// VA-API session handed a screen, which is BGRA where a VA-API surface is
    /// made of NV12 — and for those the picture is read back into the buffer kept
    /// for it and converted, once, with the reason in the share's report. That
    /// copy is the one this path makes of a frame, and [`Taken::Sent`] says which
    /// of the ways sent it.
    #[cfg(target_os = "linux")]
    fn dmabuf(&mut self, shot: &Shot, sent: (u32, u32), scratch: &mut Scratch) -> Result<Taken> {
        let Some((pts, keyframe)) = self.ready(sent, None)? else {
            return Ok(Taken::Held);
        };
        let time_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let (codec, kbps, made) = {
            let video = self.video.as_mut().expect("ready has opened the stream");
            let Some(encoder) = video.encoder.as_mut() else {
                return Ok(Taken::Held);
            };
            (
                encoder.codec(),
                encoder.bitrate(),
                encoder.encode_dmabuf(shot, scratch, sent, pts, keyframe, time_ms)?,
            )
        };
        let made = match made {
            // A picture the card's own path can't take is dropped, not the path.
            // A compositor hands a buffer of another layout now and then — one
            // it drew for something else, or one tiled differently after a
            // change — and giving the path up for it would cost the share the
            // card's own copy and the pointer the portal paints into its
            // pictures for the rest of the stream. Pictures refused one after
            // another are the layout changed for good, or the device gone, and
            // that is when the encoder FFmpeg opens takes the share over: this
            // picture is converted on the processor and encoded here, exactly
            // as every picture of a share without the rung is, and the ones
            // after it follow the same way.
            #[cfg(nvcuda)]
            Sent::Card(why) => {
                self.refused_pictures = self.refused_pictures.saturating_add(1);
                if self.refused_pictures < REFUSED_PICTURES {
                    if self.refused_pictures == 1 {
                        tracing::info!(
                            why = why.as_str(),
                            "a picture the card can't take where it lies: the frame is dropped"
                        );
                    }
                    return Ok(Taken::Held);
                }
                tracing::info!(
                    why = why.as_str(),
                    pictures = self.refused_pictures,
                    "converting every picture: the card can't take them where they lie"
                );
                self.diagnosis.fallback = Some(format!(
                    "the card can't take a picture where it lies: {why}"
                ));
                // No encoder at all for this codec is the answer a picture
                // nothing here can read gets, which moves the share to the
                // capturer that has pixels rather than failing it.
                let mut encoder = match Encoder::new(
                    codec,
                    sent.0,
                    sent.1,
                    self.fps,
                    kbps,
                    self.effort,
                    self.hardware,
                ) {
                    Ok(encoder) => encoder,
                    Err(error) => return Ok(Taken::Processor(format!("{error:#}"))),
                };
                let made = pictured(&mut encoder, shot, scratch, sent, pts, keyframe, time_ms)?;
                let video = self.video.as_mut().expect("ready has opened the stream");
                video.encoder = Some(Encoding::Picture(encoder));
                made
            }
            made => made,
        };
        match made {
            Sent::Frame { frame, converted } => {
                // The card's path has taken a picture again, so the ones before
                // it were its now and then rather than gone.
                #[cfg(nvcuda)]
                {
                    self.refused_pictures = 0;
                }
                // Which encoder opened doesn't change under a stream, so the
                // answer to "does it read a descriptor" would be the same for
                // every frame after this one: it is said once, in the log and in
                // the share's report, and a reader then knows why a frame cost a
                // copy.
                if let Some(refused) = converted
                    && self.diagnosis.fallback.is_none()
                {
                    tracing::info!(
                        why = %refused,
                        "converting every picture: the encoder reads pictures"
                    );
                    self.diagnosis.fallback =
                        Some(format!("the encoder reads pictures: {refused}"));
                }
                self.delivered(codec, Ok(frame), keyframe)?;
                Ok(Taken::Sent)
            }
            Sent::Unreadable(why) => Ok(Taken::Processor(why)),
            // The card's own path answered this one where it was given up,
            // above: a picture either comes back from it ready to send or the
            // path is gone. Were one to reach here, it is one nothing here can
            // send.
            #[cfg(nvcuda)]
            Sent::Card(why) => Ok(Taken::Processor(why)),
        }
    }

    /// The same for a frame the card captured as a texture `content` big, at
    /// the size the stream sends, `size`: a screen larger than the stream is
    /// scaled to it on the card, in one pass through the driver's video
    /// processor or its shader, and its encoder reads the result where it
    /// lies. No pixel of the frame is converted, uploaded or copied by the
    /// processor; the one copy it still costs is its bitstream out of the
    /// driver.
    #[cfg(any(wgc, dup))]
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
        // A card whose FFmpeg encoder reads a surface takes the texture itself;
        // NVENC's own session scales it on the card first and reads that. Both
        // hand back an access unit, so what comes after is the same either way.
        let made = match video.encoder.as_mut() {
            // A card whose FFmpeg encoder reads a surface takes the texture the
            // capture filled, as it is.
            #[cfg(all(target_os = "windows", hwenc))]
            Some(Encoding::QsvTextures(encoder)) => {
                let codec = encoder.codec();
                // The capture's picture is BGRA at its own size where the surface
                // the session reads is NV12 at the stream's: the card's video
                // processor is the one step between them, and it costs the
                // processor nothing.
                #[cfg(d3d11conv)]
                let picture = {
                    let fits = video
                        .converter
                        .as_ref()
                        .is_some_and(|converter| converter.fits(content, size));
                    if !fits {
                        video.converter = Some(
                            d3d11::Converter::new(device, content, size)
                                .map_err(|error| anyhow::anyhow!("{error:#}"))?,
                        );
                    }
                    let converter = video.converter.as_mut().expect("made above");
                    converter.run(&texture)?
                };
                #[cfg(not(d3d11conv))]
                let picture = {
                    let _ = (content, device);
                    texture.handle
                };
                (codec, encoder.encode_texture(picture, 0, pts, keyframe))
            }
            Some(Encoding::Texture(encoder)) => {
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
            }
            _ => return Ok(()),
        };
        let (codec, made) = made;
        match made? {
            Some(frame) => {
                video.sender.send(stream_connection::Frame {
                    data: frame.data,
                    codec,
                    timestamp: frame.timestamp,
                    keyframe: frame.keyframe,
                });
            }
            // A session that answered with nothing for this picture — a
            // hardware encoder's first frame often does — has not made the
            // whole frame a viewer asked for, so the request is put back
            // rather than taken for answered: a still desktop would otherwise
            // never produce it.
            None if keyframe => video.sender.force_keyframe(),
            None => {}
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
    #[cfg(any(wgc, dup))]
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
        let (width, height) = preview_size(content.0, content.1, self.constrained());
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

    /// Publishes the tile of a frame the card captured as a picture the UI's
    /// window shows where the card drew it: the same size the tile is read back
    /// at, scaled on the GPU into a texture both processes hold, and one handle
    /// and two numbers published where the tile's pixels would have been.
    ///
    /// Answers whether the UI can show such a picture at all. `false` is an
    /// answer for the rest of the stream rather than for this frame: whether a
    /// pair of adapters can share a texture doesn't change while the stream
    /// lasts, and the tile's pixels keep going out exactly as they did, so a UI
    /// that can't show this one has nothing to fall back to but what it already
    /// has.
    #[cfg(any(wgc, dup))]
    fn share_from_card(
        &mut self,
        slot: u32,
        tile: &mut d3d11::D3d11Encoder,
        shared: &mut d3d11::Shared,
        texture: d3d11::Texture,
    ) -> Result<bool> {
        let (texture, fence_value) = shared.draw(slot, tile, texture)?;
        let (width, height) = shared.size();
        Ok(self.writer.publish_into(&Drawn {
            luid: shared.luid(),
            fence: shared.fence(),
            texture,
            fence_value,
            width,
            height,
        }))
    }

    /// The preview, and Discord's thumbnail when one is due: the picture scaled
    /// down to fit, written for the UI, and a JPEG of it when it isn't black.
    /// This is the frame's own work, and nothing makes one while the UI shows
    /// no preview and none is due for Discord.
    fn preview(&mut self, picture: &I420Buffer) {
        let (width, height) = (picture.width(), picture.height());
        let due = self.preview_due.is_some_and(|due| due <= Instant::now());
        if self.preview_shown || due {
            // The tile's own size while only the preview needs the picture, so a
            // small tile costs a small frame and a large one grows the frames
            // file once. Discord's still is made from the wider box, since a
            // tile smaller than the still would cost it its detail.
            let box_size = match (self.preview_tile, due) {
                (Some(tile), false) => tile,
                _ => (PREVIEW_WIDTH, PREVIEW_HEIGHT),
            };
            let preview = fit(width, height, box_size.0, box_size.1);
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
    /// itself: the stream opened at this size and this codec, the rate the
    /// sender's congestion controller asks for followed, and the frame's own time
    /// and whether it has to be whole. None when the frame is dropped instead — the queue is full, or
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
                refused: None,
                #[cfg(all(target_os = "windows", hwenc, d3d11conv))]
                converter: None,
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
        // size the frames are now, whether or not that changed. The session that
        // is going is let go of first, before another opens: a driver does not
        // survive one session's teardown while another is live (see
        // [`Sink::offers`]), and a switch to the codec Discord chose is exactly
        // that.
        if resized
            || video.reopen
            || video
                .encoder
                .as_ref()
                .is_none_or(|encoder| encoder.codec() != codec)
        {
            if let Some(video) = self.video.as_mut() {
                video.encoder = None;
            }
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
        // The encoder follows the stream's target, which the controller moves
        // a report at a time. Not on every wobble: a tenth is the width of a
        // change worth telling the rate control about.
        let wanted = (video.sender.target() / 1000) as u32;
        // A rate the encoder refused is not asked for again until the target
        // moves: the condition would stay true every frame otherwise, and a
        // warning a frame is a storm in the log and an allocation with it.
        if wanted.abs_diff(aimed) > aimed / 10 && video.refused != Some(wanted) {
            match video
                .encoder
                .as_mut()
                .expect("opened above")
                .set_bitrate(wanted)
            {
                Ok(()) => {
                    video.refused = None;
                    tracing::debug!(kbps = wanted, "the encoder follows the stream's target");
                }
                Err(error) => {
                    video.refused = Some(wanted);
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
        // How long the picture waited on this side of the stream: the capture
        // had it when the pass began, and the encoder is given it now. A frame
        // that came from the capturer's own thread says nothing, since nothing
        // here saw when it arrived.
        self.diagnosis.queue_delay_ms = self
            .diagnosis
            .captured_at
            .take()
            .map(|at| u32::try_from(at.elapsed().as_millis()).unwrap_or(u32::MAX));
        Ok(Some((pts, keyframe)))
    }

    /// Puts an encoded frame on the stream, as the packetizer, its resend history
    /// and the ceiling on keyframe requests have always seen them: the same
    /// bytes, the same time on the stream clock and the same `keyframe` flag,
    /// whichever encoder made them.
    fn delivered(
        &mut self,
        codec: Codec,
        made: Result<Option<encode::Frame>>,
        keyframe: bool,
    ) -> Result<()> {
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
            // Nothing came out of this picture: the whole frame a viewer asked
            // for is still owed, so the request goes back rather than being
            // taken for answered.
            Ok(None) if keyframe => video.sender.force_keyframe(),
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
    #[cfg(any(wgc, dup, target_os = "linux"))]
    fn preview_wanted(&self) -> bool {
        self.preview_shown || self.preview_due.is_some_and(|due| due <= Instant::now())
    }

    /// Whether Discord's still of the stream is due, which is made from the
    /// tile's pixels: the one reason a frame is read back into memory while the
    /// window shows the card's own picture.
    #[cfg(any(wgc, dup, target_os = "linux"))]
    fn still_wanted(&self) -> bool {
        self.preview_due.is_some_and(|due| due <= Instant::now())
    }

    /// Discord's still from the picture the card's own path converted last, in
    /// `scratch.full`: none when the card isn't what encodes this share.
    #[cfg(nvcuda)]
    fn card_still(&self, scratch: &mut Scratch) -> Option<Result<()>> {
        let Some(Encoding::Cuda(cuda)) = self.video.as_ref()?.encoder.as_ref() else {
            return None;
        };
        let (width, height) = cuda.convert.size();
        let full = buffer(&mut scratch.full, width, height);
        let (y_stride, u_stride, v_stride) = full.strides();
        Some(
            cuda.convert
                .still(full.data_mut(), [y_stride, u_stride, v_stride]),
        )
    }

    /// The preview of a window that draws on the processor, made on the card
    /// from the picture the card's own path took last, at the size a preview is
    /// made at, in `scratch.full`: only its pixels come back. None when the card
    /// isn't what encodes this share, and an error for a picture the card holds
    /// no copy of — one whose rows the processor reads itself.
    #[cfg(nvcuda)]
    fn card_preview(&self, scratch: &mut Scratch) -> Option<Result<()>> {
        let Some(Encoding::Cuda(cuda)) = self.video.as_ref()?.encoder.as_ref() else {
            return None;
        };
        let (width, height) = cuda.convert.size();
        let (width, height) = fit(width, height, PREVIEW_WIDTH, PREVIEW_HEIGHT);
        let full = buffer(&mut scratch.full, width & !1, height & !1);
        let (y_stride, u_stride, v_stride) = full.strides();
        Some(cuda.convert.preview(
            (width, height),
            full.data_mut(),
            [y_stride, u_stride, v_stride],
        ))
    }

    /// Whether a machine short of memory has this share giving back what it can
    /// spare: a smaller preview, and buffers it will never use again.
    #[cfg(any(wgc, dup))]
    fn constrained(&self) -> bool {
        self.diagnosis.memory.constrained().is_some()
    }

    /// Whether the stream needs a picture now, whatever the screen is doing: a
    /// connection that hasn't opened yet, an encoder the card took away or a
    /// codec Discord chose since the last frame, or a viewer whose decoder lost
    /// its place and asked for a whole one. A stick-still desktop is otherwise
    /// left alone: nothing here is about the preview.
    #[cfg(any(wgc, dup, target_os = "linux"))]
    fn picture_needed(&self) -> bool {
        let Some(video) = self.video.as_ref() else {
            // No stream yet: the first picture is what opens it, and what a
            // connection asks for.
            return self.server.is_some();
        };
        video.encoder.is_none() || video.reopen || video.sender.keyframe_wanted()
    }

    /// Hands the next pictures to the processor's encoder, for a window, a
    /// camera or a screen the card couldn't open. The session that read the
    /// card's textures goes with the device that made it, and the change of
    /// kind asks for a whole frame, so a viewer's decoder starts on the
    /// pictures that come rather than depending on the last one the card made.
    #[cfg(any(wgc, dup, target_os = "linux"))]
    fn fall_back_to_the_processor(&mut self) {
        // The device the encoder read on goes with the capture that made it, and
        // a session that outlived its device reads memory nobody owns. A Linux
        // share's encoder read nothing on the capture's behalf — its frames were
        // pictures converted on the processor either way — so it keeps it.
        #[cfg(any(wgc, dup))]
        self.let_go();
        self.footage = Footage::InMemory;
        self.diagnosis.presentation = Some(Presentation::Copy);
        self.diagnosis.fallback = Some("the processor captures this source".to_owned());
        if let Some(video) = self.video.as_mut() {
            video.sender.force_keyframe();
        }
    }

    /// Lets go of the stream's encoder, keeping the rest: the device the
    /// encoder read on is about to go with the capture that made it, and a
    /// session that outlived its device reads memory nobody owns. The next
    /// frame opens another on the device the capture has then.
    #[cfg(any(wgc, dup))]
    fn let_go(&mut self) {
        if let Some(video) = self.video.as_mut() {
            video.encoder = None;
            video.reopen = true;
        }
    }

    /// The codecs to offer Discord, most wanted first, and an encoder for
    /// the first: AV1, then H265, where the GPU's encoder for each opens,
    /// then H264. Discord streams the first every viewer decodes. The
    /// settings' own codec, where one is chosen and this machine encodes it,
    /// is put in front of all of them.
    ///
    /// A card's own session is asked which codecs its driver encodes rather
    /// than another opened for each codec and given up beside it: a driver does
    /// not survive a session's teardown while another is live. A share's worker
    /// died with SIGSEGV in `libnvcuvid`'s own thread a second after its two
    /// codec probes had been let go of, the kept session encoding at the time,
    /// so what a card can send is read from the session that stays open. An
    /// encoder FFmpeg opens has no such list to read, and is opened for each
    /// codec as it always was.
    fn offers(
        &self,
        width: u32,
        height: u32,
        device: Option<*mut c_void>,
    ) -> Result<(Vec<Codec>, Encoding)> {
        // The settings' codec is asked for first, so the encoder opened for the
        // stream is that codec's. H264 joins the probe only where the setting
        // names it: it is the stream's fallback whatever the setting says.
        let choice = stream_codec();
        let mut probes = GPU_CODECS.to_vec();
        if let Some(wanted) = choice.wanted().filter(|wanted| !probes.contains(wanted)) {
            probes.push(wanted);
        }
        let mut first = None;
        for codec in preferred(probes, choice)
            .into_iter()
            .filter(|_| self.hardware)
        {
            if let Ok(encoder) = self.open(codec, width, height, device) {
                first = Some(encoder);
                break;
            }
        }
        let encoder = match first {
            Some(encoder) => encoder,
            None => self.open(stream_connection::SEND_CODEC, width, height, device)?,
        };
        let mut offers = match &encoder {
            // The codecs the session's driver encodes, which is what the card
            // can send: nothing else is opened to ask.
            #[cfg(nvcuda)]
            Encoding::Cuda(cuda) => cuda.encoder.driver_codecs().unwrap_or_default(),
            // What FFmpeg's encoders can send, asked by opening one for each
            // codec: a hardware encoder has no list of its own to read.
            _ => {
                let mut offers = vec![encoder.codec()];
                for codec in GPU_CODECS
                    .iter()
                    .copied()
                    .filter(|codec| self.hardware && *codec != encoder.codec())
                {
                    if self.open(codec, width, height, device).is_ok() {
                        offers.push(codec);
                    }
                }
                offers.sort_by_key(|codec| GPU_CODECS.iter().position(|one| one == codec));
                offers
            }
        };
        if !offers.contains(&stream_connection::SEND_CODEC) {
            offers.push(stream_connection::SEND_CODEC);
        }
        // A codec the setting names that this machine has no encoder for is
        // nothing to reorder around: the share offers what it can, and says so
        // once, rather than failing to start.
        if let Some(wanted) = choice.wanted().filter(|wanted| !offers.contains(wanted)) {
            tracing::warn!(
                wanted = wanted.name(),
                "the settings ask for a codec this machine doesn't encode; the share offers the \
                 ones it does"
            );
        }
        let offers = preferred(offers, choice);
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

    /// An encoder in `codec`, at the stream's target, or what a stream of this
    /// size opens at before it has one, reading the frames where this share's
    /// capture puts them: on the card, a session opens on the device the
    /// capture fills its textures on, which the stream hands over as `device`.
    fn open(
        &self,
        codec: Codec,
        width: u32,
        height: u32,
        _device: Option<*mut c_void>,
    ) -> Result<Encoding> {
        // Where the controller has got to, so an encoder opened mid-stream
        // starts at the rate the stream sends at rather than at the declared
        // ceiling a path may not carry; a stream's first encoder, opened
        // before the stream has a target, starts where the controller would
        // open it.
        let aimed = self.video.as_ref().map_or_else(
            || congestion::start(u64::from(kbps(width, height, self.fps)) * 1000) / 1000,
            |video| video.sender.target() / 1000,
        ) as u32;
        match self.footage {
            // The card's session reads a texture only on the device it opened
            // on, so it opens on the capture's own, which is the whole of what
            // lets a frame reach it without being copied first. It reads the
            // ring the capture fills, so it keeps the surfaces of one.
            #[cfg(any(wgc, dup))]
            Footage::OnTheCard => {
                let device = _device.context("the card's encoder needs the capture's device")?;
                // A card whose FFmpeg encoder reads a capture's own texture —
                // Quick Sync and AMF — is opened on the capture's device and
                // handed the texture itself. NVENC has no such path in FFmpeg,
                // so it keeps the native session that scales on the card.
                #[cfg(all(target_os = "windows", hwenc))]
                if !encode::planned(codec).is_some_and(|name| name.ends_with("_nvenc"))
                    && surfaces_offered()
                    && let Ok(encoder) = encode::Encoder::on_device(
                        codec,
                        width,
                        height,
                        self.fps,
                        aimed,
                        self.effort,
                        self.hardware,
                        device,
                    )
                    && encoder.took_surfaces()
                {
                    return Ok(Encoding::QsvTextures(encoder));
                }
                Ok(Encoding::Texture(d3d11::D3d11Encoder::open_on(
                    device,
                    codec,
                    width,
                    height,
                    self.fps,
                    aimed,
                    d3d11::Surfaces::Ring,
                )?))
            }
            // A picture the desktop's compositor drew is a descriptor the encoder
            // is handed itself, which an encoder that reads memories or an
            // import it doesn't carry answers with a refusal: the session that
            // reads pictures is opened for it either way, and it is the one
            // [`Sink::dmabuf`] falls back to. The device argument is the card's
            // texture path's and nothing here opens on it.
            Footage::InMemory => Ok(Encoding::Picture(Encoder::new(
                codec,
                width,
                height,
                self.fps,
                aimed,
                self.effort,
                self.hardware,
            )?)),
            // The session that reads pictures, which is also the one [`Sink::dmabuf`]
            // falls back to: the card's own on a machine with NVIDIA's, or an
            // encoder FFmpeg opens on any other.
            #[cfg(target_os = "linux")]
            Footage::Dmabuf => {
                // NVENC is on the card the picture is already on, so the driver
                // imports it where the compositor drew it and converts it there,
                // and the session that reads it needs no copy of any pixel. On a
                // machine with NVIDIA's card that is the path to open first:
                // FFmpeg has no way from a descriptor into NVENC, so its session
                // would be one opened for a picture it can't read and given up
                // again — a teardown the card's own session does not survive (see
                // [`Sink::offers`]), and a log line for an encoder that never
                // encodes a frame. A card that can't take the path says why once,
                // and every picture goes the processor's way, through the session
                // opened below.
                #[cfg(nvcuda)]
                if encode::nvidia_card()
                    && let Some(cuda) = Cuda::open(codec, width, height, self.fps, aimed)
                {
                    return Ok(Encoding::Cuda(cuda));
                }
                Ok(Encoding::Picture(Encoder::new(
                    codec,
                    width,
                    height,
                    self.fps,
                    aimed,
                    self.effort,
                    self.hardware,
                )?))
            }
        }
    }

    /// Takes Yapfast's controls, reports the stream starting, and fails once
    /// its connection ended. Gives the source a [`Control::Switch`] asked for.
    fn check(
        &mut self,
        reading: pressure::Reading,
        report: &mut dyn FnMut(Report),
    ) -> Result<Option<Source>> {
        // The machine's memory, sampled by the loop once a second rather than
        // from a thread of its own: a share gives back what it can spare while
        // it is short, and says so in its report until it has had the room back
        // for a while. The stream that goes out is never what is given up.
        let now = Instant::now();
        match self.diagnosis.memory.update(reading, now) {
            pressure::Change::Enter(reason) => {
                tracing::info!(
                    why = reason.name(),
                    "the machine is short of memory; a share keeps only what it sends"
                );
                if let Some(video) = self.video.as_mut() {
                    video.trim_pool();
                }
            }
            pressure::Change::Leave => {
                tracing::info!("the machine has room again; a share takes its own back")
            }
            pressure::Change::Same => {}
        }
        let mut switch = None;
        while let Ok(control) = self.controls.try_recv() {
            match control {
                Control::Send(server) => self.server = Some(server),
                Control::Switch(source) => switch = Some(source),
                Control::Preview(shown) => self.preview_shown = shown,
                // The tile showing this share's preview says how large it is
                // when that changes, not with every frame: the preview is
                // written at the size it shows at from then on, and a larger
                // tile grows the frames file once.
                Control::Size { width, height } => {
                    self.preview_tile = tile_size((width, height));
                }
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
            let mut sending = video.sending(self.fps);
            // What is captured, which is the half a person can't see from the
            // picture: a screen shared from the card and the same screen
            // shared from the processor look alike once they arrive.
            sending.capture = self.capture.clone();
            // And where it went: the adapters, the way the window shows it,
            // whether a preview is being made, why the share isn't on the path
            // it asked for, and what the machine had it give back.
            sending.presentation = self.diagnosis.presentation;
            sending.capture_luid = self.diagnosis.gpu_luid;
            sending.encode_luid = self.diagnosis.gpu_luid;
            sending.present_luid = self.writer.presented_on();
            sending.preview = self.preview_shown;
            sending.fallback = self.diagnosis.fallback.clone();
            sending.queue_delay_ms = self.diagnosis.queue_delay_ms;
            sending.memory = self
                .diagnosis
                .memory
                .constrained()
                .map(|reason| reason.name().to_owned());
            report(Report::Sending(sending));
        }
        Ok(switch)
    }
}

/// What decodes a watched stream: the card, where the machine's decoder left
/// the picture there and only the tile comes back, or the processor, which
/// reads every frame.
fn decoder_name(decoder: Option<&Live>) -> String {
    match decoder {
        Some(decoder) if decoder.on_the_card() => "the card's decoder".to_owned(),
        _ => "the processor's decoder".to_owned(),
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
    // The clock the streamer's sender reports put its sound and picture on,
    // read before the player takes the sounds: what places each of the two
    // positions a viewer's lip sync is measured from.
    let sounds = receiver.take_sounds();
    let clock = sounds.as_ref().map(stream_connection::SoundReceiver::clock);
    let player = sounds.and_then(|sounds| {
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
    // When the watch last said how it is decoding and showing the stream: the
    // same once a second a share's report keeps, so a viewer's Stream tab has
    // something true to show too.
    let mut told = Instant::now() - SENDING_EVERY;
    // The streamer's two clocks as its sender reports put them, and where the
    // sound audible now sits on the picture's timeline: the sound is the master
    // the pictures are judged against. Both reset on a stop, a deafen or a
    // clock the streamer broke, since nothing measured across one means anything
    // after.
    let mut timeline = av_timing::Timeline::new();
    // The stream time of the picture last handed to the window and how its
    // time is known, which the presentation skew is read against.
    let mut presented: Option<av_timing::Presented> = None;
    // The pictures the sound had passed by, said once a second with the rest of
    // how the watch is doing.
    let mut dropped = 0u64;
    while !stop.load(Ordering::Relaxed) {
        while let Ok(control) = sound.controls.try_recv() {
            match control {
                Control::Hear(hearing) => {
                    sound.hearing.store(hearing, Ordering::Relaxed);
                    // Deafened: nothing of this stream is heard, so the sound
                    // is no longer what the picture is timed against.
                    if !hearing {
                        timeline.reset();
                        presented = None;
                    }
                }
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
        // What decodes this stream, read before the decoder is borrowed to
        // decode: a watch says it once a second while it plays.
        let decoder = decoder_name(decoding.decoder.as_ref());
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
        // How the watch is doing, said once a second while it plays: a hidden
        // tile is still a stream being decoded, and its report says so rather
        // than going quiet and leaving the call's Stream Tab with nothing.
        if told.elapsed() >= SENDING_EVERY {
            told = Instant::now();
            let (width, height) = picture.size();
            // The streamer's two clocks, read where the connection keeps them:
            // its sender reports come about once a second, so this is read
            // once a second too rather than locking them for every frame. The
            // timeline carries each on from its own report between reads.
            let now = Instant::now();
            let (mut heard, mut queued) = (None, None);
            if let Some(clock) = clock.as_ref() {
                if let Some(report) = clock.audio() {
                    timeline.heard(av_timing::Clock::Sound, mapping_of(report));
                }
                if let Some(report) = clock.video() {
                    timeline.heard(av_timing::Clock::Picture, mapping_of(report));
                }
            }
            if let Some((played, held)) = player.as_ref().and_then(stream_audio::Player::heard) {
                heard = Some(played);
                queued = Some(held);
            }
            // How far the picture runs from the sound, on the streamer's own
            // timeline: the two clocks carried to now, and what the device
            // says it has played against the picture last handed over. Both
            // are `None` while a mapping is missing or older than five
            // seconds, and the device's buffer is only in the second.
            let held = Duration::from_millis(u64::from(queued.unwrap_or(0)));
            let arrival_skew = timeline.arrival_ms(now);
            let presentation_skew =
                timeline.presentation_ms(now, held, presented.map(|presented| presented.time_ms));
            tracing::debug!(
                arrival_skew_ms = ?arrival_skew,
                presentation_skew_ms = ?presentation_skew,
                dropped,
                "watched stream A/V timing"
            );
            report(Report::Sending(StreamStats {
                codec: frame.codec.name().to_owned(),
                decoder: Some(decoder.clone()),
                // A watched picture is converted into the frames file for the
                // window to show: this side copies it, and says so.
                presentation: Some(Presentation::Copy),
                present_luid: writer.presented_on(),
                width,
                height,
                preview: visible,
                sound_heard_ms: heard,
                sound_queued_ms: queued,
                arrival_skew_ms: arrival_skew,
                presentation_skew_ms: presentation_skew,
                late_pictures: Some(dropped),
                ..StreamStats::default()
            }));
        }
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
        // Only the Windows decode path below sets it.
        #[cfg_attr(not(d3d11dec), allow(unused_mut))]
        let mut abandoned = false;
        // The picture's own time on the stream, which the frames file carries to
        // the window: the sender's 90 kHz RTP clock in milliseconds, so a
        // picture is told from the one before it and can be matched against the
        // sound when the two are put on one timeline.
        let time_ms = u64::from(frame.timestamp) * 1_000 / 90_000;
        // The sound already audible is the master: a picture it has passed by
        // more than 80 ms is dropped rather than shown late. One it hasn't
        // reached is shown when it arrives: a decoded picture lives only until
        // the decoder's next frame, so holding it back would mean copying it to
        // keep it, which is a picture's worth of memory and a copy per frame
        // for a wait of at most 80 ms. Nothing here blocks, so neither the UI
        // nor the network thread waits on a picture's turn.
        let now = Instant::now();
        let held = player
            .as_ref()
            .and_then(stream_audio::Player::heard)
            .map_or(Duration::ZERO, |(_, queued)| {
                Duration::from_millis(u64::from(queued))
            });
        if let Some(audible) = timeline.audible_ms(now, held)
            && av_timing::decide(time_ms, audible) == av_timing::Show::Drop
        {
            dropped += 1;
            continue;
        }
        let (width, height) = match picture {
            Frame::Software(picture) => write_picture(
                &mut writer,
                &picture.i420(),
                &mut scratch,
                (max_width, max_height),
                time_ms,
            ),
            // The machine's decoder left the picture on the graphics card, at
            // the size the stream was encoded at: the card scales it to the
            // tile in the same pass that reads its colour, and only the tile
            // comes back to the processor.
            #[cfg(d3d11dec)]
            Frame::Hardware(hardware) => {
                let (stream_width, stream_height) = hardware.size();
                let (width, height) = fit(stream_width, stream_height, max_width, max_height);
                let written = writer.write_timed_rendered(width, height, time_ms, |pixels| {
                    hardware.rgba(pixels, width, height).is_ok()
                });
                if !written {
                    abandoned = true;
                }
                (width, height)
            }
            // The card's decoder on Linux leaves the picture in a DMA-BUF: no
            // pixel of it is copied, and the window imports it where it lies if
            // it can. A window that can't is what the publish says so, and the
            // stream falls back to the processor's decoder from a keyframe —
            // the picture before this one stays up rather than a blank tile.
            #[cfg(vaapidec)]
            Frame::Hardware(hardware) => {
                let (stream_width, stream_height) = hardware.size();
                let (width, height) = fit(stream_width, stream_height, max_width, max_height);
                let published = hardware
                    .dmabuf()
                    .is_some_and(|picture| writer.publish_dmabuf(&picture));
                hardware.release();
                if !published {
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
        // What reached the window, for the presentation skew: a worker hands
        // a picture over through the frames file and the renderer's own
        // completion is the window's to observe, so this is its submission and
        // says so.
        if !abandoned {
            presented = Some(av_timing::Presented::submitted(time_ms, Instant::now()));
        }
        if first {
            // What decodes it and where it goes, which is the half a viewer
            // can't tell from the picture: the card leaves it on the card and
            // only the tile comes back, and the processor reads every frame.
            tracing::info!(
                width,
                height,
                codec = ?frame.codec,
                on_the_card = decoding
                    .decoder
                    .as_ref()
                    .is_some_and(Live::on_the_card),
                "showing the stream"
            );
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
    time_ms: u64,
) -> (u32, u32) {
    let (width, height) = fit(picture.width, picture.height, max_width, max_height);
    writer.write_timed(width, height, time_ms, |pixels| {
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

/// One of the streamer's clocks as its last sender report puts it, in the
/// timeline's own terms. The connection keeps the two separately, so which
/// clock a report is of is already known here and no SSRC has to be carried.
fn mapping_of(report: stream_connection::SenderReport) -> av_timing::Mapping {
    av_timing::Mapping {
        ntp: report.ntp,
        rtp: report.rtp,
        heard: report.heard,
    }
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
    // Colour bars cost nothing to encode, so the pattern stays small unless a
    // measurement asks for a size: see `YAPFAST_STREAM_MAX_BITRATE`.
    let (width, height) = if std::env::var_os("YAPFAST_STREAM_MAX_BITRATE").is_some() {
        (width, height)
    } else {
        (width.min(640), height.min(360))
    };
    report(Report::Sharing { width, height });
    let started = Instant::now();
    let mut slot = 0u64;
    let mut rgba = vec![0; (width * height * 4) as usize];
    let mut picture = I420Buffer::new(width, height);
    let mut sampler = pressure::Sampler::new();
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
        let _switch = sink.check(sampler.read(Instant::now()), report)?;
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
///
/// The box is a landscape frame — a stream's resolution is named that way — and
/// it turns with the picture, because a screen on its side is the same screen:
/// a 1080x1920 display shared at 1080p is 1080x1920, where a box that stayed
/// on its feet would leave it 606x1080 and a third of its width behind. A
/// square box fits either way round.
pub fn fit(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let (width, height) = (width.max(1), height.max(1));
    let (max_width, max_height) = if (width > height) == (max_width > max_height) {
        (max_width, max_height)
    } else {
        (max_height, max_width)
    };
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(max_width) / f64::from(width),
            f64::from(max_height) / f64::from(height),
        ),
    );
    let even = |value: f64| ((value as u32) & !1).max(2);
    (
        even(f64::from(width) * scale),
        even(f64::from(height) * scale),
    )
}

/// The size a preview of a `width` by `height` frame is carried at: the stream's
/// own picture, scaled down to [`PREVIEW_LONG_EDGE`] at most.
///
/// A preview drawn on the card costs nothing a frame — it is the same scale that
/// fills the texture the UI shows — so it keeps every pixel the stream has, a
/// portrait screen included. The pixels read back for a stream the card can't
/// share a picture with stop at [`PREVIEW_WIDTH`] by [`PREVIEW_HEIGHT`], since
/// that copy is made on the processor for every frame it is made for. A machine
/// short of memory takes the smaller box as well, whatever the path: the tile
/// is the one thing a share can give up, and the stream it sends is untouched.
#[cfg(any(wgc, dup))]
fn preview_size(width: u32, height: u32, constrained: bool) -> (u32, u32) {
    if constrained {
        return fit(width, height, CONSTRAINED_WIDTH, CONSTRAINED_HEIGHT);
    }
    fit(width, height, PREVIEW_LONG_EDGE, PREVIEW_LONG_EDGE)
}

/// The box a preview is scaled into when the machine is short of memory, which
/// is smaller than any tile it is shown in and never upscales: what it saves is
/// the texture, the readback and the scale itself, not the picture that goes out.
#[cfg(any(wgc, dup))]
const CONSTRAINED_WIDTH: u32 = 640;
#[cfg(any(wgc, dup))]
const CONSTRAINED_HEIGHT: u32 = 360;

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

/// What a share from the card hands back when its loop is done: nothing more
/// to share, or a source the card's path can't take, which the processor's
/// does.
#[cfg(any(wgc, dup, target_os = "linux"))]
enum After {
    /// The share is over: it was stopped, or what it captured closed.
    Ended,
    /// Share `source` next. A window or a camera isn't a screen: the card's
    /// paths take a display, and the processor's capturer is what shares one,
    /// so the stream goes on there.
    Processor(Source),
}

/// Whether the card's capture paths can take `source` at all: they capture a
/// display, where a window, a camera and a test pattern are the processor's
/// or nobody's.
#[cfg(any(wgc, dup, test))]
fn on_the_card(source: &Source) -> bool {
    match source {
        Source::Screen | Source::Display(_) => true,
        // A window is the card's where Windows Graphics Capture can take one:
        // DXGI duplicates a display and nothing else, and the person chose the
        // window rather than the display it sits on.
        Source::Window(_) => cfg!(wgc),
        Source::Camera(_) | Source::TestPattern => false,
    }
}

fn capture_screen(
    source: Source,
    mut sink: Sink,
    max_width: u32,
    max_height: u32,
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<()> {
    // Where a share's frames come from is decided here, and nowhere else: from
    // the card, as the texture Windows Graphics Capture fills or DXGI's
    // Desktop Duplication copies, when this build has either and the machine
    // can encode what it fills; from the processor's memory, as the I420
    // libwebrtc's capturer hands over, everywhere else, which is every machine
    // without NVENC. Everything that keeps a share on the processor is named in
    // the line this logs, so a stream that costs more than it should says why.
    //
    // Linux has the same shape and a different door: the DMA-BUFs the desktop's
    // compositor drew, which the window imports and draws where they lie, from
    // the screen sharing portal where one answers and from the X server's DRI3
    // export where it can. A share that leaves that path — a window with no
    // importer, a picture the encoder can't read — carries on from the
    // processor's capturer, which is where every Linux share used to start.
    #[cfg(any(wgc, dup))]
    let source = {
        let mut wanted = source;
        loop {
            match card_capture(&wanted, &sink, (max_width, max_height)) {
                Ok(chosen) => {
                    let (back, after) =
                        capture_textures(chosen, sink, (max_width, max_height), stop, report)?;
                    sink = back;
                    match after {
                        After::Ended => return Ok(()),
                        // Another screen is opened the way this one was, with
                        // the stream going on; a window or a camera is the
                        // processor's, below.
                        After::Processor(next) if on_the_card(&next) => wanted = next,
                        After::Processor(next) => break next,
                    }
                }
                Err(why) => {
                    tracing::info!(why = format!("{why:#}"), "sharing from the processor");
                    sink.diagnosis.fallback = Some("the card's capture wouldn't open".to_owned());
                    break wanted;
                }
            }
        }
    };
    #[cfg(target_os = "linux")]
    let source = match dmabuf_capture(&source, &mut sink) {
        Ok(opened) => {
            let (back, after) =
                capture_dmabufs(opened, source, sink, (max_width, max_height), stop, report)?;
            sink = back;
            match after {
                After::Ended => return Ok(()),
                // A share that left the card mid-stream stays on the processor:
                // the answer that sent it there — a tiled picture, a window that
                // can't sample one — is the same answer the next attempt would
                // get, and a share that opened and fell back over and over would
                // be a spin.
                After::Processor(next) => next,
            }
        }
        Err(why) => {
            tracing::info!(why = format!("{why:#}"), "sharing from the processor");
            sink.diagnosis.fallback = Some(format!("the processor captures this source: {why:#}"));
            source
        }
    };
    sink.capture = "the processor's capturer".to_owned();
    // Every picture this path makes is a copy: the capturer hands over memory
    // the processor wrote, and the window is shown those pixels, whichever
    // process wrote them.
    sink.diagnosis.presentation = Some(Presentation::Copy);
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
    let mut sampler = pressure::Sampler::new();
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
            bail!(if reported.is_none() && portal_picks() {
                "Screen sharing was cancelled, or this desktop has no screen sharing dialog."
                    .to_owned()
            } else {
                failure
            });
        }
        if let Some((width, height)) = size
            && size != reported
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
        // The machine is read before the sink is locked: the capturer's own
        // thread takes that lock to hand a frame over, and a question about the
        // adapter is not something to hold it for.
        let reading = sampler.read(Instant::now());
        let switch = sink
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .check(reading, report)?;
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

/// One rung of the card's capture path: a display's picture arriving as a
/// texture the encoder reads where it lies, and the capture's own device that
/// texture is on.
///
/// The two rungs differ in which door they come through and not in what they
/// hand over: a borrowed BGRA texture and the device it was made on, so the
/// loop above them, the encoder and the preview are the same either way. Which
/// one a share got is kept here, because rebuilding it after the card takes its
/// device away means opening the same display the same way.
#[cfg(any(wgc, dup))]
enum Card {
    /// Windows Graphics Capture, which draws the pointer and, on a Windows
    /// whose session can't be asked, leaves its yellow border on.
    #[cfg(wgc)]
    Graphics {
        capture: wgc::Capture,
        /// What it captures: a display, or a window the person chose, which is
        /// never reinterpreted as the display it happens to be on.
        target: wgc::Target,
    },
    /// DXGI's Desktop Duplication, which has no border and paints the pointer
    /// into the frame itself. It is the rung a Windows whose Graphics Capture
    /// can't remove its border tries first, and the fallback everywhere else.
    #[cfg(dup)]
    Duplication {
        capture: dup::Capture,
        target: dup::Target,
    },
}

/// What the card had for the loop this pass: a new picture in the capture's
/// texture, a still screen, a size change, or the end of the capture.
#[cfg(any(wgc, dup))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arrival {
    /// The capture's texture holds a new picture. A pointer that moved counts:
    /// the screen shows it somewhere else, and the frame is the picture the
    /// stream should send.
    Frame,
    /// Nothing has been drawn and nothing has moved. This is not a failure and
    /// makes no frame: the next pass is the next slot.
    Still,
    /// The screen changed size and the capture has rebuilt at the new one.
    Resized,
    /// What was captured ended. This rung hands out no more frames.
    Closed,
}

#[cfg(any(wgc, dup))]
impl Card {
    /// Asks for the next frame. `wait` is how long a duplication may wait for
    /// the desktop to change; it is zero because the stream's own slot is the
    /// pace on both rungs, and a capture that waited would spend a second wait
    /// on top of that slot. Graphics Capture's look never waits at all.
    fn frame(&mut self, wait: Duration) -> Result<Arrival> {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => match capture.frame()? {
                wgc::Arrival::Frame => Ok(Arrival::Frame),
                // Nothing drawn since the last look is the compositor's own
                // rate, and the loop's next slot is when it asks again.
                wgc::Arrival::Idle => Ok(Arrival::Still),
                wgc::Arrival::Resized => Ok(Arrival::Resized),
                wgc::Arrival::Closed => Ok(Arrival::Closed),
            },
            // A duplication reports a moved pointer separately from a new
            // desktop image, and both leave the last picture in the texture
            // with the pointer where it is now: either is a frame to send.
            #[cfg(dup)]
            Self::Duplication { capture, .. } => match capture.frame(wait)? {
                dup::Arrival::Frame | dup::Arrival::Pointer => Ok(Arrival::Frame),
                dup::Arrival::Still => Ok(Arrival::Still),
            },
        }
    }

    fn content_size(&self) -> (u32, u32) {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => capture.content_size(),
            #[cfg(dup)]
            Self::Duplication { capture, .. } => capture.content_size(),
        }
    }

    /// The borrowed texture the last [`Arrival::Frame`] left here.
    fn texture(&self) -> *mut c_void {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => capture.texture(),
            #[cfg(dup)]
            Self::Duplication { capture, .. } => capture.texture(),
        }
    }

    /// The device the texture is on, which the encoder has to open on.
    fn device(&self) -> *mut c_void {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => capture.device(),
            #[cfg(dup)]
            Self::Duplication { capture, .. } => capture.device(),
        }
    }

    /// What this capture is, in one line, for the log that says which rung ran.
    fn describe(&self) -> String {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => capture.describe(),
            #[cfg(dup)]
            Self::Duplication { capture, .. } => capture.describe(),
        }
    }

    fn rung(&self) -> capture::Rung {
        match self {
            #[cfg(wgc)]
            Self::Graphics { .. } => capture::Rung::Graphics,
            #[cfg(dup)]
            Self::Duplication { .. } => capture::Rung::Duplication,
        }
    }

    /// Whether this capture's border is off, for a rung that draws one: a
    /// duplication never does, and answers `None` rather than a `true` that
    /// would read as something it did.
    fn borderless(&self) -> Option<bool> {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => Some(capture.borderless()),
            #[cfg(dup)]
            Self::Duplication { .. } => None,
        }
    }

    /// The border's answer in the shim's own words, for the share that lets a
    /// bordered capture go and has to say what refused it.
    fn border(&self) -> String {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, .. } => capture.border(),
            #[cfg(dup)]
            Self::Duplication { .. } => String::new(),
        }
    }

    /// Opens the capture again, of the display it was of, after the card took
    /// the device it was on away. The old capture is let go first, not after:
    /// DXGI gives an output to one duplicator at a time, so asking for the new
    /// one while the old one still holds it would be answered with another
    /// duplicator already having it. The encoder must already be let go too: a
    /// session read the old device, and this capture makes a new one.
    fn reopen(self) -> Result<Self> {
        match self {
            #[cfg(wgc)]
            Self::Graphics { capture, target } => {
                drop(capture);
                Ok(Self::Graphics {
                    capture: wgc::Capture::open(target)?,
                    target,
                })
            }
            #[cfg(dup)]
            Self::Duplication { capture, target } => {
                drop(capture);
                Ok(Self::Duplication {
                    capture: dup::Capture::open(target)?,
                    target,
                })
            }
        }
    }
}

/// A capture that opened, and why it is the one sharing: the setting's choice,
/// or what the fallbacks went through to reach it. The sentence goes into the
/// one log line that says which rung ran, so a share that fell through, or that
/// let a bordered capture go, says why where a reader is already looking.
#[cfg(any(wgc, dup))]
struct Chosen {
    card: Card,
    why: String,
}

/// Opens `rung`'s capture of `source`.
#[cfg(any(wgc, dup))]
fn open_rung(
    rung: capture::Rung,
    source: &Source,
    sink: &Sink,
    bounds: (u32, u32),
) -> Result<Card> {
    match rung {
        #[cfg(wgc)]
        capture::Rung::Graphics => graphics_capture(source, sink, bounds),
        #[cfg(dup)]
        capture::Rung::Duplication => duplication(source, sink, bounds),
        // A rung this build doesn't carry is never in the order; the arm keeps
        // a build with one carrier a complete match.
        #[cfg(not(all(wgc, dup)))]
        _ => bail!("this build of Yapfast has no {rung:?} capture"),
    }
}

/// Adds one rung's refusal to the line a share reports, in the order they were
/// tried, naming the rung and what it said.
#[cfg(any(wgc, dup))]
fn refuse(into: &mut String, rung: capture::Rung, reason: &str) {
    if !into.is_empty() {
        into.push_str("; ");
    }
    into.push_str(rung.name());
    into.push_str(": ");
    into.push_str(reason);
}

/// Why a share captures the way it does, in the sentence its one log line
/// carries. With nothing refused it is the choice itself — the setting's, or
/// Auto's reading of what this Windows can do about the border. With refusals
/// before it, it is those and the rung that came next: that is where a bordered
/// Graphics Capture that was let go says so, in the shim's own words.
#[cfg(any(wgc, dup))]
fn chosen_why(
    preference: capture::Preference,
    platform: capture::Platform,
    available: capture::Available,
    rung: capture::Rung,
    refused: &str,
) -> String {
    if !refused.is_empty() {
        return format!("{refused}; {} came next", rung.name());
    }
    let why = match (preference, rung) {
        (capture::Preference::GraphicsCapture, _) => {
            "the setting asks for Windows Graphics Capture"
        }
        (capture::Preference::Duplication, _) => "the setting asks for Desktop Duplication",
        // A build whose Windows SDK has no capture headers carries no Graphics
        // Capture rung at all, which is a different reason from a Windows that
        // has one and can't remove its border.
        (capture::Preference::Auto, _) if !available.graphics => {
            "this build has no Windows Graphics Capture, so Desktop Duplication captures"
        }
        (capture::Preference::Auto, capture::Rung::Graphics) if platform.can_remove_border() => {
            "this build of Windows can take Graphics Capture's border off, so it comes first"
        }
        (capture::Preference::Auto, capture::Rung::Graphics) => {
            "this build of Windows cannot take Graphics Capture's border off, and it is the rung \
             there is"
        }
        (capture::Preference::Auto, capture::Rung::Duplication) => {
            "this build of Windows cannot take Graphics Capture's border off, so Desktop \
             Duplication comes first"
        }
    };
    why.to_owned()
}

/// The monitor Windows knows a listed screen by.
///
/// libwebrtc names a screen by its place in the system's own enumeration — the
/// number a [`Source::Display`] carries is that place, `0` for the first — and
/// the card's capture paths take the `HMONITOR` the same enumeration hands out.
/// They are not the same number, so the place is turned back into the handle
/// here: a picked screen was otherwise a capture of an HMONITOR that doesn't
/// exist, which both rungs refused and left on the processor.
#[cfg(any(wgc, dup))]
fn monitor_of(place: u64) -> Result<u64> {
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
    use windows::core::BOOL;

    /// Collects each monitor the system enumerates, in its own order.
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        monitors: LPARAM,
    ) -> BOOL {
        // SAFETY: `monitors` is the Vec borrowed below for this call, and this
        // borrow of it lasts the callback only.
        let monitors = unsafe { &mut *(monitors.0 as *mut Vec<u64>) };
        monitors.push(monitor.0 as u64);
        true.into()
    }

    let mut monitors: Vec<u64> = Vec::new();
    // SAFETY: the callback only appends to `monitors`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut monitors as *mut Vec<u64> as isize),
        );
    }
    usize::try_from(place)
        .ok()
        .and_then(|place| monitors.get(place).copied())
        .with_context(|| {
            format!(
                "no display is listed at place {place}, where {} are enumerated",
                monitors.len()
            )
        })
}

/// What a share of `source` is captured as, before its handle is turned into a
/// `wgc::Target`: a window as itself, a display as the monitor the shell's list
/// named, and the primary display when nothing has been chosen.
///
/// It is the whole of the decision `graphics_capture` makes about which of the
/// two constructors it asks for, kept apart from the capture it opens so that
/// the rule can be checked without a screen or a window: a window is captured
/// as that window, and never as the display it sits on.
#[cfg(any(wgc, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardTarget {
    /// A window's own handle, which `CreateForWindow` is given.
    Window(u64),
    /// A display's monitor handle, or [`None`] for the one the desktop starts
    /// on, which `CreateForMonitor` is given as the primary display.
    Display(Option<u64>),
}

/// Which target `source` is captured as: see [`CardTarget`]. A camera and a
/// test pattern are not a screen, and saying so is the reason a share of one
/// stays on the processor.
#[cfg(any(wgc, test))]
fn card_target(source: &Source) -> Result<CardTarget> {
    match source {
        // The window's handle is carried unchanged: it is the one
        // `CreateForWindow` is given, and the display the window is composited
        // on is not what the person chose.
        Source::Window(window) => Ok(CardTarget::Window(*window)),
        Source::Screen => Ok(CardTarget::Display(None)),
        Source::Display(place) => Ok(CardTarget::Display(Some(*place))),
        Source::Camera(_) => bail!("a camera isn't a screen"),
        Source::TestPattern => bail!("a test pattern is drawn, not captured"),
    }
}

/// The Windows Graphics Capture rung: the display's compositor fills the
/// texture, so the pointer is already in it and no pixel is drawn here.
///
/// The stream's own session opens on the capture's device and is let go: what
/// it asks is whether NVENC opens where the capture fills its textures, which
/// is the one thing that decides whether these halves fit together. A session
/// reads a texture only on the device it opened on, so asked before the loop
/// starts, a card that can't answer leaves the share on the processor instead
/// of failing its first frame.
#[cfg(wgc)]
fn graphics_capture(
    source: &Source,
    sink: &Sink,
    (max_width, max_height): (u32, u32),
) -> Result<Card> {
    // Which of the two constructors this goes through, and the handle it names:
    // a window as itself, by the item `CreateForWindow` makes, and a display as
    // the monitor the shell's list named.
    let target = match card_target(source)? {
        CardTarget::Window(window) => {
            wgc::Target::from(wgc::Window::new(window as usize as *mut c_void))
        }
        CardTarget::Display(None) => wgc::Target::from(wgc::Monitor::PRIMARY),
        CardTarget::Display(Some(place)) => {
            wgc::Target::from(wgc::Monitor::new(monitor_of(place)? as usize as *mut c_void))
        }
    };
    let capture = wgc::Capture::open(target)?;
    let (width, height) = capture.content_size();
    if width == 0 || height == 0 {
        bail!("the screen is {width}x{height}, which is nothing to capture");
    }
    let (sent_width, sent_height) = fit(width, height, max_width, max_height);
    d3d11::D3d11Encoder::open_on(
        capture.device(),
        stream_connection::SEND_CODEC,
        sent_width,
        sent_height,
        sink.fps,
        kbps(sent_width, sent_height, sink.fps),
        d3d11::Surfaces::Ring,
    )
    .context("the card won't open an encoder on the device it captures with")?;
    Ok(Card::Graphics { capture, target })
}

/// The Desktop Duplication rung: DXGI hands over the compositor's own surface,
/// and native/dup.cc turns it upright, converts it to BGRA and paints the
/// pointer into it on the card before the loop sees it. No border is drawn and
/// nothing is left on the screen, so this is the rung a Windows whose Graphics
/// Capture can't remove its border tries first, and where a screen reaches a
/// stream on a Windows whose Graphics Capture is missing or won't open.
///
/// The device is the duplication's own, made by the shim on the adapter that
/// drives the display, because a duplication exists only on its own card's
/// device: the encoder then opens its session there rather than on a device of
/// its choosing. That is the same one-device rule as the first rung, and it is
/// why a machine whose NVENC is on another card refuses here too.
#[cfg(dup)]
fn duplication(source: &Source, sink: &Sink, (max_width, max_height): (u32, u32)) -> Result<Card> {
    let target = duplication_target(source)?;
    let capture = dup::Capture::open(target)?;
    let (width, height) = capture.content_size();
    if width == 0 || height == 0 {
        bail!("the screen is {width}x{height}, which is nothing to duplicate");
    }
    let (sent_width, sent_height) = fit(width, height, max_width, max_height);
    d3d11::D3d11Encoder::open_on(
        capture.device(),
        stream_connection::SEND_CODEC,
        sent_width,
        sent_height,
        sink.fps,
        kbps(sent_width, sent_height, sink.fps),
        d3d11::Surfaces::Ring,
    )
    .context(
        "the card won't open an encoder on the device the duplication fills its textures on",
    )?;
    Ok(Card::Duplication { capture, target })
}

/// Which display a duplication captures, following the rule the first rung's
/// `card_target` follows: the primary display when the person hasn't chosen
/// one, and otherwise the one they chose — here as the adapter and output index
/// DXGI knows it by, since a duplication is opened by those and not by a monitor
/// handle.
#[cfg(dup)]
fn duplication_target(source: &Source) -> Result<dup::Target> {
    match source {
        Source::Screen => dup::Target::primary(),
        Source::Display(screen) => target_of_monitor(monitor_of(*screen)?),
        Source::Window(_) => bail!(
            "a window is captured on the processor: what DXGI duplicates here is a whole display"
        ),
        Source::Camera(_) => bail!("a camera isn't a display to duplicate"),
        Source::TestPattern => bail!("a test pattern is drawn, not copied from a display"),
    }
}

/// The adapter and output driving the display `monitor`, as DXGI numbers them.
///
/// The monitor handle is the one the shell's list gives — the same one Windows
/// Graphics Capture takes — and DXGI names an output by its adapter's LUID and
/// an index, so the two are matched by asking every adapter for every output's
/// monitor, which is what native/wgc.cc does with the same handle. It is asked
/// once per share, before the loop, so it costs nothing a frame.
#[cfg(dup)]
fn target_of_monitor(monitor: u64) -> Result<dup::Target> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    // The first rung's `Monitor::new(0)` is the primary display, and the same
    // handle from a stale list means the same thing here rather than a search
    // for an output with no monitor at all.
    if monitor == 0 {
        return dup::Target::primary();
    }
    let handle = monitor as usize as *mut std::ffi::c_void;
    // SAFETY: DXGI is asked for its factory and then for each adapter and output
    // by index, and every one it hands back is released when it goes out of
    // scope.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|status| anyhow!("no DXGI factory: {status}"))?;
    let mut at = 0;
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(at) } {
        at += 1;
        let Ok(described) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        let mut output = 0;
        while let Ok(screen) = unsafe { adapter.EnumOutputs(output) } {
            let Ok(desc) = (unsafe { screen.GetDesc() }) else {
                output += 1;
                continue;
            };
            if desc.Monitor.0 == handle {
                return Ok(dup::Target::new(
                    described.AdapterLuid.HighPart as u32,
                    described.AdapterLuid.LowPart,
                    output,
                ));
            }
            output += 1;
        }
    }
    bail!("no display at monitor handle 0x{monitor:x}, which DXGI enumerates")
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
///
/// The order the two ways are tried in is [`capture::order`]'s, which is the
/// setting's choice or, under Auto, whichever way can be borderless on this
/// build of Windows: Desktop Duplication on a Windows whose Graphics Capture
/// has no interface to take its border off, Graphics Capture where it does. A
/// rung that refuses is named and the other opens; under Auto a Graphics
/// Capture that opens with its border still on is refused the same way, once it
/// has been let go in full, which is the case that used to leave a Windows 10
/// share drawing a yellow outline beside a duplication that would have been
/// clean. Either rung hands the loop the same borrowed BGRA texture on the
/// device its capture made, so everything after this decision is the same.
#[cfg(any(wgc, dup))]
fn card_capture(source: &Source, sink: &Sink, bounds: (u32, u32)) -> Result<Chosen> {
    if !sink.hardware {
        bail!("the stream is set to encode on the processor");
    }
    let preference = capture::preference();
    let platform = capture::platform();
    let available = capture::Available {
        graphics: cfg!(wgc),
        duplication: cfg!(dup),
    };
    let rungs = capture::order(preference, platform, available);
    let mut refused = String::new();
    for (at, &rung) in rungs.iter().enumerate() {
        match open_rung(rung, source, sink, bounds) {
            Ok(card) => {
                let next = rungs.get(at + 1).copied();
                if capture::keeps(preference, rung, card.borderless(), next) {
                    return Ok(Chosen {
                        card,
                        why: chosen_why(preference, platform, available, rung, &refused),
                    });
                }
                // Auto asked for a capture that can be borderless and this one
                // opened with its border still on. It is let go in full before
                // the next rung opens, because a share holds one capture at a
                // time and a duplication can't take the display while a
                // Graphics Capture still has it.
                let why = card.border();
                drop(card);
                refuse(&mut refused, rung, &why);
            }
            Err(why) => refuse(&mut refused, rung, &format!("{why:#}")),
        }
    }
    if refused.is_empty() {
        bail!("this build of Yapfast has no way to capture a screen on the card");
    }
    bail!("{refused}")
}

/// Shares a screen from the card: each frame either capture fills a texture
/// with goes to the encoder as that texture — scaled on the card to the
/// stream's size where the screen is larger — and the bitstream out of the
/// driver is the only time a frame's bytes touch the processor.
///
/// Nothing here keeps a frame. The texture is the capture's own, borrowed until
/// the next [`Card::frame`]; the encoder's buffers come back to it as frames go
/// out; and the one picture held in memory is the preview's tile, scaled on the
/// card and read back at no more than [`PREVIEW_WIDTH`] by [`PREVIEW_HEIGHT`].
#[cfg(any(wgc, dup))]
fn capture_textures(
    chosen: Chosen,
    mut sink: Sink,
    (max_width, max_height): (u32, u32),
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<(Sink, After)> {
    let Chosen {
        card: mut capture,
        why,
    } = chosen;
    let (screen_width, screen_height) = capture.content_size();
    let (sent_width, sent_height) = fit(screen_width, screen_height, max_width, max_height);
    let described = capture.describe();
    tracing::info!(
        rung = capture.rung().name(),
        why = why.as_str(),
        capture = described.as_str(),
        screen = format!("{screen_width}x{screen_height}"),
        stream = format!("{sent_width}x{sent_height}"),
        "sharing from the card"
    );
    let fps = sink.fps;
    sink.footage = Footage::OnTheCard;
    sink.capture = format!(
        "{} · {screen_width}×{screen_height} screen",
        capture.rung().name()
    );
    let started = Instant::now();
    // Only the pace of the loop matters here: a screen that drew nothing is a
    // frame skipped, not one captured twice.
    let mut slot = 0u64;
    let mut reported = None;
    let mut sampler = pressure::Sampler::new();
    // The session a preview's tile is read back through, opened on the capture's
    // device when something first wants a preview and let go when nothing does.
    // A session of its own, because the encoder scales into one texture it keeps
    // for each size, and a tile sharing the stream's session would remake that
    // texture every frame, alternating the stream's size with the tile's. It
    // keeps one of the capture's surfaces rather than a ring: it reads a frame
    // in ten, so there is no rotation to keep up with, and every surface it held
    // would be a full-size one the capture couldn't fill.
    let mut tile: Option<d3d11::D3d11Encoder> = None;
    // The tile's pixels, in the card's order: BGRA, one row after another.
    let mut pixels: Vec<u8> = vec![];
    let mut read: Option<Instant> = None;
    // The picture the UI's window shows where the card drew it, made with the
    // tile's own session: the same scale that fills the tile fills the texture
    // the UI opens, so one session and one destination serve both, and a share
    // that can't be read back can still be shown. `showable` is whether the UI
    // says it can show one at all, which a refusal settles for the stream.
    let mut shared: Option<d3d11::Shared> = None;
    let mut showable = true;
    // How many times the session and the capture have been put back together
    // since a frame last went out.
    let mut restarts = 0u32;
    // Whether a picture has been made since the source or the capture changed,
    // and whether a preview became wanted since one was last made: both are
    // demand a still desktop answers, and neither outlasts the answer.
    let mut saw_picture = false;
    let mut preview_pending = false;
    let mut was_wanted = false;
    while next_frame(started, &mut slot, fps, stop).is_some() {
        // One look per stream slot, paced by `next_frame` on both rungs: a
        // duplication asked with no wait answers at once whether the desktop
        // changed, and a still one makes no frame here and no spin either. A
        // capture that stops — a duplication loses its desktop on a secure
        // screen or a display change — is put back together below with the
        // encoder, exactly as a device the card took away is.
        let mut taken = Ok(());
        let arrival = match capture.frame(Duration::ZERO) {
            Ok(arrival) => arrival,
            Err(error) => {
                taken = Err(error);
                Arrival::Still
            }
        };
        // When the capture answered with a picture, which is what this side's
        // queue delay is measured from.
        if arrival == Arrival::Frame {
            sink.diagnosis.captured_at = Some(Instant::now());
        }
        // Everything the sink has to say about this pass: the controls, which is
        // also where its reports go, whether a preview is wanted, the UI's
        // process, and the frame itself when one arrived. None of it is done
        // under a lock, because there is none: this sink is the loop's own — the
        // other capture path shares its sink with a capturer's callback thread
        // and locks it for that, and a share of the card has no second thread to
        // share with. Sole ownership is what keeps a frame's texture from being
        // read by two threads at once; the compiler holds it to that, and the
        // GPU work below — scale, encode, readback, the picture the UI shows —
        // runs with nothing held at all.
        let switched = sink.check(sampler.read(Instant::now()), report)?;
        let wanted = sink.preview_wanted();
        // A preview that has just been asked for is owed a picture now, even if
        // the screen doesn't change: waiting for the pointer to move is what a
        // tile that came back would otherwise do.
        if wanted && !was_wanted {
            preview_pending = true;
        }
        was_wanted = wanted;
        // The UI's process, which a picture drawn here is shared into: zero
        // until it opens these frames, and a process that has gone is one
        // with nothing to show anything in.
        let ui = sink.writer.ui_process();
        // Whether the capture's own texture still holds a picture of what is
        // shared: a frame put one there, and a resize, a capture put back
        // together or another source ended that. Nothing is made from a
        // picture the texture doesn't hold, so a missing one waits for the
        // capture rather than showing the source before it.
        let mut size = None;
        match arrival {
            Arrival::Frame => {
                saw_picture = true;
                let content = capture.content_size();
                let sent = fit(content.0, content.1, max_width, max_height);
                taken = sink.textured(
                    // SAFETY: the capture's own surface, on the device the capture
                    // holds, alive for as long as the capture is and not filled
                    // again until this frame has been encoded.
                    unsafe { d3d11::Texture::new(capture.texture()) },
                    content,
                    sent,
                    capture.device(),
                );
                size = Some(sent);
            }
            // The screen changed size and the capture's texture with it, so
            // the next frame is at the new size: `ready` opens an encoder
            // for it and tells the sender, and a session's first frame is
            // whole.
            Arrival::Resized => {
                // What the texture held was drawn at the size before it.
                saw_picture = false;
                tracing::info!("the screen changed size");
            }
            // Nothing has been drawn and nothing has moved, which is the
            // desktop's own rate: the next pass is the next slot. It is not a
            // reason for a stream to have nothing to say, though — a viewer
            // joining, a decoder that lost its place, a preview coming back are
            // all served from the picture the capture last made, which is still
            // in the capture's own texture, and only while one of those asks for
            // it. A desktop that is neither moving nor wanted encodes nothing.
            Arrival::Still => {
                // A still is answered from the capture's own texture only when
                // the capture answered with nothing new: a call that failed
                // hands back no texture, and its error is the one to keep.
                if taken.is_ok() && saw_picture && sink.picture_needed() {
                    let content = capture.content_size();
                    let sent = fit(content.0, content.1, max_width, max_height);
                    taken = sink.textured(
                        // SAFETY: as above: the capture's own surface, on the
                        // capture's device, alive for as long as the capture is.
                        unsafe { d3d11::Texture::new(capture.texture()) },
                        content,
                        sent,
                        capture.device(),
                    );
                    size = Some(sent);
                }
            }
            // What was being captured ended. This capture hands out no more
            // frames, so the share ends rather than waits on one that won't
            // come.
            Arrival::Closed => bail!("the capture of the screen ended"),
        }
        if let Some((width, height)) = size
            && reported != size
        {
            reported = size;
            report(Report::Sharing { width, height });
        }
        if let Some(source) = switched {
            // Another screen: another capture, and the session that encodes it
            // and the tile's own were both built on the device the old capture
            // made, so all three are let go before the new one opens — the
            // capture first, because DXGI gives an output to one duplicator at
            // a time, and the encoder before it, because a session reads on
            // the device its capture made. What can't be opened is said, and
            // the share carries on from the processor rather than ending: a
            // stream that stops is worse than one on the processor.
            if on_the_card(&source) {
                tile = None;
                shared = None;
                read = None;
                sink.let_go();
                drop(capture);
                // What the old capture's texture held went with it.
                saw_picture = false;
                preview_pending = false;
                match card_capture(&source, &sink, (max_width, max_height)) {
                    Ok(next) => {
                        let (width, height) = next.card.content_size();
                        let (sent_width, sent_height) = fit(width, height, max_width, max_height);
                        tracing::info!(
                            rung = next.card.rung().name(),
                            capture = next.card.describe().as_str(),
                            screen = format!("{width}x{height}"),
                            stream = format!("{sent_width}x{sent_height}"),
                            "sharing another screen from the card"
                        );
                        sink.capture =
                            format!("{} · {width}×{height} screen", next.card.rung().name());
                        capture = next.card;
                        // The next frame says what is shared now, even at the
                        // size the screen before it was.
                        reported = None;
                        restarts = 0;
                    }
                    Err(why) => {
                        tracing::info!(
                            why = format!("{why:#}"),
                            "switching screens: sharing from the processor"
                        );
                        // The stream goes on from the processor, and the
                        // encoder that read the card's textures goes with the
                        // device the capture made: see `fall_back_to_the_processor`.
                        sink.fall_back_to_the_processor();
                        return Ok((sink, After::Processor(source)));
                    }
                }
            } else {
                // A window or a camera: the card's paths take a display, and
                // the processor's capturer shares one, with the stream going
                // on from here.
                sink.fall_back_to_the_processor();
                return Ok((sink, After::Processor(source)));
            }
        }
        match taken {
            // A session the card took away, which a driver reset is the usual
            // cause of. Both halves are built again, since the device that
            // filled the textures is the one that went, and a card that answers
            // every frame with the same failure is one to stop sharing on.
            Err(error) => {
                restarts = restarts.saturating_add(1);
                if restarts > CARD_RESTARTS {
                    return Err(error.context("the card's session stopped"));
                }
                tracing::warn!(
                    error = format!("{error:#}"),
                    "the card stopped; opening the capture and the encoder again"
                );
                // Nothing that reads the old device may outlive it: the tile's
                // session goes now, and the stream's before the capture that
                // made the device is let go of.
                tile = None;
                shared = None;
                // A session the card took away is one whose picture went with
                // it.
                saw_picture = false;
                preview_pending = false;
                sink.let_go();
                capture = capture.reopen()?;
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
                let preview = preview_size(content.0, content.1, sink.constrained());
                match d3d11::D3d11Encoder::open_on(
                    capture.device(),
                    stream_connection::SEND_CODEC,
                    preview.0,
                    preview.1,
                    fps,
                    kbps(preview.0, preview.1, fps),
                    d3d11::Surfaces::One,
                ) {
                    Ok(opened) => {
                        // The picture the UI's window shows where it lies is made
                        // with this session and at the tile's own size: the scale
                        // that fills the tile fills it, so the stream holds a
                        // texture rather than a frame either way.
                        if showable && ui != 0 {
                            match d3d11::Shared::new(&opened, preview.0, preview.1, ui) {
                                Ok(made) => shared = Some(made),
                                Err(error) => {
                                    tracing::info!(
                                        error = format!("{error:#}"),
                                        "showing the pixels of a stream: the card won't share a picture"
                                    );
                                    showable = false;
                                    sink.diagnosis.fallback =
                                        Some("the card won't share a picture".to_owned());
                                }
                            }
                        }
                        tile = Some(opened);
                        read = Some(Instant::now());
                        // A session that has just opened has no picture of its
                        // own yet, so the one the capture made is what it is
                        // owed.
                        preview_pending = true;
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
                shared = None;
                read = None;
            }
            _ => {}
        }
        // A preview read back into memory is what a window that can't show the
        // card's own picture shows, and what Discord's still is made from.
        // While the picture itself is what the window shows, neither is
        // written: one frame is published once, rather than the tile
        // alternating between a texture and a smaller copy of it ten times a
        // second. A still desktop has a picture to read back too, whether a
        // preview has just come back, a session has just opened for one, or
        // Discord's still is due: those are what ask for it, and once each.
        let read_back = shared.is_none() || sink.still_wanted();
        let mut previewed = false;
        if let Some(tile) = &mut tile
            && saw_picture
            && (arrival == Arrival::Frame || preview_pending || sink.still_wanted())
            && read_back
            && read.is_none_or(|last| last.elapsed() >= PREVIEW_EVERY)
        {
            read = Some(Instant::now());
            let taken = sink.preview_from_card(
                tile,
                // SAFETY: the capture's own surface, on the capture's device,
                // alive for as long as the capture is: the readback is done
                // before this frame returns.
                unsafe { d3d11::Texture::new(capture.texture()) },
                capture.content_size(),
                &mut pixels,
            );
            match taken {
                Ok(()) => previewed = true,
                Err(error) => {
                    tracing::warn!(error = format!("{error:#}"), "sharing without a preview")
                }
            }
        }
        // The picture the UI's window shows where the card drew it: the tile's
        // own size, scaled on the card into a texture both processes hold, so a
        // window drawn on the GPU shows it without a copy of anything.
        //
        // The frames file's slot decides whether this frame has a picture at
        // all: a slot is only drawn into once the window showing it has moved
        // past what was there, so a picture a person is looking at is never
        // written over. A frame whose slot is still being shown keeps its
        // picture a frame longer — the stream itself is unaffected, since it
        // encodes the capture's own texture. A window that stops saying it is
        // done with pictures is one the writer stops asking: the tile's pixels
        // go out from the frame after that, which is a preview going on rather
        // than a tile frozen on the last picture it took.
        let mut withdrawn = false;
        if let (Some(tile), Some(shared)) = (tile.as_mut(), shared.as_mut())
            && saw_picture
            && (arrival == Arrival::Frame || preview_pending)
        {
            match sink.writer.claim() {
                Some(slot) => {
                    let taken = sink.share_from_card(
                        slot,
                        tile,
                        shared,
                        // SAFETY: the capture's own surface, on the capture's
                        // device, alive for as long as the capture is: the copy
                        // into the shared picture is queued before this frame
                        // returns.
                        unsafe { d3d11::Texture::new(capture.texture()) },
                    );
                    match taken {
                        Ok(true) => previewed = true,
                        // The UI looked and can't show a picture from this card,
                        // or its window draws on another one: the tile's pixels
                        // are what it shows, and the share it never opened is
                        // let go of.
                        Ok(false) => withdrawn = true,
                        Err(error) => {
                            tracing::warn!(
                                error = format!("{error:#}"),
                                "sharing without the picture drawn on the card"
                            );
                            withdrawn = true;
                        }
                    }
                }
                // The slot the writer would draw in is one nothing has
                // finished with for a second: the share stops drawing there
                // and the tile's pixels go out instead.
                None if sink.writer.given_up() => withdrawn = true,
                // A slot the window is still showing, which the frame after
                // this one asks for again.
                None => {}
            }
        }
        if withdrawn {
            showable = false;
            shared = None;
            // The tile is a copy of the picture; the stream's own path is
            // untouched, so the report's fallback stays clear for it: what the
            // window shows is the presentation's to say.
            sink.diagnosis.presentation = Some(Presentation::Copy);
        }
        // What the window does with the pictures is the one thing only the two
        // processes together know: a picture the window took is shown from the
        // card's own memory, and anything else is a copy of the pixels.
        if previewed && let Some(shared) = shared.as_ref() {
            sink.diagnosis.presentation = Some(Presentation::Shared);
            sink.diagnosis.gpu_luid = Some(shared.luid());
        }
        // The preview that was owed has been made, whichever way it went out.
        if previewed {
            preview_pending = false;
        }
    }
    Ok((sink, After::Ended))
}

/// One way a Linux share captures a screen as the DMA-BUFs the compositor drew,
/// as the loop above it asks one: what the next picture is, and what the capture
/// is.
///
/// The two captures differ in where their pictures come from and not in what they
/// hand over — a descriptor, its layout, and the size it is — so the loop, the
/// encoder and the window are the same either way. A capture is a boxed trait
/// object rather than an enum because a test has no portal and no X server to
/// open, and the loop against a capture is the half worth checking.
#[cfg(target_os = "linux")]
trait Pictures {
    /// Waits up to `wait` for the picture to change, and answers with what
    /// arrived: the capture's own wait is the share's pace, so a still desktop
    /// sleeps here rather than spinning beside the loop.
    fn frame(&mut self, wait: Duration) -> Result<Found>;

    /// What this capture is, in one line, for the log that says which ran.
    fn describe(&self) -> String;

    /// Why the share ended, in the capture's own words: empty while it still
    /// runs, and the sentence [`Found::Closed`] is reported with.
    fn ending(&self) -> String;
}

/// What asking a Linux capture for its next picture found, in the terms the loop
/// works in: both captures answer these four, whatever they call them.
#[cfg(target_os = "linux")]
enum Found {
    /// A picture, which this share keeps: see [`Shot`].
    Picture(Shot),
    /// Nothing has been drawn since the last picture, which is not a failure and
    /// makes no frame of its own.
    Still,
    /// What is captured changed size: whatever encodes the frames was built for
    /// the old one, and the next picture is at the new.
    Resized,
    /// The desktop stopped sharing: the person ended it, the window closed, or
    /// the session went away. The capture hands out nothing more.
    Closed,
}

/// What became of a picture the desktop captured as a DMA-BUF.
#[cfg(target_os = "linux")]
#[derive(Debug, PartialEq, Eq)]
enum Taken {
    /// The stream has it.
    Sent,
    /// It didn't reach the encoder this pass — no stream yet, a full send queue,
    /// a codec Discord withdrew — and the next picture may.
    Held,
    /// It can't reach the encoder at all, and it can't be read back on the
    /// processor either: the reason, in the words of the read that failed. A
    /// share of these pictures goes on from the capturer that has pixels.
    Processor(String),
}

/// A picture the desktop captured, as the loop hands it on: the compositor's own
/// DMA-BUF, in a descriptor this share keeps rather than the capture that handed
/// it over.
///
/// No copy of the picture is made here. The layout is kept beside the descriptor
/// so that one can be built from it whenever it is wanted — once for the window,
/// once for the encoder, and once more for Discord's still — and the descriptor
/// is the picture itself, which the window imports and draws where it lies.
///
/// The fd is this share's own, duplicated from the capture's, because a picture
/// outlives the call that gave it: a desktop that hasn't drawn anything since is
/// still a stream a viewer joining can be shown, and the picture it is shown is
/// the one the capture handed over before.
#[cfg(target_os = "linux")]
struct Shot {
    width: u32,
    height: u32,
    /// The one plane and the descriptor it is in: a compositor's picture is one
    /// buffer, in one of the two four-byte orders a desktop is drawn in.
    planes: [dmabuf::Plane; 1],
    fds: [std::os::fd::RawFd; 1],
    /// The fd `fds` names, closed when this share lets go of the picture.
    memory: std::os::fd::OwnedFd,
    /// The DRM modifier the driver laid the memory out with, when the capture
    /// could say: `None` from a DRI3 server older than modifiers telling anyone,
    /// which is read as the linear layout such a server draws under.
    modifier: Option<u64>,
    order: Order,
}

#[cfg(target_os = "linux")]
impl Shot {
    /// The picture as the window and the encoder are handed it.
    fn descriptor(&self) -> dmabuf::Descriptor<'_> {
        dmabuf::Descriptor {
            width: self.width,
            height: self.height,
            fourcc: self.order.fourcc(),
            modifier: self.modifier.unwrap_or(0),
            planes: &self.planes,
            colour: dmabuf::Colour::SDR,
            fds: &self.fds,
        }
    }

    /// The picture's memory, mapped for reading.
    fn read(&self) -> Result<linear::Readback> {
        linear::read(
            self.memory.as_fd(),
            self.width,
            self.height,
            self.planes[0].offset,
            self.planes[0].stride,
            self.modifier,
        )
    }

    /// The same picture as the encoder reads it when it reads memory, at the size
    /// the stream sends, `sent`: converted into the buffer kept for it, and scaled
    /// to `sent` where the screen is larger than the stream.
    ///
    /// This is the copy [`Sink::dmabuf`] makes of a frame while the encoder can't
    /// take a descriptor, and it is made of a linear picture only: a tiled one is
    /// refused by name rather than read as noise.
    fn converted<'a>(&self, scratch: &'a mut Scratch, sent: (u32, u32)) -> Result<&'a I420Buffer> {
        let read = self.read()?;
        let full = buffer(&mut scratch.full, self.width, self.height);
        convert_ordered(full, read.rows(), read.stride(), self.order);
        if (self.width, self.height) == sent {
            return Ok(full);
        }
        let scaled = buffer(&mut scratch.shared, sent.0, sent.1);
        scale_into(&planes(full), scaled, sent.0, sent.1);
        Ok(scaled)
    }

    /// The picture as the still is made from, in the buffer the stream's scale
    /// left: Discord's thumbnail is the whole picture scaled down to fit its own
    /// box, so this is the conversion without the scale to the stream's size.
    fn still(&self, scratch: &mut Scratch) -> Result<()> {
        let read = self.read()?;
        let full = buffer(&mut scratch.full, self.width, self.height);
        convert_ordered(full, read.rows(), read.stride(), self.order);
        Ok(())
    }

    /// The shot a capture hands over, with the fd duplicated out of the capture
    /// that owns it.
    fn held(
        width: u32,
        height: u32,
        offset: u32,
        stride: u32,
        modifier: Option<u64>,
        order: Order,
        memory: BorrowedFd<'_>,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            bail!("the desktop shared a {width}x{height} picture, which is nothing to send");
        }
        let memory = memory
            .try_clone_to_owned()
            .context("the desktop's picture couldn't be kept past the frame that brought it")?;
        Ok(Self {
            width,
            height,
            planes: [dmabuf::Plane { offset, stride }],
            fds: [memory.as_raw_fd()],
            memory,
            modifier,
            order,
        })
    }
}

/// The screen sharing portal's capture, as the loop above it asks it.
#[cfg(screencast)]
struct Portal(capture::pipewire::Capture);

#[cfg(screencast)]
impl Pictures for Portal {
    fn frame(&mut self, wait: Duration) -> Result<Found> {
        use capture::pipewire::{Arrival, Format};
        Ok(match self.0.frame(wait)? {
            Arrival::Frame(picture) => {
                let order = match picture.format() {
                    // A compositor draws a desktop into one of these two, and the
                    // other two are the same bytes with a fourth nothing reads.
                    Format::Bgra | Format::Bgrx => Order::Bgra,
                    Format::Rgba | Format::Rgbx => Order::Rgba,
                    other => bail!(
                        "the desktop shares its screen as {}, which is no picture a stream can read",
                        other.name()
                    ),
                };
                let (width, height) = picture.size();
                Found::Picture(Shot::held(
                    width,
                    height,
                    picture.offset(),
                    picture.stride(),
                    Some(picture.modifier()),
                    order,
                    picture.fd(),
                )?)
            }
            Arrival::Still => Found::Still,
            Arrival::Resized => Found::Resized,
            Arrival::Closed => Found::Closed,
        })
    }

    fn describe(&self) -> String {
        self.0.describe()
    }

    fn ending(&self) -> String {
        self.0.ending()
    }
}

/// The X server's capture, as the loop above it asks it.
#[cfg(target_os = "linux")]
struct Server(capture::x11::Capture);

#[cfg(target_os = "linux")]
impl Pictures for Server {
    fn frame(&mut self, wait: Duration) -> Result<Found> {
        use capture::x11::Arrival;
        Ok(match self.0.frame(wait)? {
            Arrival::Frame(picture) => {
                let buffer = picture.buffer();
                // A monitor is the screen's picture cut to its part of it, so the
                // picture starts later in the memory than the buffer does.
                let region = buffer.region;
                let stride = buffer.stride;
                Found::Picture(Shot::held(
                    region.width,
                    region.height,
                    buffer.offset + region.y * stride + region.x * Order::Bgra.bytes(),
                    stride,
                    buffer.modifier,
                    // The one depth and bpp a capture here hands on is 24 and 32,
                    // which is B, G, R and a byte nothing reads: see `x11`.
                    Order::Bgra,
                    buffer.fd(),
                )?)
            }
            Arrival::Still => Found::Still,
            Arrival::Resized => Found::Resized,
            Arrival::Closed => Found::Closed,
        })
    }

    fn describe(&self) -> String {
        self.0.describe()
    }

    fn ending(&self) -> String {
        self.0.ending().to_owned()
    }
}

/// A capture that opened, and the rung it is: the sentence saying why it is the
/// one sharing, and the name it goes in the share's report under.
#[cfg(target_os = "linux")]
struct Opened {
    capture: Box<dyn Pictures>,
    rung: capture::LinuxRung,
    why: String,
}

/// Whether a Linux capture can take `source` at all: the portal and the X server
/// both take a window, a display or the screen, where a camera and a test
/// pattern are the processor's or nobody's.
#[cfg(any(target_os = "linux", test))]
fn dmabuf_takes(source: &Source) -> bool {
    match source {
        Source::Screen | Source::Display(_) => true,
        // Both rungs capture a window as that window: the portal asks its dialog
        // for windows, and the X server redirects the window's own storage, never
        // the desktop behind it.
        Source::Window(_) => true,
        Source::Camera(_) | Source::TestPattern => false,
    }
}

/// What a Linux share can capture with, as the rungs it is about to try.
#[cfg(target_os = "linux")]
fn available() -> capture::Linux {
    capture::Linux {
        // A build without the `screencast` feature carries no capture of the
        // portal's stream at all, and asking a bus for a portal this build can't
        // read is worse than not asking.
        #[cfg(screencast)]
        portal: capture::pipewire::answers(),
        #[cfg(not(screencast))]
        portal: false,
        // Whether an X server is named at all. Whether it exports a redirected
        // pixmap through DRI3 is the opening capture's answer, and a server that
        // doesn't is a rung that refused.
        x11: std::env::var_os("DISPLAY").is_some(),
    }
}

/// Whether the window shows a picture the worker publishes as a DMA-BUF, which is
/// the whole point of capturing this way: [`Writer::dmabuf_showable`] answers
/// with the device the window imports one on, and `None` while it hasn't looked,
/// while it refused them, or while it has no importer at all.
///
/// Asked once per share, before a capture opens: a window's answer is settled for
/// the stream. A picture it can't show is a share that stays on the processor,
/// where its frames have always been pixels it can be given.
#[cfg(target_os = "linux")]
fn showable(writer: &mut Writer) -> bool {
    writer.dmabuf_showable().is_some()
}

/// Whether this build has an encoder that reads a descriptor where it lies: the
/// VA-API import, or the CUDA rung NVIDIA's cards need. The question is the
/// encoder's, not the window's — a share whose tile is drawn on the processor
/// still sends from the card, with its preview read back instead of shown.
#[cfg(target_os = "linux")]
fn encodes_descriptors() -> bool {
    #[cfg(any(dmabufenc, nvcuda))]
    {
        true
    }
    #[cfg(not(any(dmabufenc, nvcuda)))]
    {
        false
    }
}

/// Opens `rung`'s capture of `source`, whose pictures are the DMA-BUFs the
/// compositor drew.
#[cfg(target_os = "linux")]
fn open_rung(rung: capture::LinuxRung, source: &Source) -> Result<Box<dyn Pictures>> {
    match rung {
        #[cfg(screencast)]
        capture::LinuxRung::Portal => {
            // Which kind of thing to share is the one thing the person's pick
            // says to the dialog: a window is never asked for as the screen it
            // sits on, and a display is one monitor rather than every screen.
            // The id itself is Yapfast's own list's, which the portal has no way
            // of being told, so the dialog picks inside that kind.
            let asked = match source {
                Source::Window(_) => capture::pipewire::Source::Windows,
                Source::Display(_) => capture::pipewire::Source::Monitors,
                _ => capture::pipewire::Source::Both,
            };
            Ok(Box::new(Portal(capture::pipewire::Capture::open(asked)?)))
        }
        #[cfg(not(screencast))]
        capture::LinuxRung::Portal => bail!(
            "this build has no capture of the portal's stream: the `screencast` feature and \
             PipeWire's headers are what build one"
        ),
        capture::LinuxRung::X11 => Ok(Box::new(Server(capture::x11::Capture::open(x11_target(
            source,
        )?)?))),
    }
}

/// Which of the X server's drawables `source` names, following the rule the rest
/// of the share follows: a window is that window, a display is one monitor, and
/// the screen is the desktop.
///
/// The id a [`Source::Display`] carries is the one Yapfast's menu listed the
/// screen by, which on X11 is libwebrtc's own: the root window's id for the
/// screen, or a screen's place in its list. Both are answered here — the root id
/// as the screen, and a place as the monitor the list would show there — and an
/// id that is neither is refused rather than captured as something else.
#[cfg(target_os = "linux")]
fn x11_target(source: &Source) -> Result<capture::x11::Target> {
    use capture::x11::Target;
    match source {
        Source::Screen => Ok(Target::Screen),
        Source::Window(window) => Ok(Target::Window(
            u32::try_from(*window)
                .with_context(|| format!("0x{window:x} is no X window: an id there is 32 bits"))?,
        )),
        Source::Display(id) => {
            let root = capture::x11::root_window()?;
            let monitors: Vec<capture::x11::Display> = capture::x11::listed()?
                .into_iter()
                .filter_map(|listed| match listed.target {
                    Target::Display(display) => Some(display),
                    Target::Window(_) | Target::Screen => None,
                })
                .collect();
            screen_target(*id, root, &monitors)
        }
        Source::Camera(_) => bail!("a camera isn't a screen"),
        Source::TestPattern => bail!("a test pattern is drawn, not captured"),
    }
}

/// The target a listed screen's id names, given the server's root window and the
/// monitors it has: the whole screen for the root's own id, the monitor at that
/// place for a place, and nothing for anything else.
///
/// This is the whole of the decision [`x11_target`] makes, kept apart from the
/// server it asks so that it can be checked without one.
#[cfg(any(target_os = "linux", test))]
fn screen_target(
    id: u64,
    root: u32,
    monitors: &[capture::x11::Display],
) -> Result<capture::x11::Target> {
    if id == u64::from(root) {
        return Ok(capture::x11::Target::Screen);
    }
    usize::try_from(id)
        .ok()
        .and_then(|place| monitors.get(place).cloned())
        .map(capture::x11::Target::Display)
        .with_context(|| {
            format!(
                "the X server has no screen {id:#x} and no monitor at place {id}, where it has {}",
                monitors.len()
            )
        })
}

/// A capture of `source` whose pictures are the compositor's DMA-BUFs, or the
/// reason this share stays on the processor.
///
/// The rungs are tried in [`capture::linux_order`]'s order — the portal where it
/// answers, and the X server after it — and each that refuses is named with what
/// it said. Two things are asked before any of them opens, because either leaves
/// the share with nothing to do: whether the window can show a picture the worker
/// published as a DMA-BUF at all, and whether this source is one the card's paths
/// take.
#[cfg(target_os = "linux")]
fn dmabuf_capture(source: &Source, sink: &mut Sink) -> Result<Opened> {
    if !dmabuf_takes(source) {
        bail!("this source has no picture the card made");
    }
    if !showable(&mut sink.writer) && !encodes_descriptors() {
        bail!("nothing here can read a picture the card made");
    }
    let session = capture::Session::new(portal_picks());
    let available = available();
    let mut refused = String::new();
    for rung in capture::linux_order(session, available) {
        match open_rung(rung, source) {
            Ok(capture) => {
                let why = capture::linux_why(session, available, rung, &refused);
                return Ok(Opened { capture, rung, why });
            }
            Err(why) => capture::refused(&mut refused, rung, &format!("{why:#}")),
        }
    }
    if refused.is_empty() {
        bail!("this session has no way to capture a screen on the card");
    }
    bail!("{refused}")
}

/// Whether a pass of a share whose pictures are DMA-BUFs has a picture to send:
/// the one that has just arrived, or — for a still desktop — the one the capture
/// last handed over, which a viewer joining, a decoder that lost its place or a
/// codec Discord chose can be answered from.
///
/// A still picture is sent only while the stream asks for one. A desktop that
/// neither moves nor is wanted sends nothing at all, which is what keeps a share
/// of a screen nobody is touching from encoding the same picture for ever.
#[cfg(any(target_os = "linux", test))]
fn sending(fresh: bool, saw_picture: bool, needed: bool) -> bool {
    fresh || (saw_picture && needed)
}

/// Whether a pass publishes a picture for the window, which is what makes it the
/// tile the person sees: only while something shows one, and only while the
/// window hasn't been given it.
///
/// A picture published is the picture the window keeps showing until another
/// slot replaces it, and publishing the same one again costs a message, a
/// descriptor and an import for a picture already on the screen. So one is owed
/// when a preview is first wanted and when the share has just moved to another
/// picture; a still desktop owes none.
#[cfg(any(target_os = "linux", test))]
fn publishing(wanted: bool, owed: bool) -> bool {
    wanted && owed
}

/// Shares a screen or a window the desktop captured as the DMA-BUFs its
/// compositor drew: each picture goes to the window as the descriptor it already
/// is, which the window imports and draws where it lies, and to the encoder as
/// the same descriptor — or as the same picture converted on the processor, when
/// the encoder reads memory and no import of one is built; see [`Sink::dmabuf`].
///
/// Nothing here keeps a picture. The descriptor is the capture's own, borrowed
/// until its next frame and kept only as the one duplicate of an fd that a still
/// desktop is answered from; the one picture in this process's memory is Discord's
/// still, read back when it is due and scaled down to a thumbnail.
///
/// A capture that ends ends the share — the person stopped it, the window closed
/// — and one that can't give a picture the encoder can take is a share that
/// carries on from the processor's capturer rather than ending: a stream that
/// stops is worse than one that costs a copy.
#[cfg(target_os = "linux")]
fn capture_dmabufs(
    opened: Opened,
    mut source: Source,
    mut sink: Sink,
    (max_width, max_height): (u32, u32),
    stop: &AtomicBool,
    report: &mut dyn FnMut(Report),
) -> Result<(Sink, After)> {
    let Opened {
        mut capture,
        mut rung,
        why,
    } = opened;
    tracing::info!(
        rung = rung.name(),
        why = why.as_str(),
        capture = capture.describe().as_str(),
        "sharing from the card"
    );
    sink.footage = Footage::Dmabuf;
    sink.diagnosis.presentation = Some(Presentation::Shared);
    let started = Instant::now();
    // Only the pace of the loop matters here: the capture is asked for a picture
    // until the next slot is due, and its own wait is where a still desktop spends
    // the time. Nothing here sleeps beside it and nothing spins. A capture answers
    // as soon as the desktop draws, which a busy screen does at its refresh rate,
    // so a picture that comes before its slot is kept and the next one asked for:
    // the newest is sent when the slot comes. Sending each as it came encoded a
    // 30 fps stream at the compositor's rate, filled the send queue, and had the
    // encoder overshoot a rate it shares out over the stream's frame rate.
    let mut slot = 0u64;
    // Whether the picture in `last` arrived since one was last sent.
    let mut fresh = false;
    // What the stream is sent at, and what the window has been told: both change
    // when the shared thing does.
    let mut size: Option<(u32, u32)> = None;
    let mut sampler = pressure::Sampler::new();
    // The picture the capture last handed over, which a still desktop is answered
    // from when a viewer asks for one, and which is dropped when the size changes
    // under it.
    let mut last: Option<Shot> = None;
    // Whether a picture has been made since the source or the capture changed,
    // and whether a preview became wanted since one was last published: both are
    // demand a still desktop answers, and neither outlasts the answer.
    let mut saw_picture = false;
    let mut preview_pending = false;
    let mut was_wanted = false;
    // Whether the tile's preview is read back, because the window draws on the
    // processor and can't sample the picture the card made: decided once, on the
    // first picture the window refused.
    // Whether a read-back preview failed already, which is said once.
    let mut preview_refused = false;
    let mut readback = false;
    // The buffers a picture is converted in, when the encoder reads memory: one
    // for the picture and one for the stream's size, made again only when a
    // picture comes at another size.
    let mut scratch = Scratch::default();
    // When the capture's account of itself was last logged while it had still
    // produced nothing: a stream that negotiates nothing waits here, and the line
    // the share opens with is a snapshot taken before that wait began.
    let mut waited: Option<Instant> = None;
    // When the desktop last drew something, and whether the share has said that
    // it stopped drawing: a screen nothing is drawing to sends no pictures, so a
    // stream that goes quiet and a preview that stops following it are the
    // desktop's doing rather than the share's, and worth one line each way.
    let mut drew_at: Option<Instant> = None;
    let mut idle_said = false;
    while !stop.load(Ordering::Relaxed) {
        // The next slot and how long it is away. The capture waits that long for
        // the desktop to change, so a still screen costs one wake-up a slot and
        // no spin at all.
        let (due, wait) = pace(started, slot, sink.fps, Instant::now());
        let deadline = Instant::now() + wait.unwrap_or_default();
        let arrival = capture.frame(wait.unwrap_or_default())?;
        // Answered before the slot is due: nothing is sent yet, and the slot
        // stays the one being waited for.
        let early = Instant::now() < deadline;
        if !early {
            slot = due.saturating_add(1);
        }
        // Told to stop while the capture was waiting: the capture goes with this
        // frame, which closes the portal's session and lets go of the X server's
        // redirection.
        if stop.load(Ordering::Relaxed) {
            break;
        }
        // Everything the sink has to say about this pass: the controls, which is
        // also where its reports go, and whether a preview is wanted. None of it
        // needs the picture.
        let switched = sink.check(sampler.read(Instant::now()), report)?;
        let wanted = sink.preview_wanted();
        if wanted && !was_wanted {
            preview_pending = true;
        }
        was_wanted = wanted;
        if let Some(next) = switched {
            // Another screen or window. The descriptor of the one before belongs
            // to the capture that made it, so the picture goes with it, and the
            // sink keeps its encoder: nothing on this path read on a device the
            // capture made. What can't be opened is said, and the share carries on
            // from the processor rather than ending.
            match dmabuf_capture(&next, &mut sink) {
                Ok(opened) => {
                    tracing::info!(
                        rung = opened.rung.name(),
                        why = opened.why.as_str(),
                        capture = opened.capture.describe().as_str(),
                        "sharing another screen from the card"
                    );
                    capture = opened.capture;
                    rung = opened.rung;
                    source = next;
                    // The next picture says what is shared now, even at the size
                    // the screen before it was.
                    size = None;
                    saw_picture = false;
                    last = None;
                    fresh = false;
                    preview_pending = true;
                }
                Err(why) => {
                    tracing::info!(
                        why = format!("{why:#}"),
                        "switching screens: sharing from the processor"
                    );
                    sink.fall_back_to_the_processor();
                    return Ok((sink, After::Processor(next)));
                }
            }
        }
        match arrival {
            Found::Picture(shot) => {
                let now = Instant::now();
                sink.diagnosis.captured_at = Some(now);
                if idle_said {
                    idle_said = false;
                    tracing::info!(
                        still_for_ms =
                            drew_at.map_or(0, |at| now.duration_since(at).as_millis() as u64),
                        "the desktop is drawing again"
                    );
                }
                drew_at = Some(now);
                saw_picture = true;
                last = Some(shot);
                fresh = true;
                if early {
                    continue;
                }
            }
            Found::Resized => {
                // What the capture held was drawn at the size before it, and the
                // next picture is at the new one. Nothing is sent of a picture of
                // the old size under the new one's numbers: the next picture
                // opens the encoder at its own size and tells the sender.
                tracing::info!("the shared picture changed size");
                saw_picture = false;
                last = None;
                fresh = false;
                size = None;
                preview_pending = true;
                continue;
            }
            Found::Still => {
                // Nothing has come yet: what the capture is waiting for, now and
                // then, rather than only in the line this share opened with. A
                // portal stream that never negotiates says which state it is in
                // and what the producer answered, which is otherwise a share that
                // shows nothing with no reason anywhere.
                if !saw_picture && waited.is_none_or(|last| last.elapsed() >= WAITING_EVERY) {
                    waited = Some(Instant::now());
                    tracing::info!(
                        capture = capture.describe().as_str(),
                        "the share has no picture yet"
                    );
                }
                // A screen that has drawn something and then stops sends nothing:
                // the compositor draws a frame when it has one, and a share's
                // pictures are the compositor's. Said once, so a quiet stream is
                // known to be a quiet desktop rather than a share that stalled,
                // and the tile is known to hold its last picture on purpose.
                if saw_picture && !idle_said && drew_at.is_some_and(|at| at.elapsed() >= IDLE_EVERY)
                {
                    idle_said = true;
                    tracing::info!(
                        still_for_ms = drew_at.map(|at| at.elapsed().as_millis() as u64),
                        "the desktop has drawn nothing for a while: the share keeps its last picture, and the stream sends nothing until it draws"
                    );
                }
                // Nothing has been drawn since the last picture. It is still a
                // stream a viewer joining, a decoder that lost its place or a
                // preview coming back can be shown — from the picture the capture
                // last handed over, and only while one of those asks for it.
                // Woken before the slot, as for a format: the slot is still to
                // come, and with it whatever arrives meanwhile.
                if early {
                    continue;
                }
                if !sending(fresh, saw_picture, sink.picture_needed()) {
                    continue;
                }
            }
            Found::Closed => {
                let why = capture.ending();
                bail!(if why.is_empty() {
                    "the screen this share captured ended".to_owned()
                } else {
                    why
                })
            }
        }
        let Some(shot) = last.as_ref() else {
            continue;
        };
        // Whether this pass has a picture that has just arrived, which the stream
        // sends and the window is shown: both are answered once, and the two are
        // not the same — a still desktop is one the window can be shown again
        // without the stream sending it.
        let arrived = fresh;
        fresh = false;
        let sent = fit(shot.width, shot.height, max_width, max_height);
        if size != Some(sent) {
            size = Some(sent);
            sink.capture = format!("{} · {}×{}", rung.name(), sent.0, sent.1);
            report(Report::Sharing {
                width: sent.0,
                height: sent.1,
            });
        }
        match sink.dmabuf(shot, sent, &mut scratch)? {
            Taken::Sent => {}
            Taken::Held => continue,
            Taken::Processor(why) => {
                tracing::info!(
                    why = why.as_str(),
                    "sharing from the processor: a picture nothing here can read"
                );
                sink.fall_back_to_the_processor();
                // The reason this share left the card is this one, and not the
                // sentence the fallback says on its own.
                sink.diagnosis.fallback = Some(why);
                return Ok((sink, After::Processor(source)));
            }
        }
        // Discord's still, and only that: a screen is black until what is shared
        // draws, so the picture is looked at before one is made, and it is made
        // from the frame the share has just sent. This is the one picture a share
        // of DMA-BUFs copies.
        if sink.still_wanted() {
            // A share the card encodes has the picture it just sent on the card
            // already, at the stream's size, and a tiled screen has no rows the
            // processor could read it from instead.
            #[cfg(nvcuda)]
            let still = sink
                .card_still(&mut scratch)
                .unwrap_or_else(|| shot.still(&mut scratch));
            #[cfg(not(nvcuda))]
            let still = shot.still(&mut scratch);
            match still {
                Err(error) => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "sharing without Discord's still: the picture couldn't be read back"
                    );
                    // Asked again in a while, not on every frame.
                    sink.preview_due = Some(Instant::now() + STILL_RETRY);
                }
                Ok(()) => {
                    if let Some(full) = scratch.full.as_ref() {
                        let shown = planes(full);
                        if !is_black(shown.y) {
                            match thumbnail(&shown) {
                                Ok(jpeg) => sink.thumbnail = Some(jpeg),
                                Err(error) => tracing::warn!(
                                    error = format!("{error:#}"),
                                    "could not make a preview"
                                ),
                            }
                            sink.preview_due = Some(Instant::now() + THUMBNAIL_EVERY);
                        }
                    }
                }
            }
        }
        // The picture the window shows where the compositor drew it: the tile is
        // the descriptor itself, published where a slot is free and only while
        // something shows one. Every picture that arrives is published rather
        // than the one that was there when the preview came up: the capture's
        // buffers cycle through the compositor's pool, so a tile left on one of
        // them shows what that buffer held the last time it came round, and the
        // window redraws only when it is handed another. A still desktop
        // publishes nothing, the buffer the window has keeping what it shows. A
        // window that can't sample one after all — its socket has gone, or it
        // stopped retiring slots — leaves the share on the processor, which is
        // where its pixels are.
        if publishing(wanted, preview_pending || arrived)
            && !readback
            && let Some(_slot) = sink.writer.claim()
        {
            if sink.writer.publish_dmabuf(&shot.descriptor()) {
                sink.diagnosis.presentation = Some(Presentation::Shared);
                preview_pending = false;
            } else if encodes_descriptors() {
                // The window draws on the processor and can't sample a descriptor,
                // so its preview is read back — the same copy Discord's still is
                // made of — while the encoder goes on reading the card's own
                // picture. Said once, since it holds for the rest of the share.
                tracing::info!(
                    "the window draws on the processor: its preview is a copy of the picture, and \
                     the stream is still the card's own"
                );
                readback = true;
                // The tile is a copy; the stream is not. The report's
                // presentation says what the window shows, and the fallback is
                // reserved for the stream's own path — a share whose pictures
                // the card encodes would otherwise read as one the processor
                // took over.
                sink.diagnosis.presentation = Some(Presentation::Copy);
            } else {
                tracing::info!(
                    "sharing from the processor: the window can't show the card's picture"
                );
                sink.fall_back_to_the_processor();
                sink.diagnosis.fallback =
                    Some("the window can't show the picture the card drew".to_owned());
                return Ok((sink, After::Processor(source)));
            }
        }
        // A read-back preview is made only while a tile shows one, and only of
        // the picture the window can't sample: one copy of the screen a preview
        // frame, which is what a software renderer costs a share.
        if readback && wanted {
            // A picture the card's own path took through GL's door — a tiled
            // screen, which has no rows to read — is scaled on the card and only
            // the preview comes back; one with rows is read by the processor.
            #[cfg(nvcuda)]
            let read = match sink.card_preview(&mut scratch) {
                Some(Ok(())) => Ok(()),
                _ => shot.still(&mut scratch),
            };
            #[cfg(not(nvcuda))]
            let read = shot.still(&mut scratch);
            if let Err(error) = read {
                // Once a share: a picture that can't be read back now won't be
                // on the next frame either, and this is asked at frame rate.
                if !preview_refused {
                    preview_refused = true;
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "sharing without a preview: the picture couldn't be read back"
                    );
                }
            } else if let Some(full) = scratch.full.as_ref() {
                sink.preview(full);
                preview_pending = false;
            }
        }
    }
    Ok((sink, After::Ended))
}

/// A capturer sending a screen's, window's or camera's frames to the sink.
struct Capture {
    /// A screen's or window's, which gives a frame when asked.
    capturer: Option<livekit::webrtc::desktop_capturer::DesktopCapturer>,
    /// A camera's, which sends its frames from its own thread.
    _camera: Option<crate::camera::Camera>,
    captured: Arc<Mutex<Captured>>,
}

/// Whether the processor's capturer is asked for a window of its own: a window
/// is shared as that window — the same one the card's rungs refused, when they
/// did — and every other source as a screen. Where [`portal_picks`], the
/// system's own dialog picks either, so this is not what it is told.
fn capturer_takes_a_window(source: &Source) -> bool {
    matches!(source, Source::Window(_))
}

/// The kind of source the processor's capturer is asked for. Where
/// [`portal_picks`], the system's own screen sharing dialog picks a screen or a
/// window, and the kind only says what the dialog offers first — and the generic
/// kind is one only libwebrtc's Linux and macOS builds make.
fn capturer_kind(source: &Source) -> livekit::webrtc::desktop_capturer::DesktopCaptureSourceType {
    #[cfg(target_os = "linux")]
    if portal_picks() {
        return livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Generic;
    }
    if capturer_takes_a_window(source) {
        livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Window
    } else {
        livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Screen
    }
}

impl Capture {
    /// Starts capturing `source`, fitting its frames in `max_width` by
    /// `max_height`; where [`portal_picks`], a screen or window the system's
    /// dialog picks.
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
            CaptureError, DesktopCapturer, DesktopCapturerOptions,
        };
        let kind = capturer_kind(&source);
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
        let source = if portal_picks() {
            None
        } else {
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

/// The same, for a picture whose red and blue bytes came in `order`: a
/// compositor draws a desktop in one of the two, and libyuv names the other
/// one ABGR.
///
/// The tone is SDR's: a desktop the compositor drew is the picture the viewer's
/// player would show, and a DMA-BUF carries no metadata to tone from.
#[cfg(target_os = "linux")]
fn convert_ordered(picture: &mut I420Buffer, rows: &[u8], stride: u32, order: Order) {
    let (width, height) = (picture.width(), picture.height());
    let (y_stride, u_stride, v_stride) = picture.strides();
    let (y, u, v) = picture.data_mut();
    let (width, height) = (width as i32, height as i32);
    match order {
        Order::Bgra => yuv_helper::argb_to_i420(
            rows, stride, y, y_stride, u, u_stride, v, v_stride, width, height,
        ),
        Order::Rgba => yuv_helper::abgr_to_i420(
            rows, stride, y, y_stride, u, u_stride, v, v_stride, width, height,
        ),
    }
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

/// How long [`glance`] waits for a picture of a screen that isn't black,
/// before showing the last it took, or none.
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
    for &id in screens {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match glance_at(id, stop) {
            Ok(Some(picture)) => {
                let (width, height) = fit(picture.width(), picture.height(), max_width, max_height);
                let mut rgba = vec![0; (width * height * 4) as usize];
                let mut scratch = Vec::new();
                planes(&picture).to_rgba(&mut scratch, &mut rgba, width, height);
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
            Ok(frame) => {
                let (width, height) = (frame.width().max(1) as u32, frame.height().max(1) as u32);
                // The last picture is the buffer for this one, unless the
                // screen changed size under it.
                let mut picture = taken
                    .0
                    .take()
                    .unwrap_or_else(|| I420Buffer::new(width, height));
                if (picture.width(), picture.height()) != (width, height) {
                    picture = I420Buffer::new(width, height);
                }
                convert(&mut picture, frame.data(), frame.stride(), &tone);
                taken.0 = Some(picture);
            }
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

/// Whether a stream may declare more than the safe cap for its picture:
/// Settings' switch, off until the UI says. It travels with the job because the
/// worker is a process of its own, and it is the worker that declares the
/// stream: op 12 carries the ceiling once, when a share starts, so a share
/// already running keeps the ceiling it declared.
static HIGH_BITRATE: AtomicBool = AtomicBool::new(false);

/// Raises the ceiling the next share declares, from 9 Mbit/s to 35; see
/// [`stream_connection::set_high_bitrate`].
pub fn set_high_bitrate(on: bool) {
    HIGH_BITRATE.store(on, Ordering::Relaxed);
}

/// The codec Settings' experimental choice asks a share to offer first, as an
/// index into [`StreamCodec::ALL`]. It travels with the job because the worker
/// is a process of its own, and it is the worker that offers codecs to Discord.
static STREAM_CODEC: AtomicU8 = AtomicU8::new(0);

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
fn stream_codec() -> StreamCodec {
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

/// Yapfast's handle on a running worker. Dropping it stops the worker.
pub struct Worker {
    /// Shared with the thread reading its reports, which waits on it when
    /// it ends without saying so.
    child: Arc<Mutex<Child>>,
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

/// What [`deliver`] left for the caller to do. As with [`Report`], the big
/// variant is one message handed over once.
#[allow(clippy::large_enum_variant)]
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
            hardware: crate::encode::hardware(),
            high_bitrate: HIGH_BITRATE.load(Ordering::Relaxed),
            codec: stream_codec(),
            capture: crate::capture::preference(),
        };
        let (worker, reports) = Self::start(&job, Some(&frames))?;
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
        Self::start(
            &Job::Glance {
                screens,
                width,
                height,
            },
            None,
        )
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
        let (worker, reports) = Self::start(&job, Some(&frames))?;
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
        let (worker, reports) = Self::start(&job, Some(&frames))?;
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

    /// Tell a watching worker the size in pixels of the tile showing its
    /// pictures, and a sharing one the size of the tile showing its preview:
    /// each scales what it writes to it; see [`Control::Size`].
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

    /// Starts the worker for `job`, whose frames file `frames` is when it has
    /// one: a stream's pictures travel over a socket this makes before the spawn,
    /// so that the worker inherits its end; see [`crate::dmabuf`].
    fn start(job: &Job, frames: Option<&Frames>) -> Result<(Self, mpsc::Receiver<Report>)> {
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
        // The socket this stream's pictures' descriptors travel on: the frames
        // header names the descriptor the worker's end has, and the spawn below
        // keeps it open across the exec. A stream whose socket can't be made is a
        // stream of pixels, which is what it would have been anyway.
        #[cfg(target_os = "linux")]
        let pictures = frames
            .and_then(|frames| {
                frames
                    .open_pictures()
                    .inspect_err(|error| {
                        tracing::warn!(
                            error = format!("{error:#}"),
                            "a stream whose pictures can't be shared as descriptors shows its pixels"
                        )
                    })
                    .ok()
            })
            .map(|fd| fd as std::os::fd::RawFd);
        #[cfg(not(target_os = "linux"))]
        let _ = frames;
        // `mut` only where the closure is called directly: on Linux it is
        // handed to `inheriting`, which calls it, and the borrow is the same.
        #[cfg_attr(target_os = "linux", allow(unused_mut))]
        let mut spawning = || {
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(crate::logging::for_worker())
                .spawn()
        };
        let mut child = {
            #[cfg(target_os = "linux")]
            {
                crate::dmabuf::inheriting(pictures, spawning)
            }
            #[cfg(not(target_os = "linux"))]
            {
                spawning()
            }
        }
        .context("could not start the stream worker")?;
        // This process's copy of the worker's end goes now that the worker has
        // its own: from here the window's end sees the socket close when the
        // worker goes, which is how a stream's pictures end.
        #[cfg(target_os = "linux")]
        if let Some(frames) = frames {
            frames.pictures_spawned();
        }
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
        let child = Arc::new(Mutex::new(child));
        let mut last = Last::new(job);
        let waited = child.clone();
        // The caller's, so a test's logger hears this thread too.
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        std::thread::Builder::new()
            .name("yapfast-stream-reports".into())
            .spawn(move || {
                let _logging = tracing::dispatcher::set_default(&dispatch);
                let mut ended = false;
                for line in stdout
                    .into_iter()
                    .flat_map(|stdout| BufReader::new(stdout).lines().map_while(Result::ok))
                {
                    match serde_json::from_str::<Report>(&line) {
                        Ok(report) => {
                            ended |= matches!(report, Report::Ended { .. });
                            last.saw(&report);
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
                    // The log says how it ended, where its own words don't;
                    // and the UI hears of a worker gone quietly, its end
                    // worth waiting for room for, like any other.
                    last.log(exited(&waited));
                    let _ = reports.blocking_send(Report::Ended {
                        error: Some(unexpected.into()),
                    });
                }
            })
            .context("could not start the stream worker")?;
        Ok((
            Self {
                child,
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

/// What a worker's reports last said about its job, to log if it ends
/// without saying why.
struct Last {
    job: &'static str,
    /// The encoder's engine and codec, once a share reports how it goes out.
    encoder: Option<(String, String)>,
    /// The size reported, or the size the job fits in until one is.
    size: (u32, u32),
    reported: bool,
}

impl Last {
    fn new(job: &Job) -> Self {
        Self {
            job: job.kind(),
            encoder: None,
            size: job.bounds(),
            reported: false,
        }
    }

    fn saw(&mut self, report: &Report) {
        match report {
            Report::Sharing { width, height } | Report::Opened { width, height, .. } => {
                (self.size, self.reported) = ((*width, *height), true);
            }
            Report::Sending(stats) => {
                (self.size, self.reported) = ((stats.width, stats.height), true);
                self.encoder = Some((stats.engine.clone(), stats.codec.clone()));
            }
            _ => {}
        }
    }

    /// What the log says about a worker that ended without a
    /// [`Report::Ended`]: the job it had, what encoded it and how big the
    /// pictures were, and how it went. The same words the warning carries, so a
    /// test can read them without a logger to catch it through.
    fn said(&self, exit: &str) -> String {
        let (width, height) = self.size;
        let (engine, codec) = self
            .encoder
            .as_ref()
            .map_or(("unknown", "unknown"), |(engine, codec)| {
                (engine.as_str(), codec.as_str())
            });
        let size = if self.reported {
            format!("{width}x{height}")
        } else {
            format!("up to {width}x{height}")
        };
        format!(
            "the stream worker ended without saying why: job=\"{}\" engine=\"{}\" codec=\"{}\" \
             size=\"{}\" exit=\"{}\"",
            self.job, engine, codec, size, exit
        )
    }

    /// Warns that the worker ended without a [`Report::Ended`], and how: the
    /// one sentence [`Self::said`] writes, so a test reads what the log says
    /// rather than a second copy of it.
    fn log(&self, exit: String) {
        tracing::warn!("{}", self.said(&exit));
    }
}

/// How the worker ended, once its output closed: it has exited by then, or
/// is about to.
fn exited(child: &Mutex<Child>) -> String {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let waited = child.lock().map(|mut child| child.try_wait());
        match waited {
            Ok(Ok(Some(status))) => return how(status),
            Ok(Ok(None)) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(Ok(None)) => return "still running".into(),
            Ok(Err(error)) => return format!("unknown ({error})"),
            Err(_) => return "unknown".into(),
        }
    }
}

/// An exit status for the log: the signal that killed it on Unix, such as
/// `signal=11 (SIGSEGV)`, or its exit code.
fn how(status: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            let core = if status.core_dumped() {
                ", core dumped"
            } else {
                ""
            };
            return match signal_name(signal) {
                Some(name) => format!("signal={signal} ({name}{core})"),
                None => format!("signal={signal}{core}"),
            };
        }
    }
    match status.code() {
        // Windows reports a crash as an NTSTATUS code, such as 0xC0000005
        // for an access violation.
        #[cfg(windows)]
        Some(code) if code < 0 => format!("code={code} ({:#010X})", code as u32),
        Some(code) => format!("code={code}"),
        None => status.to_string(),
    }
}

/// The name of a signal that commonly ends a process.
#[cfg(unix)]
fn signal_name(signal: i32) -> Option<&'static str> {
    Some(match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        4 => "SIGILL",
        5 => "SIGTRAP",
        6 => "SIGABRT",
        #[cfg(target_os = "linux")]
        7 => "SIGBUS",
        #[cfg(not(target_os = "linux"))]
        10 => "SIGBUS",
        8 => "SIGFPE",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        15 => "SIGTERM",
        _ => return None,
    })
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
        let child = self.child.clone();
        let stopping = std::thread::Builder::new()
            .name("yapfast-stream-stop".into())
            .spawn(move || {
                let deadline = Instant::now() + STOP_WAIT;
                loop {
                    let waited = child.lock().map(|mut child| child.try_wait());
                    match waited {
                        Ok(Ok(None)) if Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Ok(Ok(None)) => {
                            tracing::warn!("the stream worker didn't stop; ending it");
                            if let Ok(mut child) = child.lock() {
                                let _ = child.kill();
                                let _ = child.wait();
                            }
                            return;
                        }
                        _ => return,
                    }
                }
            });
        if let Err(error) = stopping {
            tracing::warn!(%error, "could not wait for the stream worker to stop");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A worker that dies without a [`Report::Ended`] is one the UI hears about
    /// anyway, with the job it had and how it went — and the warning that says
    /// so is the same sentence, which is checked here without a logger to catch
    /// it through: a scoped logger is hearsay under a test binary whose other
    /// tests may have started the logging first.
    #[cfg(unix)]
    #[test]
    fn a_worker_that_dies_says_how() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let died = |script: &str| {
            let worker = directory.path().join("worker");
            std::fs::write(&worker, format!("#!/bin/sh\nread job\n{script}\n")).unwrap();
            std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o755)).unwrap();
            // Only this test starts a worker.
            unsafe { std::env::set_var("YAPFAST_STREAM_WORKER", &worker) };
            let (worker, mut reports) = Worker::start(
                &Job::Glance {
                    screens: vec![],
                    width: 320,
                    height: 180,
                },
                None,
            )
            .unwrap();
            assert_eq!(
                reports.blocking_recv(),
                Some(Report::Ended {
                    error: Some("Screen sharing stopped unexpectedly.".into())
                })
            );
            // The image the popup shows, and the exit the warning carries.
            let exit = exited(&worker.child);
            drop(worker);
            (
                exit,
                Last::new(&Job::Glance {
                    screens: vec![],
                    width: 320,
                    height: 180,
                }),
            )
        };

        // No core file: the shell's would be in the way.
        let (exit, last) = died("ulimit -c 0; kill -SEGV $$");
        assert!(exit.contains("signal=11 (SIGSEGV"), "{exit}");
        let log = last.said(&exit);
        assert!(log.contains("ended without saying why"), "{log}");
        assert!(log.contains("job=\"glance\""), "{log}");
        assert!(log.contains("size=\"up to 320x180\""), "{log}");
        assert!(log.contains("engine=\"unknown\""), "{log}");

        let (exit, last) = died("exit 3");
        assert!(exit.contains("code=3"), "{exit}");
        assert!(last.said(&exit).contains("exit=\"code=3\""), "{exit}");
    }

    #[cfg(unix)]
    #[test]
    fn says_which_signal_ended_it() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(how(ExitStatus::from_raw(11)), "signal=11 (SIGSEGV)");
        assert_eq!(
            how(ExitStatus::from_raw(6 | 0x80)),
            "signal=6 (SIGABRT, core dumped)"
        );
        assert_eq!(how(ExitStatus::from_raw(1 << 8)), "code=1");
    }

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

    /// A window is the card's only where Windows Graphics Capture can take
    /// one: DXGI duplicates a display and nothing else, and a person who chose
    /// a window never gets the desktop it happens to sit on.
    #[test]
    fn a_window_is_the_cards_only_where_capture_takes_one() {
        assert_eq!(on_the_card(&Source::Window(0x1234)), cfg!(wgc));
        assert!(on_the_card(&Source::Screen));
        assert!(on_the_card(&Source::Display(0)), "the primary display");
        assert!(!on_the_card(&Source::Camera("cam".into())));
        assert!(!on_the_card(&Source::TestPattern));
    }

    /// What the card asks Windows Graphics Capture for is what the person chose:
    /// a window's own handle goes to `CreateForWindow`, a display's to
    /// `CreateForMonitor`, and there is no source that is captured as the other
    /// kind. Nothing here needs a screen or a window, so the rule is checked on
    /// every platform this crate builds for, including the one where the rung
    /// isn't carried at all.
    #[cfg(any(wgc, test))]
    #[test]
    fn the_card_captures_a_window_as_itself_and_never_as_a_display() {
        // The handle `Source::Window` carries is the one the window is captured
        // by, unchanged: nothing reinterprets it as a monitor's.
        assert_eq!(
            card_target(&Source::Window(0x1234)).ok(),
            Some(CardTarget::Window(0x1234))
        );
        // A screen and a display are displays, and the primary display is the
        // one with no handle of its own.
        assert_eq!(
            card_target(&Source::Screen).ok(),
            Some(CardTarget::Display(None))
        );
        assert_eq!(
            card_target(&Source::Display(3)).ok(),
            Some(CardTarget::Display(Some(3)))
        );
        // Whichever source is asked about, the two kinds of target are never
        // each other's.
        for source in [
            Source::Window(7),
            Source::Screen,
            Source::Display(1),
            Source::Camera("cam".into()),
            Source::TestPattern,
        ] {
            let kind = card_target(&source).ok();
            assert_eq!(
                matches!(kind, Some(CardTarget::Window(_))),
                matches!(source, Source::Window(_)),
                "{source:?} is captured as {kind:?}"
            );
        }
        // A camera and a test pattern are no screen, and are refused by name
        // rather than captured as the desktop behind them.
        for source in [Source::Camera("cam".into()), Source::TestPattern] {
            let refused = card_target(&source).expect_err("not a screen");
            assert!(!format!("{refused:#}").is_empty(), "{source:?}");
        }
    }

    /// The fallback keeps the window it was given: what the card's paths can't
    /// take is the processor's, and the processor's capturer is asked for a
    /// window of its own rather than for a screen — the same window, by the same
    /// handle, and never the display it sits on.
    #[test]
    fn a_window_the_card_cannot_take_goes_on_as_that_window() {
        for source in [
            Source::Window(0x1234),
            Source::Screen,
            Source::Display(2),
            Source::Camera("cam".into()),
            Source::TestPattern,
        ] {
            assert_eq!(
                capturer_takes_a_window(&source),
                matches!(source, Source::Window(_)),
                "{source:?}"
            );
        }
        // A build with Windows Graphics Capture keeps a window on the card, so a
        // window reaches the processor only where the card has no way to take
        // one — and there it arrives as the window it was.
        assert_eq!(on_the_card(&Source::Window(0x1234)), cfg!(wgc));
        // Where the system's own dialog picks, there is no kind to ask for: it
        // picks a screen or a window, and what it picks is what the fallback
        // shares. Everywhere else the kind carries the choice.
        if portal_picks() {
            assert_eq!(
                capturer_kind(&Source::Window(0x1234)),
                livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Generic
            );
        } else {
            assert_eq!(
                capturer_kind(&Source::Window(0x1234)),
                livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Window
            );
            assert_eq!(
                capturer_kind(&Source::Display(2)),
                livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Screen
            );
        }
    }

    /// A duplication asked for a window says so rather than capturing the
    /// display behind it: the person chose the window.
    #[cfg(dup)]
    #[test]
    fn a_duplication_never_takes_a_window_for_a_display() {
        let why = duplication_target(&Source::Window(0x1234)).unwrap_err();
        assert!(format!("{why:#}").contains("window"), "{why:#}");
    }

    #[test]
    fn fits_frames_with_even_sizes() {
        assert_eq!(fit(1920, 1080, 1280, 720), (1280, 720));
        assert_eq!(fit(3440, 1440, 1280, 720), (1280, 534));
        assert_eq!(fit(801, 601, 1280, 720), (800, 600), "never larger");
    }

    /// The box a stream fits in turns with the picture, so a screen on its side
    /// keeps its own shape at the resolution asked for instead of being shrunk
    /// to a landscape frame's height.
    #[test]
    fn a_portrait_screen_fits_the_box_stood_up() {
        // 1080x1920 on a 1080p share: every pixel of it, not 606x1080.
        assert_eq!(fit(1080, 1920, SHARE_WIDTH, SHARE_HEIGHT), (1080, 1920));
        assert_eq!(fit(1080, 1920, 1280, 720), (720, 1280));
        assert_eq!(fit(1080, 1920, 3840, 2160), (1080, 1920), "never larger");
        // A landscape screen is fitted exactly as it was.
        assert_eq!(fit(1920, 1080, SHARE_WIDTH, SHARE_HEIGHT), (1920, 1080));
        assert_eq!(fit(3440, 1440, SHARE_WIDTH, SHARE_HEIGHT), (1920, 802));
        // A square box fits either way round.
        assert_eq!(fit(1080, 1920, 1920, 1920), (1080, 1920));
    }

    /// A preview drawn on the card is the stream's own picture, up to the 1920
    /// its longest edge is allowed; the shape of the screen decides which edge
    /// that is.
    #[cfg(any(wgc, dup))]
    #[test]
    fn a_preview_on_the_card_keeps_the_streams_pixels() {
        assert_eq!(preview_size(3440, 1440, false), (1920, 802));
        // A machine short of memory takes the smaller box, and never upscales.
        assert_eq!(preview_size(3440, 1440, true), (640, 266));
        assert_eq!(preview_size(1080, 1920, false), (1080, 1920));
        assert_eq!(preview_size(1080, 1920, true), (360, 640));
        assert_eq!(preview_size(1280, 720, false), (1280, 720), "never larger");
        assert_eq!(
            preview_size(320, 180, true),
            (320, 180),
            "and never larger here"
        );
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
            high_bitrate: false,
            codec: StreamCodec::Automatic,
            capture: crate::capture::Preference::Auto,
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
        let size = write_picture(&mut writer, &shown, &mut scratch, (32, 18), 0);
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
        let size = write_picture(&mut writer, &shown, &mut scratch, (32, 18), 0);
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

    /// A share is declared what its picture needs: bits per pixel per frame,
    /// so the same quality at every size, under the safe ceiling.
    #[test]
    fn aims_for_what_the_picture_needs() {
        // 0.07 bits a pixel a frame, under the 9 Mbit/s a share declares by
        // default: a 1080p60 share asks for less than that and gets it.
        assert_eq!(kbps(1920, 1080, 60), 8_709);
        assert_eq!(kbps(1280, 720, 30), 3_500, "the floor a screen is given");
        // Above that the declaration is cut to the ceiling, which is what
        // Settings' experimental switch raises.
        assert_eq!(kbps(3440, 1440, 60), 9_000, "cut to the safe cap");
        assert_eq!(kbps(3840, 2160, 60), 9_000);
        assert_eq!(kbps(7680, 4320, 60), 9_000, "whatever the size");
        // A frame rate a person picks takes its share of the bits with it,
        // under the cap either way.
        assert_eq!(kbps(3840, 2160, 30), 9_000);
        assert_eq!(kbps(1920, 1080, 30), 4_354);
        // A screen on its side is the same picture, so it is worth the same.
        assert_eq!(kbps(1080, 1920, 30), kbps(1920, 1080, 30));
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
            preview_tile: None,
            capture: String::new(),
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
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
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

    /// The codecs to offer Discord, most wanted first, with the settings' own
    /// codec in front where this machine encodes it.
    #[test]
    fn the_settings_codec_is_offered_first() {
        let card = vec![Codec::Av1, Codec::H265, Codec::H264];
        assert_eq!(
            preferred(card.clone(), StreamCodec::Automatic),
            card,
            "automatic is the order the card's codecs are in"
        );
        assert_eq!(
            preferred(card.clone(), StreamCodec::H264),
            vec![Codec::H264, Codec::Av1, Codec::H265]
        );
        assert_eq!(
            preferred(card, StreamCodec::H265),
            vec![Codec::H265, Codec::Av1, Codec::H264]
        );
        // A codec this machine has no encoder for isn't offered at all, so
        // there is nothing to put first: the order is what it was.
        assert_eq!(
            preferred(vec![Codec::H264], StreamCodec::Av1),
            vec![Codec::H264]
        );
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
            preview_tile: None,
            capture: String::new(),
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
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
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
    #[cfg(any(wgc, dup))]
    #[test]
    fn a_share_the_card_cannot_capture_says_why() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_tile: None,
            capture: String::new(),
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
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
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

    /// The one log line says which rung ran and why: the setting's choice where
    /// that decided, the platform's answer under Auto, and every refusal before
    /// the rung that opened, in the order they were tried. A bordered Graphics
    /// Capture that was let go for Duplication therefore says so in the same
    /// line, in the shim's own words.
    #[cfg(any(wgc, dup))]
    #[test]
    fn the_chosen_rung_says_why_it_is_the_one() {
        use crate::capture::{Available, Platform, Preference, Rung};

        let both = Available {
            graphics: true,
            duplication: true,
        };

        // The setting's own answer, on a Windows where the other way would be
        // first under Auto.
        assert_eq!(
            chosen_why(
                Preference::GraphicsCapture,
                Platform::new(19045),
                both,
                Rung::Graphics,
                ""
            ),
            "the setting asks for Windows Graphics Capture"
        );
        assert_eq!(
            chosen_why(
                Preference::Duplication,
                Platform::new(22621),
                both,
                Rung::Duplication,
                ""
            ),
            "the setting asks for Desktop Duplication"
        );
        // Auto reads the platform: Windows 10's Graphics Capture can't take
        // its border off, so Duplication comes first and the line says so; on
        // Windows 11, Graphics Capture can and does.
        let windows_10 = chosen_why(
            Preference::Auto,
            Platform::new(19045),
            both,
            Rung::Duplication,
            "",
        );
        assert!(
            windows_10.contains("border") && windows_10.contains("Duplication"),
            "{windows_10}"
        );
        let windows_11 = chosen_why(
            Preference::Auto,
            Platform::new(22621),
            both,
            Rung::Graphics,
            "",
        );
        assert!(
            windows_11.contains("border") && windows_11.contains("Graphics"),
            "{windows_11}"
        );
        // A build with no Graphics Capture rung says that, rather than blaming
        // the border on a capture it doesn't carry.
        let duplication_only = chosen_why(
            Preference::Auto,
            Platform::new(22621),
            Available {
                graphics: false,
                duplication: true,
            },
            Rung::Duplication,
            "",
        );
        assert!(
            duplication_only.contains("no Windows Graphics Capture"),
            "{duplication_only}"
        );

        // Each refusal carries its rung's name and what it said, in the order
        // tried, and the rung that opened is named after them.
        let mut refused = String::new();
        refuse(
            &mut refused,
            Rung::Graphics,
            "the border is on: nothing took",
        );
        let said = chosen_why(
            Preference::Auto,
            Platform::new(22621),
            both,
            Rung::Duplication,
            &refused,
        );
        let graphics = said
            .find("Windows Graphics Capture: the border is on")
            .unwrap_or_else(|| panic!("the rung that refused is named with its reason: {said}"));
        let duplication = said
            .find("Desktop Duplication came next")
            .unwrap_or_else(|| panic!("the rung that opened is named: {said}"));
        assert!(
            graphics < duplication,
            "in the order they were tried: {said}"
        );

        let mut both = String::new();
        refuse(&mut both, Rung::Graphics, "no");
        refuse(&mut both, Rung::Duplication, "no either");
        assert_eq!(
            both,
            "Windows Graphics Capture: no; Desktop Duplication: no either"
        );
    }

    /// A duplication takes the same displays the first rung does — the primary
    /// one when nothing was picked, and the one the shell listed otherwise — and
    /// refuses everything that isn't a display by its own name, rather than
    /// duplicating the primary display and calling it the chosen one.
    #[cfg(dup)]
    #[test]
    fn a_duplication_names_the_display_it_captures_and_what_it_cannot() {
        for (source, why) in [
            (Source::Window(7), "window"),
            (Source::Camera("none".into()), "camera"),
            (Source::TestPattern, "drawn"),
        ] {
            let Err(error) = duplication_target(&source) else {
                panic!("{source:?} isn't a display to duplicate");
            };
            let said = format!("{error:#}");
            assert!(said.contains(why), "{said}");
        }
        // A screen is named by its place in the system's enumeration, so a
        // place no display is listed at names nothing.
        let Err(error) = duplication_target(&Source::Display(u64::MAX)) else {
            panic!("a place no display is listed at named one");
        };
        assert!(
            format!("{error:#}").contains("no display is listed"),
            "{error:#}"
        );

        // And the place that does have one is turned into that display's own
        // handle, which is what DXGI is asked for: screen 1 on this machine is
        // not `HMONITOR 0x1`, which is what a share of it used to look for.
        let listed = sources(livekit::webrtc::desktop_capturer::DesktopCaptureSourceType::Screen);
        for source in listed {
            let Some(monitor) = monitor_of(source.id()).ok() else {
                panic!("a listed screen is a display: {:?}", source.id());
            };
            assert_ne!(monitor, source.id(), "a place is not a handle");
            if let Err(error) = target_of_monitor(monitor) {
                panic!("a listed display is one DXGI enumerates: {error:#}");
            }
        }
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
            kbps(sent_width, sent_height, 30),
            d3d11::Surfaces::Ring,
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
            high_bitrate: false,
            codec: StreamCodec::Automatic,
            capture: crate::capture::Preference::Auto,
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
            preview_tile: None,
            capture: String::new(),
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
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
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
        // A tile's size, once the window says which tile shows it, is what the
        // preview is written at: this picture in a 480x270 tile is 480x270, not
        // the box above.
        sink.preview_tile = Some((480, 270));
        sink.frame(&picture).unwrap();
        let shown = frames.take(|width, height, _| (width, height)).unwrap();
        assert_eq!(shown, (480, 270));
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
            preview_tile: None,
            capture: String::new(),
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
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
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
        sink.check(pressure::Reading::default(), &mut report)
            .unwrap();
        sink.frame(&picture).unwrap();
        let (width, height) = frames
            .take(|width, height, _| (width, height))
            .expect("the frame after the control");
        assert_eq!((width, height), (PREVIEW_WIDTH, PREVIEW_HEIGHT));
        controls.send(Control::Preview(false)).unwrap();
        sink.check(pressure::Reading::default(), &mut report)
            .unwrap();
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
            preview_tile: None,
            capture: String::new(),
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
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
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
            preview_tile: None,
            capture: String::new(),
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
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
        };
        let mut report = |_| {};
        assert_eq!(
            sink.check(pressure::Reading::default(), &mut report)
                .unwrap(),
            None
        );
        controls.send(Control::Switch(Source::Display(1))).unwrap();
        controls.send(Control::Switch(Source::Window(2))).unwrap();
        assert_eq!(
            sink.check(pressure::Reading::default(), &mut report)
                .unwrap(),
            Some(Source::Window(2))
        );
        assert_eq!(
            sink.check(pressure::Reading::default(), &mut report)
                .unwrap(),
            None
        );
    }

    #[test]
    fn previews_a_stream_once_its_picture_shows() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            preview: vec![],
            preview_tile: None,
            capture: String::new(),
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
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
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
            preview_tile: None,
            capture: String::new(),
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
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
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
            preview_tile: None,
            capture: String::new(),
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
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
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
            high_bitrate: false,
            codec: StreamCodec::Automatic,
            capture: crate::capture::Preference::Auto,
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
            high_bitrate: false,
            codec: StreamCodec::Automatic,
            capture: crate::capture::Preference::Auto,
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

    /// The portal picks where libwebrtc captures through it: in a Wayland
    /// session, and not on X11, as on XFCE, nor where no Wayland display is
    /// named, where libwebrtc captures through X11.
    #[test]
    fn the_portal_picks_only_in_a_wayland_session() {
        assert!(wayland_session(Some("wayland"), true));
        assert!(!wayland_session(Some("x11"), false));
        assert!(!wayland_session(Some("x11"), true));
        assert!(!wayland_session(Some("tty"), false));
        assert!(!wayland_session(None, true));
        assert!(!wayland_session(Some("wayland"), false));
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
    /// On Linux, X11 lists them: a private X server, as from `env -u
    /// WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 xvfb-run`, with a window as the
    /// next test says.
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

    /// Lists what there is to share, shares the first screen and the first
    /// window for a moment each, then switches from the screen to the window.
    /// Under Xvfb, as above, it needs a window with a title, and the
    /// `WM_STATE` and `_NET_WM_WINDOW_TYPE` a window manager would give it:
    /// without them libwebrtc's X11 capturer lists no window, or asks for a
    /// property X doesn't know and Xlib ends the process.
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
        for source in sources.clone() {
            let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
            let job = Job::Share {
                source: source.clone(),
                frames: frames.path().unwrap(),
                width: PREVIEW_WIDTH,
                height: PREVIEW_HEIGHT,
                fps: 30,
                effort: Effort::Light,
                hardware: false,
                high_bitrate: false,
                codec: StreamCodec::Automatic,
                capture: crate::capture::Preference::Auto,
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
        // Switching to the window while the screen is shared.
        let [screen, window] = sources;
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let job = Job::Share {
            source: screen,
            frames: frames.path().unwrap(),
            width: PREVIEW_WIDTH,
            height: PREVIEW_HEIGHT,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
            high_bitrate: false,
            codec: StreamCodec::Automatic,
            capture: crate::capture::Preference::Auto,
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
        let shared = reports.recv_timeout(Duration::from_secs(10)).unwrap();
        let switch = Control::Switch(window.clone());
        writeln!(feed, "{}", serde_json::to_string(&switch).unwrap()).unwrap();
        // The screen's size may still settle first.
        let switched = loop {
            let report = reports.recv_timeout(Duration::from_secs(10)).unwrap();
            if report != shared {
                break report;
            }
        };
        eprintln!("{shared:?}, then {window:?}: {switched:?}");
        assert!(matches!(switched, Report::Sharing { .. }), "{switched:?}");
        assert_ne!(switched, shared);
        writeln!(feed, "stop").unwrap();
        serving.join().unwrap();
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
            high_bitrate: false,
            codec: StreamCodec::Automatic,
            capture: crate::capture::Preference::Auto,
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
            timestamp: 0,
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
            timestamp: 0,
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
            timestamp: 0,
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

    /// Whether a share whose pictures are DMA-BUFs sends one, and whether it
    /// publishes one for the window: a still desktop is answered from the picture
    /// the capture last handed over, and the window is given it once rather than
    /// a frame in ten.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_still_desktop_sends_when_asked_and_publishes_once() {
        // Nothing has been captured at all: nothing to send and nothing to show.
        assert!(!sending(false, false, true));
        // A picture that has just arrived is sent, whatever the stream wanted.
        assert!(sending(true, false, false));
        // A still desktop is sent only while the stream asks for a picture — a
        // viewer joining, a decoder that lost its place, a codec Discord chose.
        assert!(!sending(false, true, false));
        assert!(sending(false, true, true));
        // The window is published while a preview is wanted, and once: the
        // picture it was given is the one it keeps showing until another
        // replaces it, so a second pass owes it nothing.
        assert!(!publishing(false, true), "nothing shows a picture");
        assert!(!publishing(true, false), "the window has this one already");
        assert!(publishing(true, true), "a preview that has none yet");
    }

    /// A capture that answers what a test tells it to, and remembers that it was
    /// dropped: a share that stops lets go of its capture, which is the portal's
    /// session closing and the X server's redirection being undone.
    #[cfg(target_os = "linux")]
    struct Scripted {
        answers: std::vec::IntoIter<Result<Found>>,
        ending: String,
        dropped: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
        /// Whether the script has run out once already.
        done: bool,
    }

    #[cfg(target_os = "linux")]
    impl Pictures for Scripted {
        /// A picture or an ending at once, as a desktop that drew answers; a
        /// still desktop once the wait is over, as a capture says it.
        fn frame(&mut self, wait: Duration) -> Result<Found> {
            match self.answers.next() {
                Some(Ok(Found::Still)) => {
                    std::thread::sleep(wait);
                    Ok(Found::Still)
                }
                Some(answer) => answer,
                // A capture that has said everything it was told to says nothing
                // has changed — once, so a picture still waiting for its slot is
                // sent — and then asks the share to stop: a test of a share that
                // runs until it is stopped has no other way to end.
                None => {
                    std::thread::sleep(wait);
                    if self.done {
                        self.stop.store(true, Ordering::Relaxed);
                    }
                    self.done = true;
                    Ok(Found::Still)
                }
            }
        }

        fn describe(&self) -> String {
            "a test's capture".to_owned()
        }

        fn ending(&self) -> String {
            self.ending.clone()
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for Scripted {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    /// A picture of `width` by `height` in a file of its own, which is what a
    /// compositor's buffer is to a share: memory, and the layout it was drawn in.
    #[cfg(target_os = "linux")]
    fn shot(width: u32, height: u32) -> Shot {
        let file = tempfile::tempfile().unwrap();
        let stride = width * 4;
        file.set_len(u64::from(stride) * u64::from(height)).unwrap();
        Shot::held(width, height, 0, stride, Some(0), Order::Bgra, file.as_fd())
            .unwrap_or_else(|error| panic!("a picture of {width}x{height}: {error:#}"))
    }

    /// The sink a share of a window that shows no preview reports through: a
    /// frames file of its own, no stream, and nothing shown.
    #[cfg(target_os = "linux")]
    fn sink_of(width: u32, height: u32) -> Sink {
        let frames = Frames::create(width, height).unwrap();
        let path = frames.path().unwrap();
        let writer = Writer::open(&path).unwrap();
        frames.unlink();
        let (_controls, controls) = channel::channel();
        Sink {
            writer,
            preview: vec![],
            preview_tile: None,
            capture: String::new(),
            preview_shown: false,
            controls,
            server: None,
            sound: false,
            video: None,
            fps: 60,
            effort: Effort::default(),
            hardware: false,
            footage: Footage::Dmabuf,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
            diagnosis: Diagnosis::default(),
            #[cfg(nvcuda)]
            refused_pictures: 0,
        }
    }

    /// Runs a share of a scripted capture, and gives back what it reported and
    /// what it ended as.
    #[cfg(target_os = "linux")]
    fn scripted(
        answers: Vec<Result<Found>>,
        ending: &str,
        stop: Arc<AtomicBool>,
        dropped: Arc<AtomicBool>,
    ) -> (Vec<String>, Result<After>) {
        let mut reported = vec![];
        let mut report = |report: Report| {
            if let Report::Sharing { width, height } = report {
                reported.push(format!("{width}x{height}"));
            }
        };
        let opened = Opened {
            capture: Box::new(Scripted {
                answers: answers.into_iter(),
                ending: ending.to_owned(),
                dropped,
                stop: stop.clone(),
                done: false,
            }),
            rung: capture::LinuxRung::X11,
            why: "a test said so".to_owned(),
        };
        let outcome = capture_dmabufs(
            opened,
            Source::Screen,
            sink_of(1920, 1080),
            (SHARE_WIDTH, SHARE_HEIGHT),
            &stop,
            &mut report,
        );
        (reported, outcome.map(|(_sink, after)| after))
    }

    /// The screen changes size under a share: what it reports is the size it is
    /// sending, once for each, and the picture drawn at the size before it is not
    /// sent under the new one's numbers.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_resize_is_reported_at_the_size_the_picture_is() {
        let stop = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicBool::new(false));
        let (reported, outcome) = scripted(
            vec![
                Ok(Found::Picture(shot(640, 480))),
                Ok(Found::Picture(shot(640, 480))),
                Ok(Found::Resized),
                Ok(Found::Picture(shot(1280, 720))),
            ],
            "",
            stop,
            dropped.clone(),
        );
        assert!(matches!(outcome, Ok(After::Ended)), "the share ends");
        assert_eq!(
            reported,
            ["640x480", "1280x720"],
            "one report for each size, and the second only after the picture at it"
        );
        assert!(
            dropped.load(Ordering::SeqCst),
            "the capture goes with the share"
        );
    }

    /// Stopping a share ends it and lets go of its capture, which is the portal's
    /// session and the X server's redirection. Nothing spins while it waits: the
    /// loop asks the capture for a picture and the capture's own wait paces it.
    #[cfg(target_os = "linux")]
    #[test]
    fn stopping_ends_the_share_and_its_capture() {
        let stop = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicBool::new(false));
        let (_, outcome) = scripted(
            vec![Ok(Found::Picture(shot(320, 180)))],
            "",
            stop.clone(),
            dropped.clone(),
        );
        assert!(matches!(outcome, Ok(After::Ended)));
        assert!(stop.load(Ordering::Relaxed), "the script ran out");
        assert!(dropped.load(Ordering::SeqCst), "the capture is let go");
    }

    /// A desktop that draws faster than the stream's frame rate, as a busy screen
    /// on a fast monitor does.
    #[cfg(target_os = "linux")]
    struct Eager {
        until: Instant,
        pictures: u32,
        stop: Arc<AtomicBool>,
    }

    #[cfg(target_os = "linux")]
    impl Pictures for Eager {
        fn frame(&mut self, _wait: Duration) -> Result<Found> {
            if Instant::now() >= self.until {
                self.stop.store(true, Ordering::Relaxed);
                return Ok(Found::Still);
            }
            // Every other picture at another size, so each one the share sends is
            // a report of its own; and a moment apart, as frames are.
            self.pictures += 1;
            std::thread::sleep(Duration::from_millis(1));
            Ok(Found::Picture(if self.pictures.is_multiple_of(2) {
                shot(64, 48)
            } else {
                shot(32, 24)
            }))
        }

        fn describe(&self) -> String {
            "a busy desktop".to_owned()
        }

        fn ending(&self) -> String {
            String::new()
        }
    }

    /// A desktop that draws more often than the stream sends is sent at the
    /// stream's rate: the pictures that come before their slot are held, and only
    /// the newest goes out when the slot comes. Sending each as it came encoded a
    /// 30 fps share at a monitor's refresh rate, which the send queue then dropped.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_busy_desktop_is_sent_at_the_streams_rate() {
        let stop = Arc::new(AtomicBool::new(false));
        let mut sent = 0;
        let mut report = |report: Report| {
            if let Report::Sharing { .. } = report {
                sent += 1;
            }
        };
        let opened = Opened {
            capture: Box::new(Eager {
                until: Instant::now() + Duration::from_millis(1500),
                pictures: 0,
                stop: stop.clone(),
            }),
            rung: capture::LinuxRung::X11,
            why: "a test said so".to_owned(),
        };
        let mut sink = sink_of(64, 48);
        sink.fps = 20;
        let outcome = capture_dmabufs(
            opened,
            Source::Screen,
            sink,
            (SHARE_WIDTH, SHARE_HEIGHT),
            &stop,
            &mut report,
        );
        assert!(matches!(outcome, Ok((_, After::Ended))));
        // 1.5 s at 20 frames a second is thirty slots, and the desktop drew
        // hundreds of pictures in them. The band is wide because the slots are
        // wall-clock ones and this suite runs its tests side by side: a thread
        // that waits its turn sends some of them late, and what the test is for
        // is that the rate bounds the pictures sent rather than the capture.
        assert!(
            (12..=40).contains(&sent),
            "{sent} pictures sent in thirty slots"
        );
    }

    /// A capture whose share ends — the window was closed, or the person stopped
    /// it — ends the share with the capture's own sentence, rather than waiting
    /// for a picture that will never come.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_capture_that_ends_ends_the_share_with_its_reason() {
        let stop = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicBool::new(false));
        let (_, outcome) = scripted(
            vec![Ok(Found::Picture(shot(320, 180))), Ok(Found::Closed)],
            "window 0x2a was closed",
            stop,
            dropped.clone(),
        );
        let error = outcome.err().expect("the share ends");
        assert_eq!(format!("{error:#}"), "window 0x2a was closed");
        assert!(dropped.load(Ordering::SeqCst), "the capture is let go");
    }

    /// A source the card's paths can't take at all is a share that stays on the
    /// processor: a camera and a test pattern have no picture on the card, and a
    /// window, a display and the screen always do.
    #[test]
    fn a_linux_capture_takes_a_window_display_or_screen() {
        assert!(dmabuf_takes(&Source::Screen));
        assert!(dmabuf_takes(&Source::Display(0)));
        assert!(
            dmabuf_takes(&Source::Window(0x1234)),
            "a window is that window, and never the display it sits on"
        );
        assert!(!dmabuf_takes(&Source::Camera("cam".into())));
        assert!(!dmabuf_takes(&Source::TestPattern));
    }

    /// A screen's id, as Yapfast's own menu listed it, names the screen when it
    /// is the root window's own id and a monitor when it is a place: a monitor is
    /// never a window, and an id that names neither is refused rather than
    /// captured as something else.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_listed_screen_is_the_screen_it_names_or_a_monitor() {
        use capture::x11::{Display, Target};
        let monitors = vec![
            Display {
                name: "eDP-1".to_owned(),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            Display {
                name: "DP-1".to_owned(),
                x: 1920,
                y: 0,
                width: 2560,
                height: 1440,
            },
        ];
        // The root window is the screen the other windows sit on.
        assert_eq!(
            screen_target(0x1e0, 0x1e0, &monitors).ok(),
            Some(Target::Screen)
        );
        // A place is the monitor the menu would show there.
        assert_eq!(
            screen_target(1, 0x1e0, &monitors).ok(),
            Some(Target::Display(monitors[1].clone()))
        );
        assert_eq!(
            screen_target(0, 0x1e0, &monitors).ok(),
            Some(Target::Display(monitors[0].clone()))
        );
        // And an id that is neither is said so, which is a share that falls back
        // rather than one that captures a monitor nobody chose.
        let error = screen_target(7, 0x1e0, &monitors).unwrap_err();
        assert!(
            format!("{error:#}").contains("no monitor at place 7"),
            "{error:#}"
        );
    }
}
