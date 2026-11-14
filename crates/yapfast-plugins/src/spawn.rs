//! Starting the `yapfast-plugins` helper, and starting it again after a crash.
//!
//! The helper is disposable. It is started the first time a plugin has
//! something to do and ends with the last instance, so a Yapfast whose plugins
//! are all idle pays neither its memory nor its threads, and one that crashed
//! can be replaced by a fresh process rather than repaired.
//!
//! It is started with an empty environment. Extism reads `EXTISM_*` variables
//! and one of them sends guest stdout to the helper's stdout, which is the pipe
//! frames travel on; an inherited environment would also hand the helper the
//! Discord token Yapfast was started with, which the design says it never sees.
//! Only what the helper needs is passed: its thread count, and the log level so
//! its warnings land in the same log as the app's.

use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use yapfast_plugin_api::limits;

/// The helper's file name, beside the running Yapfast.
const PROGRAM: &str = if cfg!(windows) {
    "yapfast-plugins.exe"
} else {
    "yapfast-plugins"
};

/// A helper process and the two pipes it speaks on.
///
/// Its standard error is the app's to say ([`start`] takes it): a helper that
/// fails before its pipe is up still says why somewhere a person can read it.
pub struct Helper {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
}

/// Starts the helper, telling it where to keep compiled modules.
///
/// The cache directory is created here because the helper treats it as given:
/// it belongs to this app's data directory, and a plugin that is enabled at
/// startup must run even on the first launch.
pub fn start(data_dir: &Path, stderr: Stdio) -> Result<Helper> {
    let program = program()?;
    let cache = data_dir.join("plugins").join("cache");
    if let Err(error) = std::fs::create_dir_all(&cache) {
        // A missing cache costs a compile per module and nothing else, so this
        // is worth a line in the log, not the end of the plugin system.
        tracing::warn!(%error, path = %cache.display(), "could not create the plugin compile cache");
    }
    let mut command = Command::new(&program);
    command
        .arg("run")
        .arg("--cache")
        .arg(&cache)
        .env_clear()
        // Wasmtime's compile pool would otherwise size itself to the machine:
        // plugins are a side feature and must not take every core.
        .env("RAYON_NUM_THREADS", "2")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr);
    if let Some(level) = std::env::var_os("YAPFAST_LOG") {
        command.env("YAPFAST_LOG", level);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("could not start {}", program.display()))?;
    let stdin = child
        .stdin
        .take()
        .context("the plugin helper has no standard input")?;
    let stdout = child
        .stdout
        .take()
        .context("the plugin helper has no standard output")?;
    Ok(Helper {
        child,
        stdin,
        stdout,
    })
}

/// Where the helper should be, whether or not it is there.
///
/// `YAPFAST_PLUGINS` names it outright, which is how a build of the helper in a
/// target directory is tried without installing it. Otherwise it sits beside
/// the running Yapfast, which is where a package puts it, and keeps it the same
/// version as the window it talks to.
pub fn program() -> Result<PathBuf> {
    if let Some(program) = std::env::var_os("YAPFAST_PLUGINS") {
        let program = PathBuf::from(program);
        if !program.is_file() {
            bail!(
                "YAPFAST_PLUGINS names {}, which isn't a file",
                program.display()
            );
        }
        return Ok(program);
    }
    let beside = running_dir()?.join(PROGRAM);
    if !beside.is_file() {
        bail!("{} isn't installed, so plugins can't run", beside.display());
    }
    Ok(beside)
}

/// The directory the running Yapfast was started from.
///
/// On Linux this reads `/proc/self/exe` rather than asking for the current
/// executable, because an update replaces Yapfast's file: the path through
/// `/proc` still names the directory the package installed into, while the
/// answered path names a file that no longer exists.
fn running_dir() -> Result<PathBuf> {
    let exe = if cfg!(target_os = "linux") {
        let exe = std::fs::read_link("/proc/self/exe").context("could not find Yapfast")?;
        // A deleted binary is reported as `/path/to/yapfast (deleted)`.
        match exe
            .to_str()
            .and_then(|path| path.strip_suffix(" (deleted)"))
        {
            Some(stripped) => PathBuf::from(stripped),
            None => exe,
        }
    } else {
        std::env::current_exe().context("could not find Yapfast")?
    };
    exe.parent()
        .map(Path::to_path_buf)
        .context("Yapfast has no directory to look for the plugin helper in")
}

/// What to do after the helper ended without being asked to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    /// Start it again after this long. The wait grows with each crash, so a
    /// helper that dies on every input can't spin the disk and the CPU.
    After(Duration),
    /// Stop trying, with a sentence for the person: this many crashes inside
    /// one window means the fault is not a hiccup.
    Off(String),
}

/// Counts the helper's crashes inside [`limits::CRASH_WINDOW_MS`] and decides
/// what the next start should be.
///
/// Only crashes that the parent didn't ask for are recorded: a helper that exits
/// because nothing is left to run is doing its job.
#[derive(Debug, Default)]
pub struct Restarts {
    crashes: Vec<Instant>,
}

impl Restarts {
    pub fn new() -> Self {
        Restarts::default()
    }

    /// Records that the helper ended on its own, and says what happens next.
    pub fn crashed(&mut self, now: Instant) -> Next {
        let window = Duration::from_millis(limits::CRASH_WINDOW_MS);
        self.crashes.retain(|at| now.duration_since(*at) < window);
        self.crashes.push(now);
        let count = self.crashes.len() as u32;
        if count >= limits::MAX_CRASHES {
            return Next::Off(format!(
                "the plugin helper stopped {count} times in {} seconds",
                limits::CRASH_WINDOW_MS / 1000
            ));
        }
        let wait = limits::RESTART_BACKOFF_MS
            [(count as usize - 1).min(limits::RESTART_BACKOFF_MS.len() - 1)];
        Next::After(Duration::from_millis(wait))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_crash_backs_off_briefly_and_the_third_gives_up() {
        let mut restarts = Restarts::new();
        let now = Instant::now();
        assert_eq!(
            restarts.crashed(now),
            Next::After(Duration::from_millis(limits::RESTART_BACKOFF_MS[0]))
        );
        assert_eq!(
            restarts.crashed(now),
            Next::After(Duration::from_millis(limits::RESTART_BACKOFF_MS[1]))
        );
        match restarts.crashed(now) {
            Next::Off(reason) => assert!(reason.contains("3 times"), "{reason}"),
            other => panic!("three crashes in a row should give up, not {other:?}"),
        }
    }

    #[test]
    fn crashes_older_than_the_window_are_forgotten() {
        let mut restarts = Restarts::new();
        let start = Instant::now();
        let window = Duration::from_millis(limits::CRASH_WINDOW_MS);
        for _ in 0..(limits::MAX_CRASHES - 1) {
            assert!(matches!(restarts.crashed(start), Next::After(_)));
        }
        // One more crash, but a window later: the count starts over.
        assert!(matches!(
            restarts.crashed(start + window + Duration::from_millis(1)),
            Next::After(_)
        ));
    }

    #[test]
    fn the_helper_is_looked_for_beside_the_running_program() {
        // The path a packaged Yapfast looks for, without needing the helper to
        // be there: the directory is the test binary's, which plainly exists.
        let directory = running_dir().expect("the test binary has a directory");
        assert!(directory.is_dir(), "{}", directory.display());
        assert!(directory.join(PROGRAM).starts_with(&directory));
    }
}
