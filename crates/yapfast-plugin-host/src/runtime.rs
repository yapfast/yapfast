//! Extism behind the protocol: load a module, call an export, drop an instance.
//!
//! The settings are the plan's table (§3.4), each with the reason it matters:
//!
//! - the module is named by [`extism::Wasm::file`], so an instance clones a path
//!   and not the module's bytes;
//! - the page limit is ours to set, because Extism's default is unlimited;
//! - vars are disabled with an explicit zero, since `None` means Extism's own
//!   one-megabyte default;
//! - no host is allowed and no path is preopened, so a guest has no network and
//!   no filesystem even if it asks;
//! - the 30-second timeout is a last resort behind the parent's per-call
//!   deadlines;
//! - WASI is off unless the caller asks, and then offers clocks and randomness
//!   without any preopened path;
//! - the compile cache is a wasmtime TOML config *file* naming an absolute
//!   directory, because `with_cache_config` takes a file and a directory is a
//!   hard error at the first compile.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use extism::Plugin;

use crate::protocol::Backend;

/// The page limit when the caller names none: 256 pages, 16 MiB (§6.2).
pub const DEFAULT_MAX_PAGES: u32 = 256;

/// The last resort deadline behind the parent's per-call deadlines (§3.4).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// How the helper runs modules.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Config {
    /// The page limit for every instance; `None` applies [`DEFAULT_MAX_PAGES`].
    pub max_pages: Option<u32>,
    /// Whether guests get a WASI context. Off unless asked: a guest that needs
    /// no WASI cannot use a clock or randomness through it.
    pub wasi: bool,
    /// The compile cache, when one is configured.
    pub cache: Option<CacheConfig>,
}

/// A wasmtime compile cache, as wasmtime wants it: a TOML file naming an
/// absolute directory.
///
/// wasmtime refuses a relative directory, and `with_cache_config` refuses a
/// path that is not a file, so both are checked here rather than at the first
/// compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheConfig {
    directory: PathBuf,
    file: PathBuf,
}

/// Why a compile cache cannot be configured.
#[derive(Debug)]
pub enum CacheError {
    /// wasmtime writes the cache under an absolute directory or not at all.
    RelativeDirectory(PathBuf),
    /// The config file was named by a relative path.
    RelativeFile(PathBuf),
    /// A path cannot be written into the TOML the config file holds.
    Unrepresentable(PathBuf),
    /// The config file could not be written.
    Io(io::Error),
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl std::error::Error for CacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        todo!()
    }
}

impl From<io::Error> for CacheError {
    fn from(error: io::Error) -> Self {
        todo!()
    }
}

impl CacheConfig {
    /// Points the cache at `directory` and keeps wasmtime's config file at
    /// `file`. Both must be absolute.
    pub fn new(directory: impl Into<PathBuf>, file: impl Into<PathBuf>) -> Result<Self, CacheError> {
        let _ = (directory, file);
        todo!()
    }

    /// The directory wasmtime writes compiled modules into.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The TOML file [`extism::PluginBuilder::with_cache_config`] is given.
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Writes the TOML file, creating its parent directory.
    pub fn write(&self) -> Result<(), CacheError> {
        todo!()
    }
}

/// The engine: one Extism instance per loaded plugin.
#[derive(Debug)]
pub struct Runtime {
    config: Config,
    plugins: HashMap<String, Plugin>,
}

impl Runtime {
    /// Builds a runtime, writing the cache config file if one is configured.
    pub fn new(config: Config) -> Result<Self, CacheError> {
        let _ = config;
        todo!()
    }

    /// The settings this runtime applies.
    pub fn config(&self) -> &Config {
        &self.config
    }
}

impl Backend for Runtime {
    fn load(&mut self, plugin: &str, module: &Path) -> Result<(), String> {
        let _ = (plugin, module);
        todo!()
    }

    fn call(&mut self, plugin: &str, method: &str, payload: &[u8]) -> Result<Vec<u8>, String> {
        let _ = (plugin, method, payload);
        todo!()
    }

    fn unload(&mut self, plugin: &str) -> Result<(), String> {
        let _ = plugin;
        todo!()
    }

    fn instances(&self) -> usize {
        todo!()
    }
}
