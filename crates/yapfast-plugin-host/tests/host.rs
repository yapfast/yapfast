//! The protocol driven by Extism itself: a real module is loaded, called and
//! unloaded, and the settings the plan fixes are applied.
//!
//! The fixtures are assembled from `.wat` in `tests/fixtures`. Only this file's
//! helper does that; production reads WebAssembly binaries.

mod support;

use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use yapfast_plugin_api::{Frame, Kind, Request, Response};
use yapfast_plugin_host::protocol::{self, Outcome};
use yapfast_plugin_host::runtime::{CacheConfig, CacheError, Config, Runtime};

fn load(plugin: &str, module: &Path, id: u64) -> Vec<u8> {
    request(
        &Request::Load {
            plugin: plugin.to_owned(),
            module: module.to_path_buf(),
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

fn request(request: &Request, id: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    request
        .to_frame(id)
        .expect("request frame")
        .write_to(&mut bytes)
        .expect("write request");
    bytes
}

fn run(runtime: &mut Runtime, requests: Vec<Vec<u8>>) -> (Outcome, Vec<Response>) {
    let mut input = Cursor::new(requests.concat());
    let mut output = Vec::new();
    let outcome = protocol::serve(&mut input, &mut output, runtime).expect("serve");
    (outcome, answers(&output))
}

fn answers(bytes: &[u8]) -> Vec<Response> {
    let mut reader = Cursor::new(bytes);
    let mut answers = Vec::new();
    while let Some(frame) = Frame::read_from(&mut reader).expect("a readable frame") {
        assert_ne!(frame.kind(), Kind::Ready, "ready comes once, first");
        answers.push(Response::from_frame(&frame).expect("a response frame"));
    }
    answers
}

/// The JSON a TOML basic string carries for `text`: backslashes and quotes
/// escaped, which is all a Windows path needs.
fn toml_string(text: &str) -> String {
    let mut escaped = String::new();
    for character in text.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            other => escaped.push(other),
        }
    }
    escaped
}

#[test]
fn the_defaults_are_the_plans() {
    let config = Config::default();
    assert_eq!(config.max_pages, None, "None applies 256 pages");
    assert!(!config.wasi, "no WASI unless asked");
    assert_eq!(config.cache, None, "no cache unless asked");
}

#[test]
fn echo_loads_calls_and_unloads() {
    let (_dir, module) = support::wasm_fixture("echo.wat");
    let mut runtime = Runtime::new(Config::default()).expect("runtime");
    let payload = json!({"text": "hi", "n": [1, 2]});

    let (outcome, answers) = run(
        &mut runtime,
        vec![
            load("echo", &module, 1),
            call("echo", "echo", payload.clone(), 2),
            unload("echo", 3),
        ],
    );

    assert_eq!(outcome, Outcome::Idle);
    assert_eq!(
        answers,
        vec![
            Response::Loaded,
            Response::Reply { payload },
            Response::Unloaded,
        ]
    );
}

#[test]
fn a_text_module_never_reaches_extism() {
    let (_dir, module) = support::text_fixture("echo.wat");
    let mut runtime = Runtime::new(Config::default()).expect("runtime");

    let (_, answers) = run(&mut runtime, vec![load("echo", &module, 1)]);

    match &answers[0] {
        Response::Error { message } => assert!(
            message.contains("WebAssembly"),
            "the message says the file is not a binary: {message}"
        ),
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn an_export_the_module_lacks_is_an_error() {
    let (_dir, module) = support::wasm_fixture("echo.wat");
    let mut runtime = Runtime::new(Config::default()).expect("runtime");

    let (_, answers) = run(
        &mut runtime,
        vec![
            load("echo", &module, 1),
            call("echo", "shout", Value::Null, 2),
            unload("echo", 3),
        ],
    );

    match &answers[1] {
        Response::Error { message } => {
            assert!(message.contains("shout"), "names the export: {message}")
        }
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn loading_the_same_id_twice_is_an_error() {
    let (_dir, module) = support::wasm_fixture("echo.wat");
    let mut runtime = Runtime::new(Config::default()).expect("runtime");

    let (_, answers) = run(
        &mut runtime,
        vec![
            load("echo", &module, 1),
            load("echo", &module, 2),
            unload("echo", 3),
        ],
    );

    assert_eq!(answers[0], Response::Loaded);
    assert!(
        matches!(&answers[1], Response::Error { message } if message.contains("already loaded")),
        "expected an already-loaded error, got {:?}",
        answers[1]
    );
    assert_eq!(answers[2], Response::Unloaded);
}

#[test]
fn an_unload_of_something_not_loaded_is_an_error() {
    let mut runtime = Runtime::new(Config::default()).expect("runtime");
    let (_, answers) = run(&mut runtime, vec![unload("ghost", 1)]);
    assert!(
        matches!(&answers[0], Response::Error { message } if message.contains("ghost")),
        "expected an error naming the plugin, got {:?}",
        answers[0]
    );
}

#[test]
fn a_cache_config_file_is_written_where_wasmtime_reads_it() {
    // wasmtime takes the path of a TOML file, never a directory, and the
    // directory inside it must be absolute.
    let (_dir, module) = support::wasm_fixture("echo.wat");
    let data = tempfile::tempdir().expect("temp dir");
    let cache_dir = data.path().join("cache");
    let config_file = data.path().join("wasmtime-cache.toml");
    let cache = CacheConfig::new(&cache_dir, &config_file).expect("cache config");
    let config = Config {
        cache: Some(cache),
        ..Config::default()
    };
    let mut runtime = Runtime::new(config).expect("runtime writes the cache file");

    let written = std::fs::read_to_string(&config_file).expect("the cache file exists");
    assert_eq!(
        written,
        format!(
            "[cache]\ndirectory = \"{}\"\n",
            toml_string(&cache_dir.display().to_string())
        )
    );
    assert!(cache_dir.is_absolute(), "wasmtime wants an absolute path");

    // The real check: wasmtime parses that file and compiles through it.
    let (outcome, answers) = run(
        &mut runtime,
        vec![load("echo", &module, 1), unload("echo", 2)],
    );
    assert_eq!(answers, vec![Response::Loaded, Response::Unloaded]);
    assert_eq!(outcome, Outcome::Idle);
}

#[test]
fn a_guest_that_grows_past_the_page_limit_fails_the_call() {
    // The default is 256 pages: the guest asks for more, and Extism's limiter
    // refuses the grow. Hitting the cap is a plugin fault, not a helper fault.
    let (_dir, module) = support::wasm_fixture("oom.wat");
    let mut runtime = Runtime::new(Config::default()).expect("runtime");

    let (_, answers) = run(
        &mut runtime,
        vec![
            load("oom", &module, 1),
            call("oom", "eat_memory", Value::Null, 2),
            unload("oom", 3),
        ],
    );

    assert_eq!(answers[0], Response::Loaded);
    match &answers[1] {
        Response::Error { message } => assert!(
            message.contains("oom"),
            "Extism classifies the cap as oom: {message}"
        ),
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn a_relative_cache_directory_is_refused() {
    let data = tempfile::tempdir().expect("temp dir");
    let error = CacheConfig::new(Path::new("cache"), data.path().join("cache.toml"))
        .expect_err("wasmtime refuses a relative directory");
    assert!(
        matches!(&error, CacheError::RelativeDirectory(path) if path == Path::new("cache")),
        "expected RelativeDirectory, got {error:?}"
    );
}

#[test]
fn a_relative_cache_file_is_refused() {
    let data = tempfile::tempdir().expect("temp dir");
    let error = CacheConfig::new(data.path().join("cache"), PathBuf::from("cache.toml"))
        .expect_err("the config file is named absolutely too");
    assert!(
        matches!(error, CacheError::RelativeFile(_)),
        "expected RelativeFile, got {error:?}"
    );
}
