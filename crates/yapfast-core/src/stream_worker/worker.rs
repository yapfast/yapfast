//! `stream_worker`'s worker.

use super::*;

/// Yapfast's handle on a running worker. Dropping it stops the worker.
pub struct Worker {
    /// Shared with the thread reading its reports, which waits on it when
    /// it ends without saying so.
    pub(crate) child: Arc<Mutex<Child>>,
    pub(crate) stdin: Option<ChildStdin>,
    /// The place this worker takes among those decoding, given back when it
    /// stops. A worker that decodes no picture has none.
    pub(crate) _place: Option<Place>,
}

/// The workers this Yapfast has decoding pictures: streams watched and videos
/// played. Each is a process of its own, so the number of them is only known
/// here, in the Yapfast that started them, and it's from that number and the
/// machine's cores that a new worker's decoder learns how much it may take.
pub(crate) static DECODERS: AtomicUsize = AtomicUsize::new(0);

/// A worker's place among those decoding, given back when the worker goes.
pub(crate) struct Place {
    /// The threads its decoder may use.
    pub(crate) threads: u32,
}

impl Place {
    /// Takes a place for a worker starting on a machine with `cores`, and
    /// works out the share of its decoder threads that place leaves.
    pub(crate) fn claim(cores: usize) -> Self {
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

/// Hands `report` to the UI, and says what's left to do with it: a status a
/// later one replaces is dropped when there's no room, which keeps the
/// worker's reader moving and its child from blocking on a full standard
/// output. Everything else waits for room, however long the UI takes over
/// it, so the end of a job is never the report that goes missing.
pub(crate) fn deliver(reports: &mpsc::Sender<Report>, report: Report) -> Delivery {
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
pub(crate) enum Delivery {
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
            ceiling: quality.ceiling.bits_per_pixel(),
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

    pub(crate) fn control(&mut self, control: &Control) -> Result<()> {
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
    pub(crate) fn start(
        job: &Job,
        frames: Option<&Frames>,
    ) -> Result<(Self, mpsc::Receiver<Report>)> {
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
pub(crate) fn running_yapfast() -> Result<PathBuf> {
    if cfg!(target_os = "linux") {
        return Ok(PathBuf::from("/proc/self/exe"));
    }
    std::env::current_exe().context("could not find Yapfast to start streaming")
}

/// What a worker's reports last said about its job, to log if it ends
/// without saying why.
pub(crate) struct Last {
    pub(crate) job: &'static str,
    /// The encoder's engine and codec, once a share reports how it goes out.
    pub(crate) encoder: Option<(String, String)>,
    /// The size reported, or the size the job fits in until one is.
    pub(crate) size: (u32, u32),
    pub(crate) reported: bool,
}

impl Last {
    pub(crate) fn new(job: &Job) -> Self {
        Self {
            job: job.kind(),
            encoder: None,
            size: job.bounds(),
            reported: false,
        }
    }

    pub(crate) fn saw(&mut self, report: &Report) {
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
    pub(crate) fn said(&self, exit: &str) -> String {
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
    pub(crate) fn log(&self, exit: String) {
        tracing::warn!("{}", self.said(&exit));
    }
}

/// How the worker ended, once its output closed: it has exited by then, or
/// is about to.
pub(crate) fn exited(child: &Mutex<Child>) -> String {
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
pub(crate) fn how(status: ExitStatus) -> String {
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
pub(crate) fn signal_name(signal: i32) -> Option<&'static str> {
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
