//! The `yapfast-plugins` binary itself: real pipes, a real module, and the
//! process lifetime the parent relies on.
//!
//! The helper is disposable. It exits after the unload of its last instance,
//! and the parent spawns a fresh one on the next trigger, so what these tests
//! check is that the process really ends, cleanly, once it holds nothing.

#![cfg(feature = "host")]

mod support;

use std::io::{BufReader, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;

use yapfast_plugin_api::{Frame, Request, Response};

fn request(request: &Request, id: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    request
        .to_frame(id)
        .expect("request frame")
        .write_to(&mut bytes)
        .expect("write request");
    bytes
}

fn read_frame(reader: &mut impl Read) -> Frame {
    Frame::read_from(reader)
        .expect("a readable frame")
        .expect("a frame, not the end of the stream")
}

fn read_response(reader: &mut impl Read) -> Response {
    let frame = read_frame(reader);
    Response::from_frame(&frame).expect("a response frame")
}

/// Waits for the child, killing it rather than hanging a test run.
fn wait(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            return status;
        }
        if Instant::now() > deadline {
            child.kill().expect("kill the helper");
            panic!("the helper did not exit within 20 s");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn spawn() -> Child {
    Command::new(env!("CARGO_BIN_EXE_yapfast-plugins"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the helper")
}

#[test]
fn the_helper_loads_calls_and_exits_after_its_last_unload() {
    let (_dir, module) = support::wasm_fixture("echo.wat");
    let mut child = spawn();
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    assert_eq!(read_response(&mut stdout), Response::Ready);

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

    assert_eq!(read_response(&mut stdout), Response::Loaded);
    let reply = read_response(&mut stdout);
    match &reply {
        Response::Reply { payload: answered } => {
            assert_eq!(answered.to_value().expect("the reply is JSON"), payload)
        }
        other => panic!("expected a reply, got {other:?}"),
    }
    assert_eq!(read_response(&mut stdout), Response::Unloaded);

    // The last unload was the helper's last work: the stream ends, and the
    // process exits cleanly, whether or not the parent closes stdin first.
    assert!(
        Frame::read_from(&mut stdout)
            .expect("end of stream")
            .is_none(),
        "the helper wrote nothing after its last unload"
    );
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
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    assert_eq!(read_response(&mut stdout), Response::Ready);
    stdin
        .write_all(&request(&Request::Shutdown, 1))
        .expect("write shutdown");

    assert!(
        Frame::read_from(&mut stdout)
            .expect("end of stream")
            .is_none(),
        "shutdown has no answer"
    );
    let status = wait(&mut child);
    assert!(status.success(), "exit status {status}");
}
