//! Records the private test display at 60 frames a second, for
//! `scripts/ui-session do 'record start NAME'`.
//!
//!   record_host > frames
//!
//! It asks Weston for the output's framebuffer through weston_capture_v1,
//! which Weston offers with `--debug`, into shared-memory buffers, and on a
//! steady 60 Hz clock writes the newest captured frame to standard output.
//! The first line is `WIDTH HEIGHT PIXEL_FORMAT`, the format as ffmpeg names
//! it, such as bgra; raw frames follow, top row last, as the GL renderer
//! reads them, so a player needs ffmpeg's vflip. A tick with no new capture
//! writes the last frame again, so the video lasts as long as the recording
//! did. SIGINT or SIGTERM ends it; it then reports on standard error how many
//! frames it captured, wrote and repeated.
//!
//! Each capture makes Weston repaint once, so a still window captures as
//! often as a busy one; a frame counts as changed when its pixels differ
//! from the one written before it. Weston's headless backend repaints at
//! most every 17 ms or so, its 16 ms frame timer and a least 1 ms delay
//! together, so about 58.5 captures a second fill 60 ticks: a minute holds
//! about 90 repeats even with every repaint captured. That needs a repaint
//! window of a whole frame, which scripts/ui-session gives Weston; with its
//! default 7 ms, Weston repaints a busy display about 39 times a second.
//!
//! With `--rows X0 X1` it records no video: it captures every repaint,
//! asking for the next capture a refresh after the one before (the output's
//! mode says how long that is), and writes for each
//! the time it completed and, for every row of the framebuffer, the sum of
//! its pixels' channels between columns X0 and X1, for
//! testing/frame_steps.py to tell from them how far a list moved each
//! frame. The first line is `rows WIDTH HEIGHT X0 X1`; each frame is then a
//! little-endian u64 of CLOCK_MONOTONIC nanoseconds, the clock of Weston's
//! timeline, and HEIGHT u32s, top row last as above.
//!
//! It runs only on the isolated test display, as scripts/framework-input does.

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = linux::run() {
        eprintln!("record_host: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
#[allow(
    clippy::all,
    unused_imports,
    non_upper_case_globals,
    non_camel_case_types,
    reason = "wayland-scanner's generated code"
)]
mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("../protocols/weston-output-capture.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("../protocols/weston-output-capture.xml");
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        io::Write,
        mem::ManuallyDrop,
        os::fd::{AsFd, AsRawFd, FromRawFd},
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };

    use wayland_client::{
        Connection, Dispatch, EventQueue, QueueHandle,
        globals::{GlobalListContents, registry_queue_init},
        protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool},
    };

    use crate::protocol::{weston_capture_source_v1 as source, weston_capture_v1 as capture};

    const RATE: u64 = 60;
    const FRAME: Duration = Duration::from_nanos(1_000_000_000 / RATE);
    /// Buffers: the newest frame, kept to write again, and up to two
    /// captures in flight: one Weston reads back, and one for its next repaint.
    const SLOTS: usize = 3;
    /// Captures that fail one after another before the recording gives up.
    const FAILURES: u32 = 30;

    static STOP: AtomicBool = AtomicBool::new(false);

    extern "C" fn stop(_: libc::c_int) {
        STOP.store(true, Ordering::Relaxed);
    }

    #[derive(Clone, Copy, Default, PartialEq)]
    enum Slot {
        #[default]
        Idle,
        /// Asked for, as the capture numbered so, at that time.
        Capturing(u64, Instant),
        /// Captured, with a hash of its pixels.
        Done(u64, u64),
    }

    #[derive(Default)]
    struct State {
        /// `--rows`: the columns summed for each row, and the frames summed
        /// so far, not yet written.
        rows: Option<(usize, usize)>,
        summed: Vec<(u64, Vec<u32>)>,
        /// Set as the recording ends: no more captures are asked for.
        stopping: bool,
        /// The output's refresh period, as its mode says.
        refresh: Option<Duration>,
        format: Option<u32>,
        size: Option<(i32, i32)>,
        slots: [Slot; SLOTS],
        sources: Vec<source::WestonCaptureSourceV1>,
        buffers: Vec<wl_buffer::WlBuffer>,
        /// Captures asked for so far, which numbers the next.
        asked: u64,
        /// Set when Weston asks for buffers of a new size or format.
        retried: bool,
        failed_in_a_row: u32,
        failed: u64,
        captured: u64,
        /// The pool's mapping, to hash a frame as it completes.
        pixels: Option<memmap2::MmapMut>,
        frame_bytes: usize,
    }

    pub fn run() -> Result<(), String> {
        require_private_display()?;
        // SAFETY: the handler only stores to an atomic, and the pipe size is
        // a plain request on standard output.
        unsafe {
            libc::signal(libc::SIGINT, stop as *const () as libc::sighandler_t);
            libc::signal(libc::SIGTERM, stop as *const () as libc::sighandler_t);
            // Fewer, larger writes into ffmpeg; a file or terminal refuses it.
            libc::fcntl(1, libc::F_SETPIPE_SZ, 1 << 20);
        }
        let connection = Connection::connect_to_env().map_err(|error| error.to_string())?;
        let (globals, mut queue) =
            registry_queue_init::<State>(&connection).map_err(|error| error.to_string())?;
        let handle = queue.handle();
        let output: wl_output::WlOutput = globals
            .bind(&handle, 1..=1, ())
            .map_err(|error| format!("no wl_output: {error}"))?;
        let shm: wl_shm::WlShm = globals
            .bind(&handle, 1..=1, ())
            .map_err(|error| format!("no wl_shm: {error}"))?;
        let capturer: capture::WestonCaptureV1 = globals
            .bind(&handle, 1..=1, ())
            .map_err(|_| "no weston_capture_v1; Weston offers it only with --debug".to_string())?;
        let rows = rows_argument()?;
        let mut state = State {
            rows,
            sources: (0..SLOTS)
                .map(|slot| capturer.create(&output, capture::Source::Framebuffer, &handle, slot))
                .collect(),
            ..State::default()
        };
        queue
            .roundtrip(&mut state)
            .map_err(|error| error.to_string())?;
        let (Some(format), Some((width, height))) = (state.format, state.size) else {
            return Err("Weston gave the framebuffer no format or size".into());
        };
        let pixel_format = ffmpeg_format(format)
            .ok_or_else(|| format!("no ffmpeg name for DRM format {format:#x}"))?;
        let shm_format = match format {
            // wl_shm numbers its two oldest formats apart from DRM's codes.
            ARGB8888 => wl_shm::Format::Argb8888,
            XRGB8888 => wl_shm::Format::Xrgb8888,
            other => wl_shm::Format::try_from(other)
                .map_err(|_| format!("wl_shm has no format {other:#x}"))?,
        };
        let stride = width * 4;
        state.frame_bytes = (stride * height) as usize;
        let file = tempfile::tempfile_in("/dev/shm").map_err(|error| error.to_string())?;
        file.set_len((state.frame_bytes * SLOTS) as u64)
            .map_err(|error| error.to_string())?;
        // SAFETY: the file is this process's own, in /dev/shm, and nothing
        // else resizes it while mapped.
        state.pixels =
            Some(unsafe { memmap2::MmapMut::map_mut(&file) }.map_err(|error| error.to_string())?);
        let pool = shm.create_pool(
            file.as_fd(),
            (state.frame_bytes * SLOTS) as i32,
            &handle,
            (),
        );
        state.buffers = (0..SLOTS)
            .map(|slot| {
                pool.create_buffer(
                    (slot * state.frame_bytes) as i32,
                    width,
                    height,
                    stride,
                    shm_format,
                    &handle,
                    (),
                )
            })
            .collect();
        // Standard output as a file, unbuffered: frames are megabytes.
        // SAFETY: descriptor 1 is open for the process's life, and
        // ManuallyDrop never closes it.
        let mut out = ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(1) });
        if let Some((from, to)) = state.rows {
            if to > width as usize || from >= to {
                return Err(format!("--rows {from} {to} is not within {width} columns"));
            }
            writeln!(out, "rows {width} {height} {from} {to}")
                .map_err(|error| error.to_string())?;
            let ending = write_rows(&connection, &mut queue, &mut state, &mut out);
            for source in state.sources.drain(..) {
                source.destroy();
            }
            for buffer in state.buffers.drain(..) {
                buffer.destroy();
            }
            pool.destroy();
            capturer.destroy();
            let _ = connection.flush();
            return ending;
        }
        writeln!(out, "{width} {height} {pixel_format}").map_err(|error| error.to_string())?;

        // The clock starts with the first frame.
        while newest(&state.slots).is_none() {
            if STOP.load(Ordering::Relaxed) {
                return Ok(());
            }
            if state.failed_in_a_row >= FAILURES || state.retried {
                return Err("Weston would not capture the display".into());
            }
            wait(&connection, &mut queue, &mut state, Instant::now() + FRAME)?;
        }

        // Weston repaints, and so captures, about every frame on its own
        // clock. Ticks start half a frame after the first capture, so one a
        // little early or late still makes its tick.
        let began = Instant::now();
        let start = began + FRAME / 2;
        let mut tick: u64 = 0;
        let mut written_capture = None;
        let mut written_hash = None;
        let (mut repeated, mut changed, mut late) = (0u64, 0u64, 0u64);
        // Changed frames in each second of the video.
        let mut changed_per_second: Vec<u32> = Vec::new();
        let mut ending = None;
        while !STOP.load(Ordering::Relaxed) {
            let due = start + Duration::from_nanos(tick * 1_000_000_000 / RATE);
            wait(&connection, &mut queue, &mut state, due)?;
            if STOP.load(Ordering::Relaxed) {
                break;
            }
            if Instant::now() > due + FRAME {
                late += 1;
            }
            let Some((slot, asked, hash)) = newest(&state.slots) else {
                return Err("no frame to write".into());
            };
            if written_capture == Some(asked) {
                repeated += 1;
            }
            written_capture = Some(asked);
            let second = (tick / RATE) as usize;
            changed_per_second.resize(second + 1, 0);
            if written_hash != Some(hash) {
                changed += 1;
                changed_per_second[second] += 1;
            }
            written_hash = Some(hash);
            let pixels = state.pixels.as_ref().expect("mapped");
            let frame = &pixels[slot * state.frame_bytes..(slot + 1) * state.frame_bytes];
            if let Err(error) = out.write_all(frame) {
                ending = Some(format!("the frames' reader went away: {error}"));
                break;
            }
            tick += 1;

            if state.retried {
                if state.size != Some((width, height)) || state.format != Some(format) {
                    ending = Some("the display changed size or format".into());
                    break;
                }
                state.retried = false;
            }
            if state.failed_in_a_row >= FAILURES {
                ending = Some(format!("{FAILURES} captures failed in a row"));
                break;
            }
        }
        let _ = out.flush();
        let seconds = began.elapsed().as_secs_f64();
        eprintln!(
            "record_host: {tick} frames written over {seconds:.2} s ({:.2} a second); \
             {} captured, {} written fresh, {repeated} repeated, {} captures dropped unwritten, \
             {} failed; {changed} changed from the frame before, {late} written late",
            tick as f64 / seconds.max(f64::EPSILON),
            state.captured,
            tick - repeated,
            state.captured.saturating_sub(tick - repeated),
            state.failed,
        );
        let per_second: Vec<String> = changed_per_second.iter().map(u32::to_string).collect();
        eprintln!(
            "record_host: changed frames in each second: {}",
            per_second.join(" ")
        );
        for source in state.sources.drain(..) {
            source.destroy();
        }
        for buffer in state.buffers.drain(..) {
            buffer.destroy();
        }
        pool.destroy();
        capturer.destroy();
        let _ = connection.flush();
        match ending {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }

    /// `--rows X0 X1`, if given.
    fn rows_argument() -> Result<Option<(usize, usize)>, String> {
        let arguments: Vec<String> = std::env::args().skip(1).collect();
        match arguments.as_slice() {
            [] => Ok(None),
            [flag, from, to] if flag == "--rows" => {
                let column = |text: &str| {
                    text.parse::<usize>()
                        .map_err(|_| format!("--rows wants two columns, not {text:?}"))
                };
                Ok(Some((column(from)?, column(to)?)))
            }
            _ => Err("usage: record_host [--rows X0 X1]".into()),
        }
    }

    /// `--rows`: writes each capture's row sums as it completes, until
    /// stopped.
    fn write_rows(
        connection: &Connection,
        queue: &mut EventQueue<State>,
        state: &mut State,
        out: &mut std::fs::File,
    ) -> Result<(), String> {
        let mut frames = 0u64;
        let mut bytes = Vec::new();
        while !STOP.load(Ordering::Relaxed) {
            wait(connection, queue, state, Instant::now() + FRAME)?;
            for (at, sums) in state.summed.drain(..) {
                bytes.clear();
                bytes.extend_from_slice(&at.to_le_bytes());
                for sum in sums {
                    bytes.extend_from_slice(&sum.to_le_bytes());
                }
                out.write_all(&bytes)
                    .map_err(|error| format!("the rows' reader went away: {error}"))?;
                frames += 1;
            }
            if state.retried {
                return Err("the display changed size or format".into());
            }
            if state.failed_in_a_row >= FAILURES {
                return Err(format!("{FAILURES} captures failed in a row"));
            }
        }
        // Weston's GL renderer aborts when a source goes while its
        // read-back is under way: let the last capture finish first.
        let deadline = Instant::now() + Duration::from_millis(500);
        state.stopping = true;
        while Instant::now() < deadline
            && state
                .slots
                .iter()
                .any(|slot| matches!(slot, Slot::Capturing(..)))
        {
            wait(connection, queue, state, Instant::now() + FRAME)?;
        }
        eprintln!(
            "record_host: {frames} frames' rows written, {} failed",
            state.failed
        );
        Ok(())
    }

    /// CLOCK_MONOTONIC in nanoseconds, the clock of Weston's timeline.
    fn monotonic_nanos() -> u64 {
        let mut now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `now` is a valid timespec to write.
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
        now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64
    }

    /// Each row's sum of its pixels' channels from column `from` to `to`.
    fn row_sums(frame: &[u8], width: usize, from: usize, to: usize) -> Vec<u32> {
        frame
            .chunks_exact(width * 4)
            .map(|row| {
                row[from * 4..to * 4]
                    .chunks_exact(4)
                    .map(|pixel| u32::from(pixel[0]) + u32::from(pixel[1]) + u32::from(pixel[2]))
                    .sum()
            })
            .collect()
    }

    impl State {
        /// Asks for the next frame as soon as Weston can repaint for it: when
        /// none is on its way, or when the one on its way has had its repaint
        /// and waits on its read-back. Each capture takes one repaint, so
        /// this asks for about one a refresh.
        fn ask(&mut self) {
            if self.stopping
                || self.retried
                || self.failed_in_a_row >= FAILURES
                || self.buffers.is_empty()
            {
                return;
            }
            let mut in_flight = self.slots.iter().filter_map(|slot| match slot {
                Slot::Capturing(_, at) => Some(*at),
                _ => None,
            });
            let ready = match (in_flight.next(), in_flight.next()) {
                (None, _) => true,
                // With --rows, the next once a refresh has passed, so it
                // is for the next repaint even when the read-back of the
                // one before takes longer than a refresh. Two asked for the
                // same repaint would both show it.
                (Some(at), None) => {
                    let period = match self.rows {
                        Some(_) => self.refresh.unwrap_or(FRAME),
                        None => FRAME,
                    };
                    at.elapsed() > period
                }
                _ => false,
            };
            let idle = self.slots.iter().position(|slot| *slot == Slot::Idle);
            if let (true, Some(idle)) = (ready, idle) {
                self.sources[idle].capture(&self.buffers[idle]);
                self.slots[idle] = Slot::Capturing(self.asked, Instant::now());
                self.asked += 1;
            }
        }
    }

    /// The newest finished frame: its slot, the number of its capture, and its hash.
    fn newest(slots: &[Slot; SLOTS]) -> Option<(usize, u64, u64)> {
        slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| match slot {
                Slot::Done(asked, hash) => Some((index, *asked, *hash)),
                _ => None,
            })
            .max_by_key(|(_, asked, _)| *asked)
    }

    /// Reads and handles Weston's events until `until`.
    fn wait(
        connection: &Connection,
        queue: &mut EventQueue<State>,
        state: &mut State,
        until: Instant,
    ) -> Result<(), String> {
        loop {
            queue
                .dispatch_pending(state)
                .map_err(|error| error.to_string())?;
            state.ask();
            connection.flush().map_err(|error| error.to_string())?;
            let now = Instant::now();
            if now >= until || STOP.load(Ordering::Relaxed) {
                return Ok(());
            }
            let Some(guard) = queue.prepare_read() else {
                continue;
            };
            let mut poll = libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let timeout = (until - now).as_micros().div_ceil(1000) as libc::c_int;
            // SAFETY: one valid pollfd, as the count says.
            let ready = unsafe { libc::poll(&mut poll, 1, timeout) };
            if ready > 0 {
                guard.read().map_err(|error| error.to_string())?;
            }
        }
    }

    const ARGB8888: u32 = u32::from_le_bytes(*b"AR24");
    const XRGB8888: u32 = u32::from_le_bytes(*b"XR24");

    /// ffmpeg's name for a DRM format's bytes in memory, little-endian.
    fn ffmpeg_format(format: u32) -> Option<&'static str> {
        Some(match &format.to_le_bytes() {
            b"AR24" => "bgra",
            b"XR24" => "bgr0",
            b"AB24" => "rgba",
            b"XB24" => "rgb0",
            _ => return None,
        })
    }

    /// A fast hash of a frame's pixels, to tell a changed frame from a still one.
    fn hash(bytes: &[u8]) -> u64 {
        let mut lanes = [0x9e37_79b9_7f4a_7c15u64; 4];
        let mut chunks = bytes.chunks_exact(32);
        for chunk in &mut chunks {
            for (lane, word) in lanes.iter_mut().zip(chunk.chunks_exact(8)) {
                let word = u64::from_le_bytes(word.try_into().expect("8 bytes"));
                *lane = (*lane ^ word)
                    .wrapping_mul(0x0100_0000_01b3)
                    .rotate_left(29);
            }
        }
        let mut total = lanes.iter().fold(0u64, |total, lane| {
            (total ^ lane).wrapping_mul(0xff51_afd7_ed55_8ccd)
        });
        for byte in chunks.remainder() {
            total = (total ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
        total
    }

    fn require_private_display() -> Result<(), String> {
        let runtime = std::path::PathBuf::from(
            std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/missing".into()),
        );
        let private = std::env::var("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY").as_deref() == Ok("1")
            && std::env::var("WAYLAND_DISPLAY").as_deref() == Ok("wayland-yapfast-frameworks")
            && runtime
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("yapfast-framework-display-"));
        if private {
            Ok(())
        } else {
            Err("records only the isolated display of scripts/run-framework-isolated".into())
        }
    }

    impl Dispatch<source::WestonCaptureSourceV1, usize> for State {
        fn event(
            state: &mut Self,
            _: &source::WestonCaptureSourceV1,
            event: source::Event,
            slot: &usize,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                source::Event::Format { drm_format } => state.format = Some(drm_format),
                source::Event::Size { width, height } => state.size = Some((width, height)),
                source::Event::Complete => {
                    if let (Slot::Capturing(..), Some((from, to)), Some((width, _))) =
                        (state.slots[*slot], state.rows, state.size)
                    {
                        let bytes = state.frame_bytes;
                        let pixels = state.pixels.as_ref().expect("mapped");
                        let frame = &pixels[slot * bytes..(slot + 1) * bytes];
                        let sums = row_sums(frame, width as usize, from, to);
                        state.summed.push((monotonic_nanos(), sums));
                        state.slots[*slot] = Slot::Idle;
                        state.captured += 1;
                        state.failed_in_a_row = 0;
                    } else if let Slot::Capturing(asked, _) = state.slots[*slot] {
                        let bytes = state.frame_bytes;
                        let pixels = state.pixels.as_ref().expect("mapped");
                        let hash = hash(&pixels[slot * bytes..(slot + 1) * bytes]);
                        // An older frame not yet written never will be.
                        for other in state.slots.iter_mut() {
                            if matches!(other, Slot::Done(older, _) if *older < asked) {
                                *other = Slot::Idle;
                            }
                        }
                        state.slots[*slot] = Slot::Done(asked, hash);
                        state.captured += 1;
                        state.failed_in_a_row = 0;
                    }
                }
                source::Event::Retry => {
                    state.retried = true;
                    state.slots[*slot] = Slot::Idle;
                }
                source::Event::Failed { msg } => {
                    eprintln!(
                        "record_host: a capture failed: {}",
                        msg.as_deref().unwrap_or("no reason given")
                    );
                    state.failed += 1;
                    state.failed_in_a_row += 1;
                    state.slots[*slot] = Slot::Idle;
                }
            }
        }
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    /// Objects whose events the recording doesn't need.
    macro_rules! ignore {
        ($($interface:ty),*) => {$(
            impl Dispatch<$interface, ()> for State {
                fn event(
                    _: &mut Self,
                    _: &$interface,
                    _: <$interface as wayland_client::Proxy>::Event,
                    _: &(),
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                }
            }
        )*};
    }
    impl Dispatch<wl_output::WlOutput, ()> for State {
        fn event(
            state: &mut Self,
            _: &wl_output::WlOutput,
            event: wl_output::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wl_output::Event::Mode { flags, refresh, .. } = event
                && flags
                    .into_result()
                    .is_ok_and(|flags| flags.contains(wl_output::Mode::Current))
                && refresh > 0
            {
                state.refresh = Some(Duration::from_nanos(1_000_000_000_000 / refresh as u64));
            }
        }
    }

    ignore!(
        wl_shm::WlShm,
        wl_shm_pool::WlShmPool,
        wl_buffer::WlBuffer,
        capture::WestonCaptureV1
    );
}
