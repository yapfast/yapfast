//! What a module file must be before Extism is given it.
//!
//! Extism accepts more than WebAssembly binaries: it sniffs text for the WAT it
//! compiles, and wasmtime compiles WAT from a file just as readily as from
//! memory. A `.wasm` in the content store that really holds `(module …)` would
//! therefore load, so the magic and version below are the only thing that
//! rejects a text module. They are checked here, on the file's own bytes,
//! before [`extism::Wasm::file`] ever names it.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// The largest module the helper loads, in bytes (§6.4).
pub const MAX_MODULE_BYTES: u64 = 8 * 1024 * 1024;

/// The WebAssembly binary magic, `\0asm`.
pub const MAGIC: [u8; 4] = *b"\0asm";

/// The WebAssembly binary version this helper accepts: the 1 of the core spec.
pub const VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

/// Why a module file cannot be loaded.
#[derive(Debug)]
pub enum ModuleError {
    /// The path is not absolute. The helper resolves no paths of its own, so
    /// the file the parent checked is the file the helper loads.
    Relative(PathBuf),
    /// The file is larger than [`MAX_MODULE_BYTES`].
    TooLarge {
        /// The file's size in bytes.
        len: u64,
    },
    /// The file is shorter than the eight bytes of a binary header.
    TooShort {
        /// The file's size in bytes.
        len: u64,
    },
    /// The file does not start with [`MAGIC`].
    BadMagic([u8; 4]),
    /// The file starts with the magic but names another version.
    BadVersion([u8; 4]),
    /// The file could not be read.
    Io(io::Error),
}

impl fmt::Display for ModuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl std::error::Error for ModuleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        todo!()
    }
}

/// Checks that `path` is a WebAssembly binary this helper will load.
///
/// The size is checked first, so an oversize file is refused without reading
/// it; then the eight header bytes. Nothing else about the module is inspected
/// here: wasmtime validates the rest when it compiles.
pub fn check(path: &Path) -> Result<(), ModuleError> {
    let _ = path;
    todo!()
}
