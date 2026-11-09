//! What a module file must be before Extism is given it.
//!
//! Extism accepts more than WebAssembly binaries: it sniffs text for the WAT it
//! compiles, and wasmtime compiles WAT from a file just as readily as from
//! memory. A `.wasm` in the content store that really holds `(module …)` would
//! therefore load, so the magic and version below are the only thing that
//! rejects a text module. They are checked here, on the file's own bytes,
//! before [`extism::Wasm::file`] ever names it.

use std::fmt;
use std::io::{self, Read};
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
        match self {
            Self::Relative(path) => {
                write!(f, "module path {} is not absolute", path.display())
            }
            Self::TooLarge { len } => write!(
                f,
                "module is {len} bytes, over the {MAX_MODULE_BYTES}-byte cap"
            ),
            Self::TooShort { len } => write!(
                f,
                "module is {len} bytes, shorter than a WebAssembly binary header"
            ),
            Self::BadMagic(bytes) => write!(
                f,
                "module is not a WebAssembly binary: it starts with {bytes:02x?}, not 00 61 73 6d"
            ),
            Self::BadVersion(bytes) => {
                write!(f, "module names the unsupported version {bytes:02x?}")
            }
            Self::Io(error) => write!(f, "reading the module: {error}"),
        }
    }
}

impl std::error::Error for ModuleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// Checks that `path` is a WebAssembly binary this helper will load.
///
/// The size is checked first, so an oversize file is refused without reading
/// it; then the eight header bytes. Nothing else about the module is inspected
/// here: wasmtime validates the rest when it compiles.
pub fn check(path: &Path) -> Result<(), ModuleError> {
    if !path.is_absolute() {
        return Err(ModuleError::Relative(path.to_path_buf()));
    }

    let len = std::fs::metadata(path).map_err(ModuleError::Io)?.len();
    if len > MAX_MODULE_BYTES {
        return Err(ModuleError::TooLarge { len });
    }
    if len < 8 {
        return Err(ModuleError::TooShort { len });
    }

    let mut header = [0u8; 8];
    std::fs::File::open(path)
        .map_err(ModuleError::Io)?
        .read_exact(&mut header)
        .map_err(ModuleError::Io)?;

    if header[..4] != MAGIC {
        return Err(ModuleError::BadMagic(
            header[..4].try_into().expect("four bytes"),
        ));
    }
    if header[4..] != VERSION {
        return Err(ModuleError::BadVersion(
            header[4..].try_into().expect("four bytes"),
        ));
    }
    Ok(())
}
