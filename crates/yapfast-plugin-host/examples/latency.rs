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

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitCode, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use yapfast_plugin_api::{Frame, Kind, Payload, ProtocolError, ReadError, Request, Response};

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

fn main() -> ExitCode {
    todo!()
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
    todo!()
}

/// Why a prerequisite the probe cannot invent is missing.
fn prerequisite_problem(helper: &Path, module: &Path) -> Option<String> {
    todo!()
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
    todo!()
}

/// p50, p95 and p99 of the samples, or `None` for none.
fn percentiles(values: &[u64]) -> Option<Percentiles> {
    todo!()
}

/// The plan's verdict on a measured p99, or `None` without one.
fn budget_verdict(p99: Option<u64>) -> Option<&'static str> {
    todo!()
}

/// The call payload: one JSON object whose `data` string is `bytes` long, so
/// the frame carries `bytes` + 11 bytes of JSON, as the Python harness builds.
fn call_payload(bytes: usize) -> Payload {
    todo!()
}

/// Encodes one request as the frame to send under `id`.
fn encode(request: &Request, id: u64) -> Result<Vec<u8>, ProbeError> {
    todo!()
}

/// Writes an id into an already encoded frame's header, in place.
///
/// The measured loop reuses one encoded `call` frame and changes only its id,
/// so no JSON encoding runs inside the span being timed.
fn set_id(frame: &mut [u8], id: u64) {
    todo!()
}

/// Writes one encoded frame and flushes it: a pipe has no autoflush.
fn write_encoded(output: &mut impl Write, frame: &[u8]) -> Result<(), ProbeError> {
    todo!()
}

/// Reads the one response that answers `id`, refusing one that answers another.
fn read_reply(input: &mut impl Read, id: u64) -> Result<Response, ProbeError> {
    todo!()
}

/// Writes one request under `id` and returns the response that answers it.
fn exchange(
    input: &mut impl Read,
    output: &mut impl Write,
    id: u64,
    request: &Request,
) -> Result<Response, ProbeError> {
    todo!()
}

/// Checks that a call's answer is a `reply` carrying the payload's exact bytes.
fn check_echo(response: Response, expected: &[u8], id: u64) -> Result<(), ProbeError> {
    todo!()
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
    Exited {
        code: Option<i32>,
        stderr: String,
    },
    /// Anything else, as a sentence.
    Message(String),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
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

/// Spawns the helper, measures it, and returns the run's JSON record.
fn measure(args: &Args) -> Result<Value, ProbeError> {
    todo!()
}

/// One timing block: the definition, the samples and their percentiles.
fn timing(samples: &[u64], definition: &str) -> Value {
    todo!()
}

/// Keeps the helper's stderr out of the protocol stream and its bytes out of
/// the report, while still telling a failed run what the helper said.
fn drain_stderr(mut pipe: impl Read) -> Vec<u8> {
    todo!()
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

        let wrong_kind = check_echo(Response::Loaded, expected.as_bytes(), 3).expect_err("no reply");
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

        assert_eq!(parsed.helper, PathBuf::from("target/measure/yapfast-plugins.exe"));
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
            arguments(&["--helper", "helper.exe", "--module", "C:/f.wasm", "--warmup"]),
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
        assert!(
            prerequisite_problem(Path::new("C:/fixtures/x"), Path::new("echo.wasm")).is_some()
        );
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
