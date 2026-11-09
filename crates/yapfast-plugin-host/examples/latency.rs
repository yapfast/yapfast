//! Times the helper's IPC exchange without Python in the loop.
//!
//! `testing/measure_plugins.py` speaks the same framed protocol, but its
//! timestamps include the interpreter and JSON work Python pays, which is
//! larger than the 50 µs p99 the plan sets for an event ([§1](../PLAN.md)).
//! This example is the probe for that budget: it spawns the helper, loads one
//! module under one id, and times the same exchanges with `std::time::Instant`
//! and the protocol types the parent uses.
//!
//! It reports, as one JSON record:
//!
//! - spawn-to-ready: from immediately before the spawn to the `ready` frame;
//! - spawn-to-first-call: the same clock through `ready`, the `load`, and the
//!   first warmup call's `reply`, so module load is inside the span;
//! - one `load` sample: write the `load` frame and read the `loaded` frame;
//! - one `round_trip` sample per measured call: write the `call` frame and
//!   flush, then read the `reply` with that id. Warmup calls are not counted.
//!
//! ```sh
//! cargo run --profile measure -p yapfast-plugin-host --example latency -- \
//!     --helper target/measure/yapfast-plugins \
//!     --module "$PWD/target/plugin-fixtures/echo.wasm" \
//!     --helper-arg --no-cache \
//!     --json-out target/plugin-measurements/latency-1.json
//! ```
//!
//! The helper's stderr is captured and reported as text; nothing the helper
//! writes to stdout is ever reprinted, because those bytes are protocol frames.
//! See `docs/plugin-measurements.md`.
//!
//! This example links neither Extism nor wasmtime: it only speaks the protocol,
//! so it builds without the crate's `host` feature.

use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitCode, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use yapfast_plugin_api::{
    Frame, HEADER_LEN, Kind, Payload, ProtocolError, ReadError, Request, Response,
};

/// The plan's p99 budget for one event to a subscribed plugin, pipe included.
const EVENT_P99_BUDGET_NS: u64 = 50_000;

/// The default counted round trips: enough for a stable p99.
const DEFAULT_REQUESTS: usize = 2_000;

/// The default uncounted round trips before them.
const DEFAULT_WARMUP: usize = 200;

/// The default payload's `data` length, matching the Python harness.
const DEFAULT_PAYLOAD_BYTES: usize = 1_024;

/// The default limit for one exchange.
const DEFAULT_TIMEOUT_SECONDS: u64 = 10;

/// The bytes of stderr kept for the report's tail.
const STDERR_TAIL: usize = 8 * 1_024;

/// How often the watchdog looks at an armed deadline. It only wakes the probe
/// between exchanges, so it never sits inside a measured span.
const WATCHDOG_POLL: Duration = Duration::from_millis(2);

/// A deadline that is not armed: no exchange is in flight.
const DISARMED: u64 = u64::MAX;

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(Some(args)) => args,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("latency: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    if let Some(problem) = prerequisite_problem(&args.helper, &args.module) {
        eprintln!("latency: {problem}");
        return ExitCode::from(3);
    }

    match measure(&args) {
        Ok(record) => {
            let text = match serde_json::to_string_pretty(&record) {
                Ok(text) => text,
                Err(error) => {
                    eprintln!("latency: cannot write the record: {error}");
                    return ExitCode::FAILURE;
                }
            };
            match &args.json_out {
                Some(path) => {
                    if let Err(error) = std::fs::write(path, format!("{text}\n")) {
                        eprintln!("latency: cannot write {}: {error}", path.display());
                        return ExitCode::FAILURE;
                    }
                    eprintln!("latency: wrote {}", path.display());
                }
                // Only the record goes to stdout: the helper's frames never do.
                None => println!("{text}"),
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("latency: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The command line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    helper: PathBuf,
    module: PathBuf,
    plugin: String,
    method: String,
    warmup: usize,
    requests: usize,
    payload_bytes: usize,
    helper_args: Vec<String>,
    timeout: Duration,
    json_out: Option<PathBuf>,
    run_label: Option<String>,
}

const USAGE: &str = "\
usage: latency --helper <path> --module <absolute .wasm> [options]

  --helper <path>      the yapfast-plugins binary to spawn (required)
  --module <path>      the absolute .wasm to load under --plugin (required)
  --plugin <id>        the id of the one instance (default: measure)
  --method <name>      the guest export every call names (default: echo)
  --warmup <n>         uncounted round trips first (default: 200)
  --requests <n>       counted round trips (default: 2000)
  --payload-bytes <n>  bytes in the call payload's `data` string (default: 1024)
  --helper-arg <arg>   one argument for the helper, such as --no-cache (repeatable)
  --timeout <seconds>  limit for any one exchange (default: 10)
  --json-out <path>    write the record there instead of stdout
  --run-label <name>   the label the record carries as scenario.name
  --help               print this text
";

/// Parses the command line, or `None` for `--help`.
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Option<Args>, String> {
    let mut helper: Option<PathBuf> = None;
    let mut module: Option<PathBuf> = None;
    let mut plugin = "measure".to_owned();
    let mut method = "echo".to_owned();
    let mut warmup = DEFAULT_WARMUP;
    let mut requests = DEFAULT_REQUESTS;
    let mut payload_bytes = DEFAULT_PAYLOAD_BYTES;
    let mut helper_args = Vec::new();
    let mut timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECONDS);
    let mut json_out: Option<PathBuf> = None;
    let mut run_label: Option<String> = None;

    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => return Ok(None),
            "--helper" => helper = Some(PathBuf::from(value(&mut args, "--helper")?)),
            "--module" => module = Some(PathBuf::from(value(&mut args, "--module")?)),
            "--plugin" => plugin = value(&mut args, "--plugin")?,
            "--method" => method = value(&mut args, "--method")?,
            "--warmup" => warmup = number(&mut args, "--warmup")?,
            "--requests" => requests = positive(&mut args, "--requests")?,
            "--payload-bytes" => payload_bytes = positive(&mut args, "--payload-bytes")?,
            // One argument at a time, so a helper flag that takes a value is
            // passed as two: `--helper-arg --cache-dir --helper-arg C:/cache`.
            "--helper-arg" => helper_args.push(value(&mut args, "--helper-arg")?),
            "--timeout" => {
                let seconds = positive(&mut args, "--timeout")?;
                timeout = Duration::from_secs(seconds as u64);
            }
            "--json-out" => json_out = Some(PathBuf::from(value(&mut args, "--json-out")?)),
            "--run-label" => run_label = Some(value(&mut args, "--run-label")?),
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(Some(Args {
        helper: helper.ok_or("--helper is required")?,
        module: module.ok_or("--module is required")?,
        plugin,
        method,
        warmup,
        requests,
        payload_bytes,
        helper_args,
        timeout,
        json_out,
        run_label,
    }))
}

/// The value an option promises.
fn value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{option} needs a value"))
}

/// A count an option carries.
fn number(args: &mut impl Iterator<Item = String>, option: &str) -> Result<usize, String> {
    let text = value(args, option)?;
    text.parse()
        .map_err(|_| format!("{option} needs a whole number, not `{text}`"))
}

/// A count of at least one, which is what every size and limit here is.
fn positive(args: &mut impl Iterator<Item = String>, option: &str) -> Result<usize, String> {
    let count = number(args, option)?;
    if count == 0 {
        return Err(format!("{option} must be at least 1"));
    }
    Ok(count)
}

/// Why a prerequisite the probe cannot invent is missing.
fn prerequisite_problem(helper: &Path, module: &Path) -> Option<String> {
    if !helper.is_file() {
        return Some(format!("helper not found: {}", helper.display()));
    }
    // The protocol's `load` takes an absolute path, and the helper resolves
    // none of its own: the file this probe checked is the file it loads.
    if !module.is_absolute() {
        return Some(format!("module path {} is not absolute", module.display()));
    }
    if !module.is_file() {
        return Some(format!("module not found: {}", module.display()));
    }
    None
}

/// p50, p95 and p99 of the samples, by the documented nearest-rank method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Percentiles {
    p50: u64,
    p95: u64,
    p99: u64,
}

/// The nearest-rank percentile: sort, then take the 1-based rank
/// `ceil(percent * n / 100)`, so the answer is always one observed sample.
///
/// `None` for no samples, or for a percent outside 1..=100.
fn nearest_rank(values: &[u64], percent: u32) -> Option<u64> {
    if values.is_empty() || !(1..=100).contains(&percent) {
        return None;
    }
    let mut ordered = values.to_vec();
    ordered.sort_unstable();
    let rank = (percent as usize * ordered.len()).div_ceil(100).max(1);
    ordered.get(rank - 1).copied()
}

/// p50, p95 and p99 of the samples, or `None` for none.
fn percentiles(values: &[u64]) -> Option<Percentiles> {
    Some(Percentiles {
        p50: nearest_rank(values, 50)?,
        p95: nearest_rank(values, 95)?,
        p99: nearest_rank(values, 99)?,
    })
}

/// The plan's verdict on a measured p99, or `None` without one.
fn budget_verdict(p99: Option<u64>) -> Option<&'static str> {
    p99.map(|p99| {
        if p99 <= EVENT_P99_BUDGET_NS {
            "pass"
        } else {
            "fail"
        }
    })
}

/// The call payload: one JSON object whose `data` string is `bytes` long, so
/// the frame carries `bytes` + 11 bytes of JSON, as the Python harness builds.
fn call_payload(bytes: usize) -> Payload {
    Payload::from(json!({ "data": "x".repeat(bytes) }))
}

/// Encodes one request as the frame to send under `id`.
fn encode(request: &Request, id: u64) -> Result<Vec<u8>, ProbeError> {
    let mut frame = Vec::new();
    request.to_frame(id)?.write_to(&mut frame)?;
    Ok(frame)
}

/// Writes an id into an already encoded frame's header, in place.
///
/// The measured loop reuses one encoded `call` frame and changes only its id,
/// so no JSON encoding runs inside the span being timed. The header is the
/// payload length at bytes 0..4, the kind at 4, and the id at 5..13.
fn set_id(frame: &mut [u8], id: u64) {
    assert!(
        frame.len() >= HEADER_LEN,
        "an encoded frame carries a whole header"
    );
    frame[5..13].copy_from_slice(&id.to_le_bytes());
}

/// Writes one encoded frame and flushes it: a pipe has no autoflush.
fn write_encoded(output: &mut impl Write, frame: &[u8]) -> Result<(), ProbeError> {
    output.write_all(frame)?;
    output.flush()?;
    Ok(())
}

/// Reads the one response that answers `id`, refusing one that answers another.
fn read_reply(input: &mut impl Read, id: u64) -> Result<Response, ProbeError> {
    let frame = Frame::read_from(input)?.ok_or(ProbeError::EndOfStream)?;
    let response = Response::from_frame(&frame)?;
    // Ids are the parent's, and one answer pairs with one request: a frame
    // under another id means the two sides are not in step.
    if frame.id() != id {
        return Err(ProbeError::UnexpectedId {
            expected: id,
            got: frame.id(),
        });
    }
    match response {
        Response::Error { message } => Err(ProbeError::Helper { id, message }),
        other => Ok(other),
    }
}

/// Writes one request under `id` and returns the response that answers it.
fn exchange(
    input: &mut impl Read,
    output: &mut impl Write,
    id: u64,
    request: &Request,
) -> Result<Response, ProbeError> {
    let frame = encode(request, id)?;
    write_encoded(output, &frame)?;
    read_reply(input, id)
}

/// Checks that a call's answer is a `reply` carrying the payload's exact bytes.
fn check_echo(response: Response, expected: &[u8], id: u64) -> Result<(), ProbeError> {
    match response {
        Response::Reply { payload } if payload.as_bytes() == expected => Ok(()),
        Response::Reply { payload } => Err(ProbeError::Message(format!(
            "the reply to {id} is not the call's payload: {} bytes came back instead of {}",
            payload.as_bytes().len(),
            expected.len()
        ))),
        other => Err(ProbeError::UnexpectedKind {
            expected: Kind::Reply,
            got: other.kind(),
        }),
    }
}

/// Why the probe stopped.
#[derive(Debug)]
enum ProbeError {
    /// A pipe failed.
    Io(io::Error),
    /// A frame could not be read.
    Read(ReadError),
    /// A frame's envelope is not what its kind promises.
    Protocol(ProtocolError),
    /// The helper closed stdout where a frame was due.
    EndOfStream,
    /// The helper answered an id the probe did not ask under.
    UnexpectedId { expected: u64, got: u64 },
    /// The helper answered a frame the probe did not expect there.
    UnexpectedKind { expected: Kind, got: Kind },
    /// The helper said it could not serve a request.
    Helper { id: u64, message: String },
    /// An exchange outlived `--timeout` and the helper was killed.
    TimedOut { seconds: f64 },
    /// The helper stopped without the clean end the probe requires.
    Exited { code: Option<i32>, stderr: String },
    /// Anything else, as a sentence.
    Message(String),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "a pipe failed: {error}"),
            Self::Read(error) => write!(f, "reading a frame: {error}"),
            Self::Protocol(error) => write!(f, "a frame's envelope is wrong: {error}"),
            Self::EndOfStream => write!(f, "the helper closed stdout where a frame was due"),
            Self::UnexpectedId { expected, got } => write!(
                f,
                "the helper answered id {got} where id {expected} was due"
            ),
            Self::UnexpectedKind { expected, got } => write!(
                f,
                "the helper wrote a {got:?} frame where a {expected:?} was due"
            ),
            Self::Helper { id, message } => {
                write!(f, "the helper could not serve id {id}: {message}")
            }
            Self::TimedOut { seconds } => {
                write!(f, "an exchange did not answer within {seconds} s")
            }
            Self::Exited { code, stderr } => {
                write!(f, "the helper exited with {code:?}")?;
                if !stderr.is_empty() {
                    write!(f, "; its stderr said: {stderr}")?;
                }
                Ok(())
            }
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ProbeError {}

impl From<io::Error> for ProbeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ReadError> for ProbeError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

impl From<ProtocolError> for ProbeError {
    fn from(error: ProtocolError) -> Self {
        Self::Protocol(error)
    }
}

/// What the helper did as it stopped.
#[derive(Debug)]
struct Ending {
    exit_code: Option<i32>,
    stdout_eof: bool,
    stderr_tail: String,
}

/// The helper process and the pipes it speaks over.
///
/// Reads block on the calling thread: a reader thread handing frames over a
/// channel would add its own wake-up to every measured round trip, and the
/// point of this probe is that nothing of the probe's own sits in the span.
/// Hang detection is a watchdog instead, armed around each exchange.
struct Helper {
    child: Arc<Mutex<Child>>,
    to_helper: ChildStdin,
    from_helper: BufReader<ChildStdout>,
    stderr: Option<std::thread::JoinHandle<Vec<u8>>>,
    epoch: Instant,
    timeout: Duration,
    deadline: Arc<AtomicU64>,
    timed_out: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    watchdog: Option<std::thread::JoinHandle<()>>,
}

impl Helper {
    /// Spawns the helper with `argv`, its stderr already being drained.
    fn spawn(argv: &[String], timeout: Duration) -> Result<Self, ProbeError> {
        let mut command = Command::new(&argv[0]);
        command
            .args(&argv[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| ProbeError::Message(format!("cannot start {}: {error}", argv[0])))?;

        let to_helper = child.stdin.take().expect("piped stdin");
        let from_helper = BufReader::new(child.stdout.take().expect("piped stdout"));
        let stderr_pipe = child.stderr.take().expect("piped stderr");

        // Draining stderr on its own thread keeps a chatty helper from blocking
        // on a full pipe while the probe waits for a frame.
        let stderr = std::thread::spawn(move || drain_stderr(stderr_pipe));

        let child = Arc::new(Mutex::new(child));
        let deadline = Arc::new(AtomicU64::new(DISARMED));
        let timed_out = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let epoch = Instant::now();
        let watchdog = {
            let child = Arc::clone(&child);
            let deadline = Arc::clone(&deadline);
            let timed_out = Arc::clone(&timed_out);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    let armed = deadline.load(Ordering::Acquire);
                    if armed != DISARMED && epoch.elapsed().as_nanos() as u64 >= armed {
                        timed_out.store(true, Ordering::Release);
                        if let Ok(mut child) = child.lock() {
                            let _ = child.kill();
                        }
                        return;
                    }
                    std::thread::sleep(WATCHDOG_POLL);
                }
            })
        };

        Ok(Self {
            child,
            to_helper,
            from_helper,
            stderr: Some(stderr),
            epoch,
            timeout,
            deadline,
            timed_out,
            stop,
            watchdog: Some(watchdog),
        })
    }

    /// Arms the watchdog for one exchange.
    fn arm(&self) {
        let nanos = self.timeout.as_nanos() as u64;
        self.deadline.store(
            self.epoch.elapsed().as_nanos() as u64 + nanos,
            Ordering::Release,
        );
    }

    /// Disarms it again: no exchange is in flight.
    fn disarm(&self) {
        self.deadline.store(DISARMED, Ordering::Release);
    }

    /// The timeout, if this exchange is why the helper was killed.
    fn expired(&self) -> Option<ProbeError> {
        self.timed_out
            .load(Ordering::Acquire)
            .then_some(ProbeError::TimedOut {
                seconds: self.timeout.as_secs_f64(),
            })
    }

    /// Writes one encoded frame.
    fn write_frame(&mut self, frame: &[u8]) -> Result<(), ProbeError> {
        self.arm();
        let outcome = write_encoded(&mut self.to_helper, frame);
        self.disarm();
        match self.expired() {
            Some(error) => Err(error),
            None => outcome,
        }
    }

    /// Reads the response that answers `id`.
    fn read_reply(&mut self, id: u64) -> Result<Response, ProbeError> {
        self.arm();
        let outcome = read_reply(&mut self.from_helper, id);
        self.disarm();
        match self.expired() {
            Some(error) => Err(error),
            None => outcome,
        }
    }

    /// Encodes `request` under `id`, writes it, and reads the answer.
    fn send_request(&mut self, id: u64, request: &Request) -> Result<Response, ProbeError> {
        self.arm();
        let outcome = exchange(&mut self.from_helper, &mut self.to_helper, id, request);
        self.disarm();
        match self.expired() {
            Some(error) => Err(error),
            None => outcome,
        }
    }

    /// Waits for the clean stop the probe requires: the helper ends stdout
    /// after its last `unloaded`, exits with 0, and has nothing left on stderr.
    ///
    /// A helper that writes another frame, or that does not exit, is a failed
    /// run: the probe will not report a number taken from a process it could
    /// not end.
    fn finish(&mut self) -> Result<Ending, ProbeError> {
        self.arm();
        let next = Frame::read_from(&mut self.from_helper);
        self.disarm();
        if let Some(error) = self.expired() {
            return Err(error);
        }
        let stdout_eof = match next {
            Ok(None) => true,
            Ok(Some(frame)) => {
                return Err(ProbeError::Message(format!(
                    "the helper wrote a {:?} frame after the last unload instead of stopping",
                    frame.kind()
                )));
            }
            Err(error) => return Err(ProbeError::Read(error)),
        };

        let status = self.wait_until(Instant::now() + self.timeout)?;
        let stderr_tail = self.take_stderr();
        Ok(Ending {
            exit_code: status.code(),
            stdout_eof,
            stderr_tail,
        })
    }

    /// Reaps the helper, or reports it as still running at `limit`.
    fn wait_until(&mut self, limit: Instant) -> Result<ExitStatus, ProbeError> {
        loop {
            let status = self.child.lock().expect("the helper's handle").try_wait()?;
            if let Some(status) = status {
                return Ok(status);
            }
            if Instant::now() >= limit {
                return Err(ProbeError::Message(format!(
                    "the helper did not exit within {} s",
                    self.timeout.as_secs_f64()
                )));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// The helper's stderr as text, once its pipe has ended.
    fn take_stderr(&mut self) -> String {
        let bytes = self
            .stderr
            .take()
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        // Every path, including a failed run, leaves no helper behind.
        self.stop.store(true, Ordering::Release);
        if let Some(watchdog) = self.watchdog.take() {
            let _ = watchdog.join();
        }
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Spawns the helper, measures it, and returns the run's JSON record.
fn measure(args: &Args) -> Result<Value, ProbeError> {
    let mut argv = vec![args.helper.display().to_string()];
    argv.extend(args.helper_args.iter().cloned());

    let payload = call_payload(args.payload_bytes);
    // The bytes the guest must hand back, kept as they were sent: the reply is
    // compared byte for byte, not by re-decoding it.
    let echoed = payload.as_bytes().to_vec();
    let mut call = encode(
        &Request::call(args.plugin.clone(), args.method.clone(), payload),
        1,
    )?;

    // The spawn-to-ready and spawn-to-first-call spans start here, at the same
    // instant: immediately before the process exists.
    let started = Instant::now();
    let mut helper = Helper::spawn(&argv, args.timeout)?;
    let mut next = 1u64;

    // `ready` is unsolicited, and alone at id 0.
    let ready = helper.read_reply(0)?;
    if ready != Response::Ready {
        return Err(ProbeError::UnexpectedKind {
            expected: Kind::Ready,
            got: ready.kind(),
        });
    }
    let spawn_to_ready = started.elapsed();

    let load_id = next;
    next += 1;
    let load = Request::Load {
        plugin: args.plugin.clone(),
        module: args.module.clone(),
    };
    let load_started = Instant::now();
    let loaded = helper.send_request(load_id, &load)?;
    if loaded != Response::Loaded {
        return Err(ProbeError::UnexpectedKind {
            expected: Kind::Loaded,
            got: loaded.kind(),
        });
    }
    let load_ns = load_started.elapsed().as_nanos() as u64;

    let mut round_trips = Vec::with_capacity(args.requests);
    let mut spawn_to_first_call: Option<Duration> = None;
    for index in 0..args.warmup + args.requests {
        let id = next;
        next += 1;
        // Only the id changes between calls: the encoded frame is reused so the
        // measured span holds the write, the flush and the read, and none of
        // the probe's own JSON encoding.
        set_id(&mut call, id);

        let call_started = Instant::now();
        helper.write_frame(&call)?;
        let reply = helper.read_reply(id)?;
        let elapsed = call_started.elapsed();
        check_echo(reply, &echoed, id)?;

        if spawn_to_first_call.is_none() {
            spawn_to_first_call = Some(started.elapsed());
        }
        // Warmup calls run the same path and are not counted.
        if index >= args.warmup {
            round_trips.push(elapsed.as_nanos() as u64);
        }
    }

    // The unload of the only instance is the helper's last exchange: it is
    // disposable, so it stops as soon as it holds nothing.
    let unload_id = next;
    let unload = Request::Unload {
        plugin: args.plugin.clone(),
    };
    let unloaded = helper.send_request(unload_id, &unload)?;
    if unloaded != Response::Unloaded {
        return Err(ProbeError::UnexpectedKind {
            expected: Kind::Unloaded,
            got: unloaded.kind(),
        });
    }
    let ending = helper.finish()?;
    if !ending.stdout_eof {
        return Err(ProbeError::Message(
            "the helper's stdout did not end cleanly".to_owned(),
        ));
    }
    if ending.exit_code != Some(0) {
        return Err(ProbeError::Exited {
            code: ending.exit_code,
            stderr: ending.stderr_tail,
        });
    }

    let round_trip_percentiles = percentiles(&round_trips);
    let observed_p99 = round_trip_percentiles.map(|percentiles| percentiles.p99);
    let mut load_by_plugin = Map::new();
    load_by_plugin.insert(args.plugin.clone(), json!(load_ns));
    let first_call: Vec<u64> = spawn_to_first_call
        .into_iter()
        .map(|span| span.as_nanos() as u64)
        .collect();

    Ok(json!({
        "schema": "yapfast-plugin-latency/1",
        "clock": "std::time::Instant",
        "platform": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
        "build": {
            "helper": args.helper.display().to_string(),
            "helper_size_bytes": std::fs::metadata(&args.helper).map(|file| file.len()).ok(),
            "argv": argv,
        },
        "scenario": {
            "name": args.run_label,
            "module": args.module.display().to_string(),
            "plugin": args.plugin,
            "method": args.method,
            "warmup": args.warmup,
            "requests": args.requests,
            "payload_bytes": args.payload_bytes,
            "call_payload_bytes": echoed.len(),
            "timeout_ms": args.timeout.as_millis() as u64,
        },
        "latency": {
            "spawn_to_ready_ns": timing(
                &[spawn_to_ready.as_nanos() as u64],
                "from immediately before spawn to the ready frame at id 0",
            ),
            "spawn_to_first_call_ns": timing(
                &first_call,
                "from immediately before spawn through ready, the load, and the \
                 first call's reply",
            ),
            "load_ns": {
                "unit": "nanoseconds",
                "definition": "around one load: write the load frame and read the \
                              loaded frame carrying that id",
                "samples": [load_ns],
                "percentiles": percentiles_json(&[load_ns]),
                "by_plugin": load_by_plugin,
            },
            "round_trip_ns": timing(
                &round_trips,
                "write a call frame, flush it, and read the reply frame with its id",
            ),
        },
        // The round trip is the floor of the plan's event budget: one event to a
        // subscribed plugin also carries the parent's dispatch around this.
        "budget": {
            "name": "event_p99_ns",
            "target_ns": EVENT_P99_BUDGET_NS,
            "metric": "latency.round_trip_ns.percentiles.p99",
            "observed_ns": observed_p99,
            "verdict": budget_verdict(observed_p99),
        },
        "helper_exit": {
            "exit_code": ending.exit_code,
            "stdout_eof": ending.stdout_eof,
            "ended_by": "last_unload",
            "stderr_tail": ending.stderr_tail,
        },
    }))
}

/// One timing block: the definition, the samples and their percentiles.
fn timing(samples: &[u64], definition: &str) -> Value {
    json!({
        "unit": "nanoseconds",
        "definition": definition,
        "samples": samples,
        "percentiles": percentiles_json(samples),
    })
}

/// The percentiles as the record carries them, or `null` for no samples.
fn percentiles_json(samples: &[u64]) -> Value {
    match percentiles(samples) {
        Some(percentiles) => json!({
            "p50": percentiles.p50,
            "p95": percentiles.p95,
            "p99": percentiles.p99,
        }),
        None => Value::Null,
    }
}

/// Keeps the helper's stderr out of the protocol stream and its bytes out of
/// the report, while still telling a failed run what the helper said.
///
/// Only the last [`STDERR_TAIL`] bytes are kept: a helper that prints without
/// end must not grow the probe's memory, and the end is what a failure needs.
fn drain_stderr(mut pipe: impl Read) -> Vec<u8> {
    let mut tail = Vec::new();
    let mut chunk = [0u8; 4 * 1_024];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => return tail,
            Ok(read) => {
                tail.extend_from_slice(&chunk[..read]);
                if tail.len() > STDERR_TAIL {
                    let excess = tail.len() - STDERR_TAIL;
                    tail.drain(..excess);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return tail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn arguments(list: &[&str]) -> Vec<String> {
        list.iter().map(|argument| (*argument).to_owned()).collect()
    }

    /// The frame a helper would answer `id` with.
    fn answering(id: u64, response: Response) -> Vec<u8> {
        let mut bytes = Vec::new();
        response
            .to_frame(id)
            .expect("a frame")
            .write_to(&mut bytes)
            .expect("a written frame");
        bytes
    }

    #[test]
    fn nearest_rank_takes_one_observed_sample() {
        assert_eq!(nearest_rank(&[30, 10, 20], 50), Some(20));
        assert_eq!(nearest_rank(&[30, 10, 20], 99), Some(30));
        assert_eq!(nearest_rank(&[7], 100), Some(7));
    }

    #[test]
    fn percentiles_are_p50_p95_and_p99_of_ten_samples() {
        let samples = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        assert_eq!(
            percentiles(&samples),
            Some(Percentiles {
                p50: 5,
                p95: 10,
                p99: 10,
            })
        );
    }

    #[test]
    fn percentiles_of_a_hundred_samples_are_the_ranks_they_name() {
        let samples: Vec<u64> = (1..=100).collect();
        assert_eq!(
            percentiles(&samples),
            Some(Percentiles {
                p50: 50,
                p95: 95,
                p99: 99,
            })
        );
    }

    #[test]
    fn no_samples_have_no_percentile() {
        assert_eq!(nearest_rank(&[], 50), None);
        assert_eq!(percentiles(&[]), None);
        assert_eq!(nearest_rank(&[1, 2], 0), None);
        assert_eq!(nearest_rank(&[1, 2], 101), None);
    }

    #[test]
    fn the_budget_verdict_compares_p99_with_fifty_microseconds() {
        assert_eq!(budget_verdict(Some(8_000)), Some("pass"));
        assert_eq!(budget_verdict(Some(EVENT_P99_BUDGET_NS)), Some("pass"));
        assert_eq!(budget_verdict(Some(EVENT_P99_BUDGET_NS + 1)), Some("fail"));
        assert_eq!(budget_verdict(None), None);
    }

    #[test]
    fn the_call_payload_is_a_json_object_of_the_requested_size() {
        let payload = call_payload(4);
        assert_eq!(payload.as_str(), "{\"data\":\"xxxx\"}");
        assert_eq!(payload.as_str().len(), 4 + 11);
        assert_eq!(call_payload(DEFAULT_PAYLOAD_BYTES).as_bytes().len(), 1_035);
    }

    #[test]
    fn a_frame_carries_the_id_it_is_given() {
        let request = Request::call("measure", "echo", call_payload(4));
        let mut frame = encode(&request, 1).expect("an encoded frame");
        assert_eq!(frame[5..13], 1u64.to_le_bytes());

        set_id(&mut frame, 42);
        let read = Frame::read_from(&mut Cursor::new(&frame))
            .expect("a readable frame")
            .expect("a frame");
        assert_eq!(read.id(), 42);
        assert_eq!(read.kind(), Kind::Call);

        let decoded = Request::from_frame(&read).expect("a call");
        let Request::Call { payload, .. } = decoded else {
            panic!("a call, not {decoded:?}");
        };
        assert_eq!(payload.as_str(), "{\"data\":\"xxxx\"}");
    }

    #[test]
    fn an_exchange_returns_the_answer_that_carries_its_id() {
        let mut input = Cursor::new(answering(7, Response::Loaded));
        let mut output = Vec::new();
        let request = Request::Load {
            plugin: "measure".to_owned(),
            module: PathBuf::from("C:/plugins/echo.wasm"),
        };

        let response = exchange(&mut input, &mut output, 7, &request).expect("the answer");
        assert_eq!(response, Response::Loaded);

        let written = Frame::read_from(&mut Cursor::new(output))
            .expect("a readable frame")
            .expect("a frame");
        assert_eq!(written.kind(), Kind::Load);
        assert_eq!(written.id(), 7);
    }

    #[test]
    fn an_exchange_refuses_an_answer_to_another_id() {
        let mut input = Cursor::new(answering(8, Response::Loaded));
        let mut output = Vec::new();
        let request = Request::Unload {
            plugin: "measure".to_owned(),
        };

        let error = exchange(&mut input, &mut output, 7, &request).expect_err("a refused answer");
        assert!(
            matches!(
                error,
                ProbeError::UnexpectedId {
                    expected: 7,
                    got: 8
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn an_exchange_reports_a_helper_error_as_its_sentence() {
        let mut input = Cursor::new(answering(
            9,
            Response::Error {
                message: "the module trapped".to_owned(),
            },
        ));
        let mut output = Vec::new();
        let request = Request::call("measure", "echo", call_payload(4));

        let error = exchange(&mut input, &mut output, 9, &request).expect_err("a reported error");
        match error {
            ProbeError::Helper { id, message } => {
                assert_eq!(id, 9);
                assert_eq!(message, "the module trapped");
            }
            other => panic!("a helper error, not {other:?}"),
        }
    }

    #[test]
    fn an_exchange_reports_a_stream_that_ended() {
        let mut input = Cursor::new(Vec::new());
        let mut output = Vec::new();
        let request = Request::Unload {
            plugin: "measure".to_owned(),
        };

        let error = exchange(&mut input, &mut output, 7, &request).expect_err("a missing answer");
        assert!(matches!(error, ProbeError::EndOfStream), "{error:?}");
    }

    #[test]
    fn echo_must_return_the_call_payload_bytes() {
        let expected = call_payload(4);
        let answer = Response::reply(call_payload(4));
        check_echo(answer, expected.as_bytes(), 3).expect("the payload came back");

        let rewritten = Response::reply(call_payload(5));
        let error = check_echo(rewritten, expected.as_bytes(), 3).expect_err("other bytes");
        assert!(matches!(error, ProbeError::Message(_)), "{error:?}");

        let wrong_kind =
            check_echo(Response::Loaded, expected.as_bytes(), 3).expect_err("no reply");
        assert!(
            matches!(wrong_kind, ProbeError::UnexpectedKind { .. }),
            "{wrong_kind:?}"
        );
    }

    #[test]
    fn arguments_default_to_the_m0_probe() {
        let parsed = parse_args(arguments(&[
            "--helper",
            "target/measure/yapfast-plugins.exe",
            "--module",
            "C:/fixtures/echo.wasm",
        ]))
        .expect("parsed arguments")
        .expect("not a help request");

        assert_eq!(
            parsed.helper,
            PathBuf::from("target/measure/yapfast-plugins.exe")
        );
        assert_eq!(parsed.module, PathBuf::from("C:/fixtures/echo.wasm"));
        assert_eq!(parsed.plugin, "measure");
        assert_eq!(parsed.method, "echo");
        assert_eq!(parsed.warmup, DEFAULT_WARMUP);
        assert_eq!(parsed.requests, DEFAULT_REQUESTS);
        assert_eq!(parsed.payload_bytes, DEFAULT_PAYLOAD_BYTES);
        assert_eq!(parsed.helper_args, Vec::<String>::new());
        assert_eq!(parsed.timeout, Duration::from_secs(DEFAULT_TIMEOUT_SECONDS));
        assert_eq!(parsed.json_out, None);
        assert_eq!(parsed.run_label, None);
    }

    #[test]
    fn arguments_take_helper_arguments_in_order() {
        let parsed = parse_args(arguments(&[
            "--helper",
            "helper.exe",
            "--module",
            "C:/fixtures/echo.wasm",
            "--plugin",
            "echo",
            "--method",
            "shout",
            "--warmup",
            "2",
            "--requests",
            "30",
            "--payload-bytes",
            "64",
            "--helper-arg",
            "--no-cache",
            "--helper-arg",
            "--wasi",
            "--timeout",
            "3",
            "--json-out",
            "out.json",
            "--run-label",
            "warm-1",
        ]))
        .expect("parsed arguments")
        .expect("not a help request");

        assert_eq!(parsed.plugin, "echo");
        assert_eq!(parsed.method, "shout");
        assert_eq!(parsed.warmup, 2);
        assert_eq!(parsed.requests, 30);
        assert_eq!(parsed.payload_bytes, 64);
        assert_eq!(parsed.helper_args, vec!["--no-cache", "--wasi"]);
        assert_eq!(parsed.timeout, Duration::from_secs(3));
        assert_eq!(parsed.json_out, Some(PathBuf::from("out.json")));
        assert_eq!(parsed.run_label.as_deref(), Some("warm-1"));
    }

    #[test]
    fn help_is_not_an_error() {
        assert_eq!(
            parse_args(arguments(&["--help"])).expect("help is not a usage error"),
            None
        );
    }

    #[test]
    fn arguments_refuse_what_the_probe_cannot_use() {
        // A missing required option, an unknown option, a zero count, and an
        // option that promises a value it does not get.
        for broken in [
            arguments(&["--module", "C:/fixtures/echo.wasm"]),
            arguments(&["--helper", "helper.exe", "--module", "C:/f.wasm", "--nope"]),
            arguments(&[
                "--helper",
                "helper.exe",
                "--module",
                "C:/f.wasm",
                "--requests",
                "0",
            ]),
            arguments(&[
                "--helper",
                "helper.exe",
                "--module",
                "C:/f.wasm",
                "--payload-bytes",
                "0",
            ]),
            arguments(&[
                "--helper",
                "helper.exe",
                "--module",
                "C:/f.wasm",
                "--warmup",
            ]),
        ] {
            assert!(
                parse_args(broken.clone()).is_err(),
                "{broken:?} parsed as usable"
            );
        }
    }

    #[test]
    fn a_missing_or_relative_module_is_a_prerequisite_problem() {
        let helper = Path::new("C:/does-not-exist/yapfast-plugins.exe");
        assert!(prerequisite_problem(helper, Path::new("C:/does-not-exist/echo.wasm")).is_some());
        assert!(prerequisite_problem(Path::new("C:/fixtures/x"), Path::new("echo.wasm")).is_some());
    }

    #[test]
    fn a_timing_block_carries_its_samples_and_percentiles() {
        let block = timing(&[4, 8, 12, 16], "one exchange");
        assert_eq!(block["unit"], "nanoseconds");
        assert_eq!(block["definition"], "one exchange");
        assert_eq!(block["samples"], json!([4, 8, 12, 16]));
        assert_eq!(
            block["percentiles"],
            json!({"p50": 8, "p95": 16, "p99": 16})
        );
        assert_eq!(timing(&[], "one exchange")["percentiles"], Value::Null);
    }

    #[test]
    fn the_stderr_tail_keeps_what_the_helper_said() {
        assert_eq!(
            drain_stderr(Cursor::new(b"the module trapped".to_vec())),
            b"the module trapped"
        );
    }

    #[test]
    fn the_stderr_tail_is_bounded_to_its_documented_size() {
        let long: Vec<u8> = vec![b'e'; STDERR_TAIL + 100];
        let tail = drain_stderr(Cursor::new(long));
        assert_eq!(tail.len(), STDERR_TAIL);
        assert!(tail.iter().all(|byte| *byte == b'e'));
    }
}
