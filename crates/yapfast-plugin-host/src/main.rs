//! The `yapfast-plugins` helper binary.
//!
//! Requests arrive on stdin and responses leave on stdout; nothing else may be
//! written to either, so every diagnostic goes to stderr. The helper ends on
//! `shutdown`, on a clean end of stdin, or after the unload of its last
//! instance. See `docs/plugin-host.md`.

use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::process::ExitCode;

use yapfast_plugin_host::protocol;
use yapfast_plugin_host::runtime::{Cache, CacheConfig, Config, Runtime};

const USAGE: &str = "\
usage: yapfast-plugins [--cache-dir <dir> | --no-cache] [--wasi]

  --cache-dir <dir>  keep wasmtime's compile cache under <dir>, which must be
                     absolute: the config file is <dir>/wasmtime-cache.toml and
                     the cache itself is <dir>/cache
  --no-cache         compile every module, with no on-disk cache
  --wasi             give every guest a WASI context: clocks and randomness,
                     and no preopened path, so still no filesystem
  --help             print this text

The helper reads requests on stdin and writes responses on stdout.
";

fn main() -> ExitCode {
    // Wasmtime compiles on rayon. Two threads are the plan's setting: a cold
    // compile stays at 70-90 ms, and the pool is 4 threads rather than 30.
    // Safety: this runs before any thread exists, and nothing in the helper
    // reads or writes the environment after this point.
    if std::env::var_os("RAYON_NUM_THREADS").is_none() {
        unsafe { std::env::set_var("RAYON_NUM_THREADS", "2") };
    }

    let options = match parse(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => {
            eprint!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("yapfast-plugins: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let config = match options.config() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("yapfast-plugins: {message}");
            return ExitCode::from(2);
        }
    };

    let mut runtime = match Runtime::new(config) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("yapfast-plugins: {error}");
            return ExitCode::FAILURE;
        }
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = BufReader::new(stdin.lock());
    let mut output = BufWriter::new(stdout.lock());

    match protocol::serve(&mut input, &mut output, &mut runtime) {
        Ok(_outcome) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("yapfast-plugins: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The command line.
#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    wasi: bool,
    no_cache: bool,
    cache_dir: Option<PathBuf>,
}

/// Parses the command line, or `None` for `--help`.
fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => return Ok(None),
            "--wasi" => options.wasi = true,
            "--no-cache" => options.no_cache = true,
            "--cache-dir" => {
                let directory = args
                    .next()
                    .ok_or_else(|| "--cache-dir needs a directory".to_owned())?;
                options.cache_dir = Some(PathBuf::from(directory));
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Some(options))
}

impl Options {
    /// The runtime settings these options ask for.
    fn config(&self) -> Result<Config, String> {
        if self.no_cache && self.cache_dir.is_some() {
            return Err("--no-cache and --cache-dir cannot both be given".to_owned());
        }
        let cache = match &self.cache_dir {
            Some(directory) if !directory.is_absolute() => {
                return Err(format!(
                    "cache directory {} is not absolute",
                    directory.display()
                ));
            }
            Some(directory) => {
                Cache::File(CacheConfig::under(directory).map_err(|error| error.to_string())?)
            }
            None if self.no_cache => Cache::Disabled,
            None => Cache::Fallback,
        };
        Ok(Config {
            max_pages: None,
            wasi: self.wasi,
            cache,
        })
    }
}
