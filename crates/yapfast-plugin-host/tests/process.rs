//! The `yapfast-plugins` binary itself: real pipes, a real module, the process
//! lifetime the parent relies on, and the environment it must not inherit.
//!
//! The helper is disposable. It exits after the unload of its last instance,
//! and the parent spawns a fresh one on the next trigger, so what these tests
//! check is that the process really ends, cleanly, once it holds nothing.
//!
//! Every read from the helper runs on its own thread with a deadline. A pipe
//! read has no timeout of its own, so a helper that stops answering would
//! otherwise hang the whole test run instead of failing one test.

#![cfg(feature = "host")]

mod support;

use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::json;

use yapfast_plugin_api::{Frame, Payload, Request, Response};

/// How long the helper may take to answer, or to end its stream.
const WAIT: Duration = Duration::from_secs(20);

fn request(request: &Request, id: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    request
        .to_frame(id)
        .expect("request frame")
        .write_to(&mut bytes)
        .expect("write request");
    bytes
}

/// Waits for the child, killing it rather than hanging a test run.
fn wait(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return status;
        }
        if Instant::now() > deadline {
            child.kill().expect("kill the helper");
            panic!("the helper did not exit within {WAIT:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn spawn() -> Child {
    spawn_with(&[], &[])
}

/// Spawns the helper with arguments and extra environment variables. The rest
/// of the environment is inherited, so a test can hand the helper a variable
/// that the parent's `env_clear()` would have taken away.
fn spawn_with(arguments: &[&str], environment: &[(&str, &str)]) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_yapfast-plugins"));
    command
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in environment {
        command.env(name, value);
    }
    command.spawn().expect("spawn the helper")
}

/// One item the reader thread took from the helper's stdout.
enum Item {
    Frame(Frame),
    /// The helper closed its stream, cleanly and between frames.
    End,
    /// The bytes were not a frame: something else wrote to the stream.
    Unreadable(String),
}

/// The helper's stdout, decoded into frames on a thread of its own so every
/// read has a deadline.
struct Answers {
    items: Receiver<Item>,
}

impl Answers {
    fn of(child: &mut Child) -> Self {
        let mut stdout = child.stdout.take().expect("stdout");
        let (sender, items) = mpsc::channel();
        std::thread::spawn(move || {
            loop {
                let (item, done) = match Frame::read_from(&mut stdout) {
                    Ok(Some(frame)) => (Item::Frame(frame), false),
                    Ok(None) => (Item::End, true),
                    Err(error) => (Item::Unreadable(error.to_string()), true),
                };
                if sender.send(item).is_err() || done {
                    return;
                }
            }
        });
        Self { items }
    }

    fn response(&self) -> Response {
        match self.items.recv_timeout(WAIT) {
            Ok(Item::Frame(frame)) => Response::from_frame(&frame).expect("a response frame"),
            Ok(Item::End) => panic!("the helper closed its stream instead of answering"),
            Ok(Item::Unreadable(error)) => panic!("the stream was not frames: {error}"),
            Err(error) => panic!("no answer within {WAIT:?}: {error}"),
        }
    }

    fn expect_end(&self) {
        match self.items.recv_timeout(WAIT) {
            Ok(Item::End) => {}
            Ok(Item::Frame(frame)) => panic!("the helper wrote another frame: {frame:?}"),
            Ok(Item::Unreadable(error)) => panic!("the stream was not frames: {error}"),
            Err(error) => panic!("the stream did not end within {WAIT:?}: {error}"),
        }
    }
}

#[test]
fn the_helper_loads_calls_and_exits_after_its_last_unload() {
    let (_dir, module) = support::wasm_fixture("echo.wat");
    let mut child = spawn();
    let mut stdin = child.stdin.take().expect("stdin");
    let answers = Answers::of(&mut child);

    assert_eq!(answers.response(), Response::Ready);

    let payload = json!({"text": "hi", "n": [1, 2]});
    let requests = [
        request(
            &Request::Load {
                plugin: "echo".to_owned(),
                module: module.clone(),
            },
            1,
        ),
        request(&Request::call("echo", "echo", payload.clone()), 2),
        request(
            &Request::Unload {
                plugin: "echo".to_owned(),
            },
            3,
        ),
    ];
    for bytes in requests {
        stdin.write_all(&bytes).expect("write request");
    }
    stdin.flush().expect("flush");

    assert_eq!(answers.response(), Response::Loaded);
    match answers.response() {
        Response::Reply { payload: answered } => {
            assert_eq!(answered.to_value().expect("the reply is JSON"), payload)
        }
        other => panic!("expected a reply, got {other:?}"),
    }
    assert_eq!(answers.response(), Response::Unloaded);

    // The last unload was the helper's last work: the stream ends, and the
    // process exits cleanly, whether or not the parent closes stdin first.
    answers.expect_end();
    let status = wait(&mut child);
    assert!(status.success(), "exit status {status}");

    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut stderr)
        .expect("read stderr");
    assert!(
        stderr.is_empty(),
        "a healthy run says nothing on stderr: {stderr}"
    );
}

#[test]
fn shutdown_ends_the_helper_without_an_answer() {
    let mut child = spawn();
    let mut stdin = child.stdin.take().expect("stdin");
    let answers = Answers::of(&mut child);

    assert_eq!(answers.response(), Response::Ready);
    stdin
        .write_all(&request(&Request::Shutdown, 1))
        .expect("write shutdown");

    answers.expect_end();
    let status = wait(&mut child);
    assert!(status.success(), "exit status {status}");
}

#[test]
fn a_guest_stdout_cannot_write_into_the_frame_stream() {
    // Extism forwards a guest's WASI stdout to the helper's own stdout when
    // `EXTISM_ENABLE_WASI_OUTPUT` is set, and the helper's stdout is the frame
    // stream: one byte of noise there and the parent cannot parse a frame. The
    // parent spawns the helper with a cleared environment, but the measurement
    // harness and a person at a terminal do not, so the helper drops every
    // `EXTISM_*` variable itself before Extism reads one.
    let (_dir, module) = support::wasm_fixture("wasi_stdout.wat");
    let mut child = spawn_with(&["--wasi"], &[("EXTISM_ENABLE_WASI_OUTPUT", "1")]);
    let mut stdin = child.stdin.take().expect("stdin");
    let answers = Answers::of(&mut child);

    assert_eq!(answers.response(), Response::Ready);
    for bytes in [
        request(
            &Request::Load {
                plugin: "noise".to_owned(),
                module,
            },
            1,
        ),
        request(&Request::call("noise", "speak", serde_json::Value::Null), 2),
        request(
            &Request::Unload {
                plugin: "noise".to_owned(),
            },
            3,
        ),
    ] {
        stdin.write_all(&bytes).expect("write request");
    }
    stdin.flush().expect("flush");

    assert_eq!(
        answers.response(),
        Response::Loaded,
        "the guest's own stdout is not part of the stream"
    );
    assert_eq!(
        answers.response(),
        Response::Reply {
            payload: Payload::null()
        }
    );
    assert_eq!(answers.response(), Response::Unloaded);
    answers.expect_end();
    let status = wait(&mut child);
    assert!(status.success(), "exit status {status}");
}
