//! The protocol driven by Extism itself: a real module is loaded, called and
//! unloaded, and the settings the plan fixes are applied.
//!
//! The fixtures are assembled from `.wat` in `testing/plugins`. Only the test
//! support and `examples/build_fixtures.rs` do that; production reads
//! WebAssembly binaries.

#![cfg(feature = "host")]

mod support;

use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use yapfast_plugin_api::{Frame, Request, Response};
use yapfast_plugin_host::protocol::{self, Outcome};
use yapfast_plugin_host::runtime::{Cache, CacheConfig, CacheError, Config, Runtime};

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
    request(&Request::call(plugin, method, payload), id)
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
    let mut answers = answers(&output);
    assert_eq!(
        answers.first(),
        Some(&Response::Ready),
        "ready comes first, once: {answers:?}"
    );
    answers.remove(0);
    (outcome, answers)
}

/// A reply's JSON, for comparing what a guest answered rather than its bytes.
fn reply_json(response: &Response) -> Value {
    match response {
        Response::Reply { payload } => payload.to_value().expect("the reply is JSON"),
        other => panic!("expected a reply, got {other:?}"),
    }
}

fn answers(bytes: &[u8]) -> Vec<Response> {
    let mut reader = Cursor::new(bytes);
    let mut answers = Vec::new();
    while let Some(frame) = Frame::read_from(&mut reader).expect("a readable frame") {
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
    assert_eq!(config.cache, Cache::Fallback, "no cache configured");
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
    assert_eq!(answers.len(), 3, "loaded, reply, unloaded: {answers:?}");
    assert_eq!(answers[0], Response::Loaded);
    assert_eq!(reply_json(&answers[1]), payload);
    assert_eq!(answers[2], Response::Unloaded);
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
        cache: Cache::File(cache),
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
fn a_disabled_cache_still_loads_a_module() {
    // `--no-cache` is what the measurement harness asks for when it times a
    // cold compile, so the path has to work, not just be documented.
    let (_dir, module) = support::wasm_fixture("echo.wat");
    let config = Config {
        cache: Cache::Disabled,
        ..Config::default()
    };
    let mut runtime = Runtime::new(config).expect("runtime");

    let (outcome, answers) = run(
        &mut runtime,
        vec![load("echo", &module, 1), unload("echo", 2)],
    );

    assert_eq!(answers, vec![Response::Loaded, Response::Unloaded]);
    assert_eq!(outcome, Outcome::Idle);
}

#[test]
fn two_instances_of_one_module_load_call_and_unload_independently() {
    let (_first, one) = support::wasm_fixture("echo.wat");
    let (_second, two) = support::wasm_fixture("echo.wat");
    let mut runtime = Runtime::new(Config::default()).expect("runtime");

    let (outcome, answers) = run(
        &mut runtime,
        vec![
            load("one", &one, 1),
            load("two", &two, 2),
            call("one", "echo", json!({"n": 1}), 3),
            call("two", "echo", json!({"n": 2}), 4),
            unload("one", 5),
            call("two", "echo", json!({"n": 3}), 6),
            unload("two", 7),
        ],
    );

    assert_eq!(outcome, Outcome::Idle, "the second unload ends the helper");
    assert_eq!(answers.len(), 7, "one answer per request: {answers:?}");
    assert_eq!(reply_json(&answers[2]), json!({"n": 1}));
    assert_eq!(reply_json(&answers[3]), json!({"n": 2}));
    assert_eq!(answers[4], Response::Unloaded);
    assert_eq!(
        reply_json(&answers[5]),
        json!({"n": 3}),
        "the other instance still answers after the first is unloaded"
    );
    assert_eq!(answers[6], Response::Unloaded);
}

#[test]
fn the_helper_cache_layout_keeps_its_config_file() {
    // wasmtime deletes files it does not recognise from its cache directory,
    // so the config file has to live beside it. This is the regression guard:
    // with the file inside, wasmtime removed it during the first load.
    let (_dir, module) = support::wasm_fixture("echo.wat");
    let root = tempfile::tempdir().expect("temp dir");
    let cache = CacheConfig::under(root.path()).expect("cache config");
    let config_file = cache.file().to_path_buf();
    let mut runtime = Runtime::new(Config {
        cache: Cache::File(cache),
        ..Config::default()
    })
    .expect("runtime");

    let (_, answers) = run(
        &mut runtime,
        vec![load("echo", &module, 1), unload("echo", 2)],
    );
    assert_eq!(answers, vec![Response::Loaded, Response::Unloaded]);
    assert!(
        config_file.is_file(),
        "the config file survives wasmtime's cache cleanup"
    );
    assert!(
        root.path().join("cache").is_dir(),
        "wasmtime caches compiled modules under <dir>/cache"
    );
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
