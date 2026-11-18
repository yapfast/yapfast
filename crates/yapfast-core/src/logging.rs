use std::{fs::File, path::Path, process::Stdio, sync::OnceLock};

use tracing_subscriber::{filter::LevelFilter, filter::Targets, prelude::*};

pub fn init() {
    // Several tests in one process may call this; the first subscriber stays.
    #[cfg(target_os = "android")]
    {
        let _ = tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .without_time()
                    .with_writer(android_logcat::MakeLogcatWriter)
                    .with_filter(filter()),
            )
            .try_init();
    }
    #[cfg(not(target_os = "android"))]
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

#[cfg(target_os = "android")]
mod android_logcat {
    use std::{
        ffi::{CString, c_char, c_int},
        io,
    };

    use tracing_subscriber::fmt::MakeWriter;

    const ANDROID_LOG_INFO: c_int = 4;
    const TAG: &[u8] = b"Yapfast\0";

    unsafe extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }

    pub struct MakeLogcatWriter;

    pub struct LogcatWriter(Vec<u8>);

    impl<'a> MakeWriter<'a> for MakeLogcatWriter {
        type Writer = LogcatWriter;

        fn make_writer(&'a self) -> Self::Writer {
            LogcatWriter(Vec::with_capacity(256))
        }
    }

    impl io::Write for LogcatWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Drop for LogcatWriter {
        fn drop(&mut self) {
            if self.0.is_empty() {
                return;
            }
            while self.0.last() == Some(&b'\n') {
                self.0.pop();
            }
            let Ok(text) = CString::new(std::mem::take(&mut self.0)) else {
                return;
            };
            // SAFETY: TAG and text are nul-terminated C strings.
            unsafe {
                __android_log_write(ANDROID_LOG_INFO, TAG.as_ptr().cast(), text.as_ptr());
            }
        }
    }
}
