//! The loop over the framed protocol, with a backend that stands in for Extism.
//!
//! These tests pin the helper's side of `docs/plugin-protocol.md`: `ready`
//! first, an answer to every request, a clean end of stream, `shutdown` without
//! an answer, the exit after the last unload, and the errors that are answered
//! rather than fatal.

mod support;

use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use yapfast_plugin_api::{Frame, Kind, ReadError, Request, Response};
use yapfast_plugin_host::protocol::{self, Backend, Outcome, ServeError};

/// A backend with no engine: it remembers what is loaded and echoes what it is
/// given, which is all the loop needs to know.
#[derive(Debug, Default)]
struct Fake {
    loaded: Vec<String>,
}

impl Backend for Fake {
    fn load(&mut self, plugin: &str, module: &Path) -> Result<(), String> {
        if self.loaded.iter().any(|id| id == plugin) {
            return Err(format!("plugin {plugin} is already loaded"));
        }
        if !module.is_absolute() {
            return Err(format!("module {} is not absolute", module.display()));
        }
        self.loaded.push(plugin.to_owned());
        Ok(())
    }

    fn call(&mut self, plugin: &str, method: &str, payload: &[u8]) -> Result<Vec<u8>, String> {
        if !self.loaded.iter().any(|id| id == plugin) {
            return Err(format!("plugin {plugin} is not loaded"));
        }
        match method {
            "echo" => Ok(payload.to_vec()),
            // An export that sets no output: Extism hands back no bytes.
            "silent" => Ok(Vec::new()),
            other => Err(format!("Function not found: {other}")),
        }
    }

    fn unload(&mut self, plugin: &str) -> Result<(), String> {
        match self.loaded.iter().position(|id| id == plugin) {
            Some(index) => {
                self.loaded.remove(index);
                Ok(())
            }
            None => Err(format!("plugin {plugin} is not loaded")),
        }
    }

    fn instances(&self) -> usize {
        self.loaded.len()
    }
}

/// The module path a `load` names. The fake only checks that it is absolute.
fn module() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/echo.wat")
}

fn load(plugin: &str, id: u64) -> Vec<u8> {
    request(
        &Request::Load {
            plugin: plugin.to_owned(),
            module: module(),
        },
        id,
    )
}

fn call(plugin: &str, method: &str, payload: Value, id: u64) -> Vec<u8> {
    request(
        &Request::Call {
            plugin: plugin.to_owned(),
            method: method.to_owned(),
            payload,
        },
        id,
    )
}

fn unload(plugin: &str, id: u64) -> Vec<u8> {
    request(
        &Request::Unload {
            plugin: plugin.to_owned(),
        },
        id,
    )
}

fn shutdown(id: u64) -> Vec<u8> {
    request(&Request::Shutdown, id)
}

fn request(request: &Request, id: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    request
        .to_frame(id)
        .expect("request frame")
        .write_to(&mut bytes)
        .expect("write request");
    bytes
}

fn run(requests: Vec<Vec<u8>>) -> (Result<Outcome, ServeError>, Vec<(Kind, u64, Response)>) {
    let mut backend = Fake::default();
    let mut input = Cursor::new(requests.concat());
    let mut output = Vec::new();
    let outcome = protocol::serve(&mut input, &mut output, &mut backend);
    (outcome, answers(&output))
}

/// Every response the helper wrote, in order.
fn answers(bytes: &[u8]) -> Vec<(Kind, u64, Response)> {
    let mut reader = Cursor::new(bytes);
    let mut answers = Vec::new();
    while let Some(frame) = Frame::read_from(&mut reader).expect("a readable frame") {
        let response = Response::from_frame(&frame).expect("a response frame");
        answers.push((frame.kind(), frame.id(), response));
    }
    answers
}

#[test]
fn ready_is_the_first_frame_and_a_clean_end_of_stream_stops_the_helper() {
    let (outcome, answers) = run(Vec::new());

    assert_eq!(outcome.expect("clean stop"), Outcome::EndOfStream);
    assert_eq!(answers.len(), 1, "nothing but ready: {answers:?}");
    assert_eq!(answers[0].0, Kind::Ready);
    assert_eq!(answers[0].1, 0);
    assert_eq!(answers[0].2, Response::Ready);
}

#[test]
fn shutdown_ends_the_loop_without_an_answer() {
    let (outcome, answers) = run(vec![shutdown(7)]);

    assert_eq!(outcome.expect("shutdown"), Outcome::Shutdown);
    assert_eq!(answers.len(), 1, "ready only: {answers:?}");
}

#[test]
fn load_call_and_unload_round_trip() {
    let payload = json!({"text": "hi", "n": [1, 2]});
    let (outcome, answers) = run(vec![
        load("echo", 1),
        call("echo", "echo", payload.clone(), 2),
        unload("echo", 3),
    ]);

    assert_eq!(
        outcome.expect("the last unload ends the helper"),
        Outcome::Idle
    );
    assert_eq!(
        answers,
        vec![
            (Kind::Ready, 0, Response::Ready),
            (Kind::Loaded, 1, Response::Loaded),
            (Kind::Reply, 2, Response::Reply { payload }),
            (Kind::Unloaded, 3, Response::Unloaded),
        ]
    );
}

#[test]
fn the_last_unload_alone_ends_the_helper() {
    let (outcome, answers) = run(vec![load("echo", 1), unload("echo", 2)]);

    assert_eq!(outcome.expect("idle after the last instance"), Outcome::Idle);
    assert_eq!(answers.len(), 3, "ready, loaded, unloaded: {answers:?}");
}

#[test]
fn unloading_one_of_two_plugins_does_not_end_the_helper() {
    let (outcome, answers) = run(vec![
        load("a", 1),
        load("b", 2),
        unload("a", 3),
        call("b", "echo", json!("still here"), 4),
        unload("b", 5),
    ]);

    assert_eq!(outcome.expect("idle after b"), Outcome::Idle);
    assert_eq!(
        answers[3],
        (
            Kind::Reply,
            4,
            Response::Reply {
                payload: json!("still here")
            }
        ),
        "b still answers after a is unloaded: {answers:?}"
    );
}

#[test]
fn a_call_to_an_unloaded_plugin_is_answered_with_an_error() {
    let (outcome, answers) = run(vec![call("ghost", "echo", Value::Null, 9), shutdown(10)]);

    let error = match &answers[1].2 {
        Response::Error { message } => message.clone(),
        other => panic!("expected an error, got {other:?}"),
    };
    assert!(
        error.contains("ghost"),
        "the message names the plugin: {error}"
    );
    assert_eq!(answers[1].0, Kind::Error);
    assert_eq!(answers[1].1, 9, "the answer carries the request's id");
    assert_eq!(outcome.expect("shutdown"), Outcome::Shutdown);
}

#[test]
fn an_export_that_answers_nothing_replies_null() {
    let (_, answers) = run(vec![
        load("echo", 1),
        call("echo", "silent", Value::Null, 2),
        unload("echo", 3),
    ]);

    assert_eq!(answers[2].2, Response::Reply { payload: Value::Null });
}

#[test]
fn a_frame_whose_json_is_not_its_kind_is_an_error_and_the_stream_continues() {
    // A call frame carrying a shutdown envelope: the frame's end is known, so
    // the helper can answer and read on.
    let mismatched = Frame::new(Kind::Call, 4, br#"{"type":"shutdown"}"#.to_vec()).expect("frame");
    let mut bytes = Vec::new();
    mismatched.write_to(&mut bytes).expect("write");

    let (outcome, answers) = run(vec![bytes, shutdown(5)]);

    assert_eq!(answers[1].0, Kind::Error);
    assert_eq!(answers[1].1, 4);
    assert_eq!(outcome.expect("shutdown"), Outcome::Shutdown);
}

#[test]
fn a_truncated_frame_ends_the_helper() {
    let mut bytes = shutdown(6);
    bytes.truncate(bytes.len() - 2);
    let (outcome, answers) = run(vec![bytes]);

    assert!(
        matches!(
            outcome.expect_err("a frame that ended early is fatal"),
            ServeError::Read(ReadError::UnexpectedEof { .. })
        ),
        "expected UnexpectedEof"
    );
    assert_eq!(answers.len(), 1, "ready is all that was written");
}
