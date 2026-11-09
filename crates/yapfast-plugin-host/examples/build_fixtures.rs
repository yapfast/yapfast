//! Assembles the checked-in WAT fixtures into `.wasm` files for the measurement
//! harness.
//!
//! `testing/measure_plugins.py` speaks the wire protocol and cannot run a WAT
//! compiler, so it needs real module files on disk. This example is how they
//! are made:
//!
//! ```sh
//! cargo run --profile fast -p yapfast-plugin-host --example build_fixtures
//! ```
//!
//! It writes `target/plugin-fixtures/echo.wasm` and `oom.wasm`, the paths
//! `docs/plugin-measurements.md` passes as `--module`. Given a directory as its
//! argument, it writes there instead.
//!
//! This is a harness step, not part of the helper: production reads WebAssembly
//! binaries only, and nothing here is linked into `yapfast-plugins`.

use std::path::{Path, PathBuf};

/// The fixtures the harness runs.
const FIXTURES: [&str; 2] = ["echo", "oom"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .expect("the crate sits two levels under the repository root");
    let source = root.join("testing/plugins");
    let output = match std::env::args().nth(1) {
        Some(directory) => PathBuf::from(directory),
        None => root.join("target/plugin-fixtures"),
    };
    std::fs::create_dir_all(&output)?;

    for name in FIXTURES {
        let wat = source.join(format!("{name}.wat"));
        let bytes = wat::parse_file(&wat)?;
        let path = output.join(format!("{name}.wasm"));
        std::fs::write(&path, bytes)?;
        println!("{}", path.display());
    }
    Ok(())
}
