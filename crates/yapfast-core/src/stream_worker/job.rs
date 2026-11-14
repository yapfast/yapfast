//! `stream_worker`'s job.

use super::*;

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
        /// What the stream may take from the uplink, in bits per pixel a frame:
        /// the share menu's quality choice, 0.03, 0.07 or 0.12; see
        /// [`crate::voice::Ceiling`]. It travels with the job for the same
        /// reason the capture's preference does: the worker is a process of its
        /// own, and it is where the stream is declared.
        #[serde(default = "worth_a_pixel")]
        ceiling: f64,
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
    pub(crate) fn decodes(&self) -> bool {
        matches!(self, Job::Watch { .. } | Job::Play { .. })
    }

    /// What the job is, for the log.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Share { .. } => "share",
            Self::Watch { .. } => "watch",
            Self::Play { .. } => "play",
            Self::Glance { .. } => "glance",
        }
    }

    /// The size its pictures fit in.
    pub(crate) fn bounds(&self) -> (u32, u32) {
        match *self {
            Self::Share { width, height, .. }
            | Self::Watch { width, height, .. }
            | Self::Play { width, height, .. }
            | Self::Glance { width, height, .. } => (width, height),
        }
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

pub(crate) fn as_it_comes() -> f32 {
    1.0
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

pub(crate) fn serve(mut input: Box<dyn BufRead + Send>, report: &mut dyn FnMut(Report)) {
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
            ceiling,
            codec,
            capture: preference,
        } => {
            crate::capture::set(preference);
            // The stream is declared from here, so the ceiling comes with the
            // job: op 12 goes out once, with the rate this process was told to
            // allow, and the controller is seeded from it.
            stream_connection::set_ceiling(ceiling);
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
