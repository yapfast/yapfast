use std::{fs::File, path::Path, process::Stdio, sync::OnceLock};

use tracing_subscriber::{filter::LevelFilter, filter::Targets, prelude::*};

pub fn init() {
    // Several tests in one process may call this; the first subscriber stays.
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(filter()))
        .try_init();
}

/// Logs to standard error, for a worker whose standard output is for Yapfast.
pub fn init_to_stderr() {
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(filter()),
        )
        .try_init();
}

/// Logs to `path`, for a Yapfast started without a console, as from Windows'
/// Explorer. Each start adds to it; one past [`LOG_FILE_SIZE`] starts over.
/// Falls back to standard output when the file won't open.
pub fn init_to_file(path: &Path) {
    if let Some(directory) = path.parent() {
        let _ = std::fs::create_dir_all(directory);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|file| {
            if file.metadata()?.len() > LOG_FILE_SIZE {
                file.set_len(0)?;
            }
            Ok(file)
        });
    let Ok(file) = file else {
        return init();
    };
    let writer = file.try_clone().map(std::sync::Mutex::new);
    let _ = LOG_FILE.set(file);
    let Ok(writer) = writer else {
        return init();
    };
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer)
                .with_filter(filter()),
        )
        .try_init();
}

/// Where a worker Yapfast starts writes its logs: Yapfast's log file, or Yapfast's
/// own standard error.
pub fn for_worker() -> Stdio {
    LOG_FILE
        .get()
        .and_then(|file| file.try_clone().ok())
        .map_or_else(Stdio::inherit, Stdio::from)
}

static LOG_FILE: OnceLock<File> = OnceLock::new();

const LOG_FILE_SIZE: u64 = 4 << 20;

// Dependency debug logs can contain tokens or message content. Keep them off.
// `YAPFAST_LOG=debug` shows Yapfast's own, such as what Go Live's servers say.
fn filter() -> Targets {
    let level = if std::env::var("YAPFAST_LOG").is_ok_and(|level| level == "debug") {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    Targets::new()
        .with_target("yapfast", level)
        .with_target("yapfast_core", level)
}
