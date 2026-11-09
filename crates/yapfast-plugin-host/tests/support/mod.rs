//! Helpers the helper's tests share: a module fixture and the bytes to load it
//! from.
//!
//! A fixture is checked in as `.wat`, because nobody should need a wasm
//! toolchain to review or run these tests. It is assembled here, in the test
//! process, into a temporary `.wasm` that production's loader reads like any
//! other module.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Assembles `tests/fixtures/<name>` into a temporary `.wasm`.
///
/// Keep the returned directory alive: dropping it deletes the module.
pub fn wasm_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let wat = fixture_path(name);
    let bytes = wat::parse_file(&wat).unwrap_or_else(|error| panic!("{}: {error}", wat.display()));
    write_fixture(&bytes, "fixture.wasm")
}

/// Copies `tests/fixtures/<name>` verbatim into a temporary `.wasm`, so a test
/// can hand production a file that is not a WebAssembly binary.
pub fn text_fixture(name: &str) -> (tempfile::TempDir, PathBuf) {
    let bytes = std::fs::read(fixture_path(name)).expect("read fixture");
    write_fixture(&bytes, "fixture.wasm")
}

/// The path of a checked-in fixture. The guest fixtures live beside the other
/// test fixtures, in `testing/plugins`, rather than in this crate.
pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testing/plugins")
        .join(name)
}

fn write_fixture(bytes: &[u8], file: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(file);
    std::fs::write(&path, bytes).expect("write fixture");
    (dir, path)
}
