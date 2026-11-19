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

/// The window's frame trace, which `YAPFAST_LOG=debug,frames` asks for: a line
/// for each frame the CPU renderer presents and each step a list's motion
/// takes on one (`crates/yapfast-iced-kit/src/frames.rs`). It's a measurement
/// rather than a log, and a line a frame buries what a share or a call says,
/// so `YAPFAST_LOG=debug` leaves it out.
const FRAME_TRACE: &str = "yapfast::frames";

// Dependency debug logs can contain tokens or message content. Keep them off.
// `YAPFAST_LOG=debug` shows Yapfast's own, such as what Go Live's servers say,
// and `YAPFAST_LOG=debug,frames` adds the frame trace to it.
fn filter() -> Targets {
    let asked = std::env::var("YAPFAST_LOG").unwrap_or_default();
    let asks = |what: &str| asked.split(',').any(|part| part.trim() == what);
    let level = if asks("debug") {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    let trace = if asks("frames") {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    Targets::new()
        .with_target("yapfast", level)
        .with_target("yapfast_core", level)
        // The longest name wins, so this decides for the trace alone.
        .with_target(FRAME_TRACE, trace)
}

#[cfg(test)]
mod tests {
    use super::filter;
    use tracing::Level;

    /// What the trace's target answers with `YAPFAST_LOG` as written, and
    /// what Yapfast's own targets answer.
    fn asked(value: Option<&str>) -> (bool, bool) {
        // SAFETY: the tests that set this are the ones that read it, one at a
        // time, and they put it back.
        unsafe {
            match value {
                Some(value) => std::env::set_var("YAPFAST_LOG", value),
                None => std::env::remove_var("YAPFAST_LOG"),
            }
        }
        let targets = filter();
        let answers = (
            targets.would_enable(super::FRAME_TRACE, &Level::DEBUG),
            targets.would_enable("yapfast_core", &Level::DEBUG),
        );
        // SAFETY: as above.
        unsafe { std::env::remove_var("YAPFAST_LOG") };
        answers
    }

    #[test]
    fn the_frame_trace_is_asked_for_beside_the_debug_logs() {
        assert_eq!(asked(None), (false, false), "a plain start says nothing");
        assert_eq!(
            asked(Some("debug")),
            (false, true),
            "the debug logs, no trace"
        );
        assert_eq!(asked(Some("debug,frames")), (true, true), "both");
        assert_eq!(asked(Some("frames")), (true, false), "the trace alone");
    }
}
