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
//! shares another screen, window or camera on the same stream. A `stop` line, or
//! closing its input, ends the job. Frames go to the UI through [`crate::frames`].

use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
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
    stream_connection::{self, Link, StreamReceiver, StreamSender},
};

use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoBuffer},
};

use crate::{
    decode::Decoder,
    encode::{self, Effort, Encoder},
    frames::{Frames, Writer},
    stream_audio,
    voice::{Resolution, StreamQuality, StreamStats},
};

/// The argument that starts Yapfast as a stream worker.
pub const ARGUMENT: &str = "--stream-worker";

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
    /// Play a watched stream's or a video's sound this loud.
    Volume(f32),
    /// Play the video, from the start once it has ended.
    Play,
    Pause,
    /// Move to this many milliseconds into the video.
    Seek(u64),
    /// Draw the video's pictures to fit this.
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

/// Runs this process as a stream worker until its job ends.
pub fn run() -> Result<()> {
    // The stream connection uses TLS; nothing else here has started it.
    let _ = crate::init_tls();
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
                controls: received,
                server: None,
                sound: source != Source::TestPattern,
                video: None,
                fps,
                effort,
                hardware,
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

struct Video {
    sender: StreamSender,
    encoder: Encoder,
    /// Other applications' sound, while sharing a screen.
    _sound: Option<stream_audio::Capture>,
    /// When the stream last reported how it goes out, and the bytes it had
    /// sent then.
    measured: (Instant, u64),
}

impl Video {
    /// How the stream goes out, with the kbit/s sent since it last said.
    fn sending(&mut self, fps: u32) -> StreamStats {
        let sent = self.sender.sent();
        let (since, before) = self.measured;
        let bits = sent.video_bytes.saturating_sub(before) * 8;
        let milliseconds = since.elapsed().as_millis().max(1) as u64;
        self.measured = (Instant::now(), sent.video_bytes);
        let (width, height) = self.encoder.size();
        StreamStats {
            engine: self.encoder.engine().to_owned(),
            codec: self.encoder.codec().name().to_owned(),
            width,
            height,
            fps,
            target_kbps: self.encoder.bitrate(),
            sent_kbps: (bits / milliseconds) as u32,
            keyframe_requests: sent.keyframe_requests,
            resent: sent.resent,
        }
    }
}

/// The codecs only a GPU encodes, most wanted first, offered before H264
/// where the GPU's encoder for them opens.
const GPU_CODECS: [Codec; 2] = [Codec::Av1, Codec::H265];

fn log_encoder(encoder: &Encoder) {
    let (width, height) = encoder.size();
    tracing::info!(
        engine = encoder.engine(),
        codec = ?encoder.codec(),
        width,
        height,
        "encoding the stream"
    );
}

impl Sink {
    /// Takes a frame at the size it's sent: the preview shows it, scaled
    /// down to fit, and Discord gets it once the stream has a server.
    fn frame(&mut self, picture: &mut I420Buffer) -> Result<()> {
        let (width, height) = (picture.width(), picture.height());
        let preview = fit(width, height, PREVIEW_WIDTH, PREVIEW_HEIGHT);
        let mut smaller =
            (preview != (width, height)).then(|| picture.scale(preview.0 as i32, preview.1 as i32));
        let shown = match smaller.as_mut() {
            Some(smaller) => smaller,
            None => &mut *picture,
        };
        self.writer
            .write(preview.0, preview.1, |pixels| to_rgba(shown, pixels));
        if self.preview_due.is_some_and(|due| due <= Instant::now()) && !is_black(shown) {
            match thumbnail(shown) {
                Ok(thumbnail) => self.thumbnail = Some(thumbnail),
                Err(error) => {
                    tracing::warn!(error = format!("{error:#}"), "could not make a preview")
                }
            }
            self.preview_due = Some(Instant::now() + THUMBNAIL_EVERY);
        }
        if let Some(server) = self.server.take() {
            let (offers, encoder) = self.offers(width, height)?;
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
                encoder,
                _sound: sound,
                measured: (Instant::now(), 0),
            });
        }
        let Some(video) = self.video.as_ref() else {
            return Ok(());
        };
        // Discord's choice of codec changes as viewers come and go.
        let codec = video.sender.codec().unwrap_or(video.encoder.codec());
        let resized = video.encoder.size() != (width, height);
        if resized || codec != video.encoder.codec() {
            let opened = self.encoder(codec, width, height);
            let video = self.video.as_mut().expect("checked above");
            match opened {
                Ok(encoder) => {
                    video.encoder = encoder;
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
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
        }
        let Some(video) = self.video.as_mut() else {
            return Ok(());
        };
        let wanted = (video.sender.target() / 1000) as u32;
        let aimed = video.encoder.bitrate();
        // Not on every wobble, which would churn the rate control.
        if wanted.abs_diff(aimed) > aimed / 10 {
            match video.encoder.set_bitrate(wanted) {
                Ok(()) => tracing::debug!(kbps = wanted, "the encoder follows the stream's target"),
                Err(error) => {
                    tracing::warn!(error = format!("{error:#}"), "could not change the bitrate")
                }
            }
        }
        let pts = self.started.elapsed().as_micros() as u64 * encode::CLOCK / 1_000_000;
        let keyframe = video.sender.take_keyframe_request();
        let codec = video.encoder.codec();
        match video.encoder.encode_i420(picture, pts, keyframe) {
            Ok(Some(frame)) => video.sender.send(stream_connection::Frame {
                data: frame.data.to_vec(),
                codec,
                timestamp: pts as u32,
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

    /// The codecs to offer Discord, most wanted first, and an encoder for
    /// the first: AV1, then H265, where the GPU's encoder for each opens,
    /// then H264. Discord streams the first every viewer decodes. Each is
    /// opened to see that it opens, and only the first kept.
    fn offers(&self, width: u32, height: u32) -> Result<(Vec<Codec>, Encoder)> {
        let mut offers = vec![];
        let mut first = None;
        for codec in GPU_CODECS.iter().copied().filter(|_| self.hardware) {
            if let Ok(encoder) = self.open(codec, width, height) {
                offers.push(codec);
                first.get_or_insert(encoder);
            }
        }
        offers.push(stream_connection::SEND_CODEC);
        let encoder = match first {
            Some(encoder) => encoder,
            None => self.open(stream_connection::SEND_CODEC, width, height)?,
        };
        log_encoder(&encoder);
        Ok((offers, encoder))
    }

    /// An encoder for the stream in `codec`, as [`Self::open`] opens it.
    fn encoder(&self, codec: Codec, width: u32, height: u32) -> Result<Encoder> {
        let encoder = self.open(codec, width, height)?;
        log_encoder(&encoder);
        Ok(encoder)
    }

    /// An encoder in `codec`, at the stream's target, or what a stream of
    /// `height` lines starts at before it has one.
    fn open(&self, codec: Codec, width: u32, height: u32) -> Result<Encoder> {
        let target = self.video.as_ref().map_or_else(
            || congestion::start(stream_connection::max_bitrate(height).into()),
            |video| video.sender.target(),
        );
        Encoder::new(
            codec,
            width,
            height,
            self.fps,
            (target / 1000) as u32,
            self.effort,
            self.hardware,
        )
    }

    /// Takes Yapfast's controls, reports the stream starting, and fails once
    /// its connection ended. Gives the source a [`Control::Switch`] asked for.
    fn check(&mut self, report: &mut dyn FnMut(Report)) -> Result<Option<Source>> {
        let mut switch = None;
        while let Ok(control) = self.controls.try_recv() {
            match control {
                Control::Send(server) => self.server = Some(server),
                Control::Switch(source) => switch = Some(source),
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

/// Whether a picture is black, as a screen is before what's shared draws:
/// Discord's client waits for one that isn't before sending a preview.
fn is_black(picture: &I420Buffer) -> bool {
    let (y, _, _) = picture.data();
    // Black's luma is 16; some of a picture of anything is brighter.
    y.iter().step_by(7).all(|&luma| luma <= 24)
}

/// A preview of a picture as Discord's client makes one: scaled down to fit
/// 512 by 288, a JPEG as a data URI.
fn thumbnail(picture: &mut I420Buffer) -> Result<String> {
    use base64::Engine;
    use image::{ExtendedColorType, codecs::jpeg::JpegEncoder};
    let (width, height) = fit(
        picture.width(),
        picture.height(),
        THUMBNAIL_WIDTH,
        THUMBNAIL_HEIGHT,
    );
    let small = picture.scale(width as i32, height as i32);
    let mut rgba = vec![0; (width * height * 4) as usize];
    to_rgba(&small, &mut rgba);
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

/// Shows someone's stream: whole frames from the connection, decoded, then
/// scaled down if they don't fit, and plays its sound.
fn watch(
    server: StreamServer,
    mut writer: Writer,
    (max_width, max_height): (u32, u32),
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
    let mut decoder: Option<Decoder> = None;
    // A frame that doesn't fit scales through this, a picture of the size
    // shown; one that does goes straight into the shared frame.
    let mut scratch = vec![];
    let mut shown = false;
    let mut failures = 0u64;
    while !stop.load(Ordering::Relaxed) {
        while let Ok(control) = sound.controls.try_recv() {
            match control {
                Control::Hear(hearing) => sound.hearing.store(hearing, Ordering::Relaxed),
                Control::Volume(volume) => sound.volume.set(volume),
                _ => {}
            }
        }
        if let Link::Ended(reason) = receiver.link() {
            bail!("Lost the stream: {reason}");
        }
        let Some(frame) = receiver.next(Duration::from_millis(100)) else {
            continue;
        };
        if decoder
            .as_ref()
            .is_none_or(|decoder| decoder.codec() != frame.codec)
        {
            decoder = Some(Decoder::new(frame.codec)?);
        }
        let Some(decoder) = decoder.as_mut() else {
            continue;
        };
        let picture = match decoder.decode(&frame.data) {
            Ok(Some(picture)) => picture,
            Ok(None) => continue,
            Err(error) => {
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
        let (width, height) = (picture.width, picture.height);
        if writer.fits(width, height) && width <= max_width && height <= max_height {
            writer.write(width, height, |pixels| picture.to_rgba(pixels));
        } else {
            let (width, height) = fit(width, height, max_width, max_height);
            writer.write(width, height, |pixels| {
                picture.i420().to_rgba(&mut scratch, pixels, width, height)
            });
        }
        if !shown {
            shown = true;
            tracing::info!(width, height, codec = ?frame.codec, "showing the stream");
            report(Report::Streaming);
        }
    }
    Ok(())
}

/// Sleeps until the next frame is due, or returns false once told to stop.
fn next_frame(started: Instant, frame: u64, fps: u32, stop: &AtomicBool) -> bool {
    let due = started + Duration::from_secs(frame) / fps;
    if let Some(wait) = due.checked_duration_since(Instant::now()) {
        std::thread::sleep(wait);
    }
    !stop.load(Ordering::Relaxed)
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
    let mut frame = 0u64;
    let mut rgba = vec![0; (width * height * 4) as usize];
    let mut picture = I420Buffer::new(width, height);
    while next_frame(started, frame, sink.fps, stop) {
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
        sink.frame(&mut picture)?;
        // Colour bars are all it shares.
        let _switch = sink.check(report)?;
        frame += 1;
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
    let fps = sink.fps;
    let sink = Arc::new(Mutex::new(sink));
    let mut capture = Capture::start(source, &sink, max_width, max_height)?;
    // A screen, window or camera to share in its place, until it gives a
    // picture.
    let mut switching: Option<Capture> = None;
    let started = Instant::now();
    let mut frame = 0u64;
    let mut reported = None;
    while next_frame(started, frame, fps, stop) {
        run_glib();
        capture.capture_frame();
        if let Some(next) = &mut switching {
            next.capture_frame();
        }
        frame += 1;
        let seen = capture.seen();
        if let Some(failure) = &seen.failed {
            bail!(if reported.is_none() && portal_picks() {
                "Screen sharing was cancelled, or this desktop has no screen sharing dialog."
                    .to_owned()
            } else {
                failure.clone()
            });
        }
        if seen.size != reported
            && let Some((width, height)) = seen.size
        {
            reported = seen.size;
            report(Report::Sharing { width, height });
        }
        drop(seen);
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
            CaptureError, DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
        };
        let portal = portal_picks();
        let kind = match source {
            // The system's screen sharing dialog picks a screen or window.
            // libwebrtc has the generic capturer only on Linux and macOS, and
            // `portal_picks` is false elsewhere.
            #[cfg(target_os = "linux")]
            _ if portal => DesktopCaptureSourceType::Generic,
            Source::Window(_) => DesktopCaptureSourceType::Window,
            _ => DesktopCaptureSourceType::Screen,
        };
        let mut options = DesktopCapturerOptions::new(kind);
        options.set_include_cursor(true);
        let mut capturer = DesktopCapturer::new(options)
            .context("Screen sharing isn't available on this system.")?;
        let captured = Arc::new(Mutex::new(Captured::default()));
        let (into, seen) = (sink.clone(), captured.clone());
        let source = if portal {
            None
        } else {
            let mut listed = capturer.get_source_list().into_iter();
            let found = match source {
                Source::Display(id) | Source::Window(id) => listed.find(|listed| listed.id() == id),
                _ => listed.next(),
            };
            Some(found.context("That screen or window is gone.")?)
        };
        capturer.start_capture(source, move |result| {
            let mut seen = seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match result {
                Ok(frame) => {
                    let mut picture = picture_of(&frame);
                    let (width, height) = (picture.width(), picture.height());
                    let size = fit(width, height, max_width, max_height);
                    if size != (width, height) {
                        picture = picture.scale(size.0 as i32, size.1 as i32);
                    }
                    let taken = into
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .frame(&mut picture);
                    match taken {
                        Ok(()) => seen.size = Some(size),
                        Err(error) => seen.failed = Some(format!("{error:#}")),
                    }
                }
                // While the dialog is open, or between frames.
                Err(CaptureError::Temporary) => {}
                Err(CaptureError::Permanent) => {
                    seen.failed
                        .get_or_insert_with(|| "Screen sharing stopped.".into());
                }
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
        let camera = crate::camera::Camera::open_at(
            Some(id),
            (max_width, max_height, fps),
            Box::new(move |frame| {
                let size = (frame.width, frame.height);
                if kept
                    .as_ref()
                    .is_none_or(|picture| (picture.width(), picture.height()) != size)
                {
                    kept = Some(I420Buffer::new(frame.width, frame.height));
                }
                let picture = kept.as_mut().expect("made above");
                copy_planes(frame, picture);
                // The camera fits the stream already; an odd size, which
                // codecs can't take, loses a row or a column.
                let fitted = fit(size.0, size.1, max_width, max_height);
                let taken = if fitted == size {
                    into.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .frame(picture)
                } else {
                    let mut scaled = picture.scale(fitted.0 as i32, fitted.1 as i32);
                    into.lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .frame(&mut scaled)
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

/// A captured frame as an I420 picture of its size.
fn picture_of(frame: &livekit::webrtc::desktop_capturer::DesktopFrame) -> I420Buffer {
    let (width, height) = (frame.width().max(1) as u32, frame.height().max(1) as u32);
    let mut picture = I420Buffer::new(width, height);
    let (y_stride, u_stride, v_stride) = picture.strides();
    let (y, u, v) = picture.data_mut();
    // libyuv's ARGB is B, G, R, A in memory, as the capturer's frames are.
    yuv_helper::argb_to_i420(
        frame.data(),
        frame.stride(),
        y,
        y_stride,
        u,
        u_stride,
        v,
        v_stride,
        width as i32,
        height as i32,
    );
    picture
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
    // The last picture taken, and whether capture stopped for good.
    let taken = Arc::new(Mutex::new((None::<I420Buffer>, false)));
    let into = taken.clone();
    capturer.start_capture(Some(source), move |result| {
        let mut taken = into.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match result {
            Ok(frame) => taken.0 = Some(picture_of(&frame)),
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
        let lit = taken.0.as_ref().is_some_and(|picture| !is_black(picture));
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
    /// Shared with the thread reading its reports, which waits on it when
    /// it ends without saying so.
    child: Arc<Mutex<Child>>,
    stdin: Option<ChildStdin>,
}

impl Worker {
    /// Starts sharing `source`, with frames for the UI in a new [`Frames`].
    pub fn share(
        source: Source,
        quality: StreamQuality,
    ) -> Result<(Self, Frames, mpsc::UnboundedReceiver<Report>)> {
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
    ) -> Result<(Self, mpsc::UnboundedReceiver<Report>)> {
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
    ) -> Result<(Self, Frames, mpsc::UnboundedReceiver<Report>)> {
        let frames = Frames::create(WATCH_WIDTH, WATCH_HEIGHT)?;
        let job = Job::Watch {
            server: server.clone(),
            frames: frames.path().context("the video frames file has no name")?,
            width: WATCH_WIDTH,
            height: WATCH_HEIGHT,
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
    ) -> Result<(Self, Frames, mpsc::UnboundedReceiver<Report>)> {
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

    fn start(job: &Job) -> Result<(Self, mpsc::UnboundedReceiver<Report>)> {
        // Tests run from their own binary, so they name Yapfast's.
        let program = match std::env::var_os("YAPFAST_STREAM_WORKER") {
            Some(program) => PathBuf::from(program),
            None => running_yapfast()?,
        };
        let mut child = Command::new(program)
            .arg(ARGUMENT)
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
        let (reports, receiver) = mpsc::unbounded_channel();
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
                            if reports.send(report).is_err() {
                                return;
                            }
                        }
                        Err(error) => tracing::warn!(%error, "unknown stream worker report"),
                    }
                }
                if !ended {
                    last.log(exited(&waited));
                    let _ = reports.send(Report::Ended {
                        error: Some(unexpected.into()),
                    });
                }
            })
            .context("could not start the stream worker")?;
        Ok((Self { child, stdin }, receiver))
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

    /// Warns that the worker ended without a [`Report::Ended`], and how.
    /// What it wrote to standard error before that is already in the log,
    /// through [`crate::logging::for_worker`].
    fn log(&self, exit: String) {
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
        tracing::warn!(
            job = self.job,
            engine,
            codec,
            size,
            exit,
            "the stream worker ended without saying why"
        );
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
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        // The worker checks for the stop between frames.
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if !matches!(child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        tracing::warn!("the stream worker didn't stop; ending it");
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A worker that dies without a [`Report::Ended`] logs a warning with
    /// how it ended and the job it had, and the UI still gets the popup's
    /// error.
    #[cfg(unix)]
    #[test]
    fn a_worker_that_dies_says_how() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let died = |script: &str| {
            let worker = directory.path().join("worker");
            std::fs::write(&worker, format!("#!/bin/sh\nread job\n{script}\n")).unwrap();
            std::fs::set_permissions(&worker, std::fs::Permissions::from_mode(0o755)).unwrap();
            let logged = Arc::new(Mutex::new(Vec::new()));
            let writer = logged.clone();
            let logger = tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(move || Logged(writer.clone()))
                .finish();
            let _logging = tracing::subscriber::set_default(logger);
            // Only this test starts a worker.
            unsafe { std::env::set_var("YAPFAST_STREAM_WORKER", &worker) };
            let (worker, mut reports) = Worker::start(&Job::Glance {
                screens: vec![],
                width: 320,
                height: 180,
            })
            .unwrap();
            assert_eq!(
                reports.blocking_recv(),
                Some(Report::Ended {
                    error: Some("Screen sharing stopped unexpectedly.".into())
                })
            );
            drop(worker);
            String::from_utf8(logged.lock().unwrap().clone()).unwrap()
        };

        // No core file: the shell's would be in the way.
        let log = died("ulimit -c 0; kill -SEGV $$");
        assert!(log.contains("WARN"), "{log}");
        assert!(
            log.contains("the stream worker ended without saying why"),
            "{log}"
        );
        assert!(log.contains("exit=\"signal=11 (SIGSEGV"), "{log}");
        assert!(log.contains("job=\"glance\""), "{log}");
        assert!(log.contains("size=\"up to 320x180\""), "{log}");
        assert!(log.contains("engine=\"unknown\""), "{log}");

        let log = died("exit 3");
        assert!(log.contains("exit=\"code=3\""), "{log}");
    }

    /// Collects a test's log.
    #[cfg(unix)]
    struct Logged(Arc<Mutex<Vec<u8>>>);

    #[cfg(unix)]
    impl Write for Logged {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
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
    fn fits_frames_with_even_sizes() {
        assert_eq!(fit(1920, 1080, 1280, 720), (1280, 720));
        assert_eq!(fit(3440, 1440, 1280, 720), (1280, 534));
        assert_eq!(fit(801, 601, 1280, 720), (800, 600), "never larger");
    }

    /// With the GPU, AV1 and H265 before H264 where they open, and an
    /// encoder for the first; without it, H264 alone, on x264.
    #[test]
    fn offers_the_gpus_codecs_first() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: true,
            started: Instant::now(),
            streaming: false,
            preview_due: None,
            thumbnail: None,
            lost: None,
        };
        let (offers, encoder) = sink.offers(320, 180).unwrap();
        eprintln!("offering {offers:?}, encoding with {}", encoder.engine());
        assert_eq!(offers.last(), Some(&Codec::H264));
        let mut order = [Codec::Av1, Codec::H265, Codec::H264].iter();
        assert!(
            offers.iter().all(|codec| order.any(|next| next == codec)),
            "in order"
        );
        assert_eq!(encoder.codec(), offers[0]);
        sink.hardware = false;
        let (offers, encoder) = sink.offers(320, 180).unwrap();
        assert_eq!((offers, encoder.engine()), (vec![Codec::H264], "x264"));
    }

    #[test]
    fn previews_what_it_sends_at_a_smaller_size() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (_controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
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
        sink.frame(&mut picture).unwrap();
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

    #[test]
    fn switches_to_the_last_source_asked_for() {
        let frames = Frames::create(PREVIEW_WIDTH, PREVIEW_HEIGHT).unwrap();
        let (controls, received) = channel::channel();
        let mut sink = Sink {
            writer: Writer::open(&frames.path().unwrap()).unwrap(),
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
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
            controls: received,
            server: None,
            sound: false,
            video: None,
            fps: 30,
            effort: Effort::Light,
            hardware: false,
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
        sink.frame(&mut picture).unwrap();
        assert_eq!(sink.thumbnail, None, "not while the screen is black");
        picture.data_mut().0.fill(126);
        sink.frame(&mut picture).unwrap();
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
        sink.frame(&mut picture).unwrap();
        assert_eq!(sink.thumbnail, None, "the next one comes minutes later");
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
}
