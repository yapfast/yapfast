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

use extism::{Manifest, Plugin, PluginBuilder, Wasm};

use crate::module;
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
    pub cache: Cache,
}

/// Where wasmtime's compile cache goes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Cache {
    /// Extism's own fallback: `EXTISM_CACHE_CONFIG`, then wasmtime's default
    /// path. The parent passes a cache explicitly rather than relying on this.
    #[default]
    Fallback,
    /// No on-disk cache: every load compiles. The measurement harness asks for
    /// this to time a cold compile.
    Disabled,
    /// A config file the helper writes and wasmtime reads.
    File(CacheConfig),
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
        match self {
            Self::RelativeDirectory(path) => write!(
                f,
                "cache directory {} is not absolute, and wasmtime refuses a relative one",
                path.display()
            ),
            Self::RelativeFile(path) => {
                write!(f, "cache config file {} is not absolute", path.display())
            }
            Self::Unrepresentable(path) => write!(
                f,
                "cache path {} cannot be written into the TOML config",
                path.display()
            ),
            Self::Io(error) => write!(f, "writing the cache config: {error}"),
        }
    }
}

impl std::error::Error for CacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CacheError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl CacheConfig {
    /// Points the cache at `directory` and keeps wasmtime's config file at
    /// `file`. Both must be absolute.
    pub fn new(
        directory: impl Into<PathBuf>,
        file: impl Into<PathBuf>,
    ) -> Result<Self, CacheError> {
        let directory = directory.into();
        if !directory.is_absolute() {
            return Err(CacheError::RelativeDirectory(directory));
        }
        let file = file.into();
        if !file.is_absolute() {
            return Err(CacheError::RelativeFile(file));
        }
        Ok(Self { directory, file })
    }

    /// The helper's layout under `root`: wasmtime's config file at
    /// `root/wasmtime-cache.toml`, and the cache itself in `root/cache`.
    ///
    /// The config file is deliberately outside the cache directory: wasmtime's
    /// own cleanup deletes files it does not recognise from the cache
    /// directory, a config file placed there included.
    pub fn under(root: impl Into<PathBuf>) -> Result<Self, CacheError> {
        let root = root.into();
        Self::new(root.join("cache"), root.join("wasmtime-cache.toml"))
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
    ///
    /// The shape is wasmtime's: a `[cache]` table whose `directory` key names
    /// the absolute path. Writing it here rather than handing Extism a
    /// directory is the whole point: `with_cache_config` reads a *file*.
    pub fn write(&self) -> Result<(), CacheError> {
        let directory = self.directory.display().to_string();
        let mut escaped = String::with_capacity(directory.len());
        for character in directory.chars() {
            match character {
                '\\' => escaped.push_str("\\\\"),
                '"' => escaped.push_str("\\\""),
                control if control.is_control() => {
                    return Err(CacheError::Unrepresentable(self.directory.clone()));
                }
                other => escaped.push(other),
            }
        }

        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.file, format!("[cache]\ndirectory = \"{escaped}\"\n"))?;
        Ok(())
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
        if let Cache::File(cache) = &config.cache {
            cache.write()?;
        }
        Ok(Self {
            config,
            plugins: HashMap::new(),
        })
    }

    /// The settings this runtime applies.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The manifest for a module: the plan's table, applied in one place.
    fn manifest(&self, module: &Path) -> Manifest {
        let mut manifest = Manifest::new([Wasm::file(module)]);
        // Our default, not Extism's: `None` here would install no limiter at
        // all, and the whole store counts against the limit, input included.
        manifest.memory.max_pages = Some(self.config.max_pages.unwrap_or(DEFAULT_MAX_PAGES));
        // `Some(0)`, because `None` means Extism's own one megabyte of vars.
        manifest.memory.max_var_bytes = Some(0);
        manifest.allowed_hosts = Some(Vec::new());
        manifest.timeout_ms = Some(DEFAULT_TIMEOUT.as_millis() as u64);
        manifest
    }
}

impl Backend for Runtime {
    fn load(&mut self, plugin: &str, module: &Path) -> Result<(), String> {
        if self.plugins.contains_key(plugin) {
            return Err(format!("plugin {plugin} is already loaded"));
        }
        // The magic check runs on the file's bytes and before Extism names it,
        // because wasmtime would compile a WAT file just as happily.
        module::check(module).map_err(|error| error.to_string())?;

        let mut builder = PluginBuilder::new(self.manifest(module)).with_wasi(self.config.wasi);
        builder = match &self.config.cache {
            Cache::Fallback => builder,
            Cache::Disabled => builder.with_cache_disabled(),
            Cache::File(cache) => builder.with_cache_config(cache.file()),
        };
        let compiled = builder
            .compile()
            .map_err(|error| format!("compiling {}: {error}", module.display()))?;
        let instance = Plugin::new_from_compiled(&compiled)
            .map_err(|error| format!("instancing {}: {error}", module.display()))?;
        self.plugins.insert(plugin.to_owned(), instance);
        Ok(())
    }

    fn call(&mut self, plugin: &str, method: &str, payload: &[u8]) -> Result<Vec<u8>, String> {
        let instance = self
            .plugins
            .get_mut(plugin)
            .ok_or_else(|| format!("plugin {plugin} is not loaded"))?;
        // Copied into a `Vec`, since a borrowed answer points into guest memory
        // that the next call invalidates.
        instance
            .call::<&[u8], Vec<u8>>(method, payload)
            .map_err(|error| format!("{plugin}.{method}: {error}"))
    }

    fn unload(&mut self, plugin: &str) -> Result<(), String> {
        match self.plugins.remove(plugin) {
            Some(instance) => {
                drop(instance);
                Ok(())
            }
            None => Err(format!("plugin {plugin} is not loaded")),
        }
    }

    fn instances(&self) -> usize {
        self.plugins.len()
    }
}
