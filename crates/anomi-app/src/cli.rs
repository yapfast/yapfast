//! The command line, as `src/main.rs` of the Slint app takes it:
//!
//!     anomi [--preview] [--data-dir PATH] [--renderer gpu|software] [--background] [LINK]
//!     anomi --version | --help
//!
//! and `--stream-worker`, which the core starts this same binary with for Go
//! Live and video. A link comes as one word, or after `--open-link` as the
//! desktop hands it; the `session` commands stay with the Slint binary until
//! Q05 (docs/iced/tasks/integration.md).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use yapfast_core::runtime::BackgroundRuntime;

/// What a start asks for.
#[derive(Debug, Default)]
pub struct Start {
    pub preview: bool,
    pub data_dir: Option<PathBuf>,
    /// Started at sign-in: the app goes to the tray with the saved session,
    /// where there's a tray and a session.
    pub background: bool,
    /// A Discord link to open (`platform::links`).
    pub link: Option<String>,
}

pub fn main() -> Result<()> {
    fix_mmap_threshold();
    // A stream worker starts nothing else: no window, runtime or TLS.
    #[cfg(feature = "discord")]
    if std::env::args().nth(1).as_deref() == Some(yapfast_core::stream_worker::ARGUMENT) {
        yapfast_core::logging::init_to_stderr();
        return yapfast_core::stream_worker::run();
    }
    let start = match parse(std::env::args().skip(1))? {
        Parsed::Run(start) => start,
        Parsed::Print(text) => {
            println!("{text}");
            return Ok(());
        }
    };
    crate::logging::init();
    yapfast_core::init_tls()?;
    let runtime = BackgroundRuntime::new().context("could not start background workers")?;
    let root = match &start.data_dir {
        Some(directory) => directory.clone(),
        None => yapfast_core::default_directory()?,
    };
    crate::run::run(runtime, root, start)
}

pub enum Parsed {
    Run(Start),
    Print(String),
}

pub fn parse(arguments: impl Iterator<Item = String>) -> Result<Parsed> {
    let mut start = Start::default();
    let mut linked = false;
    let mut arguments = arguments.peekable();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--preview" => start.preview = true,
            "--background" => start.background = true,
            "--data-dir" => {
                let path = arguments.next().context("--data-dir needs a path")?;
                start.data_dir = Some(path.into());
            }
            // Only the CPU renderer is built; the GPU one is an experiment
            // behind a cargo feature (docs/iced/README.md).
            "--renderer" => {
                let renderer = arguments
                    .next()
                    .context("--renderer needs gpu or software")?;
                if renderer != "software" && renderer != "gpu" {
                    bail!("--renderer takes gpu or software");
                }
            }
            // As the desktop hands a link: whatever follows is the link.
            "--open-link" => {
                let link = arguments.next().context("--open-link needs a link")?;
                start.link = written(&link);
            }
            other if link(other) => {
                if std::mem::replace(&mut linked, true) {
                    bail!("specify one Discord link");
                }
                start.link = written(other);
                if start.link.is_none() {
                    tracing::info!("Anomi has no place for the link it was handed");
                }
            }
            "--version" | "-V" => {
                return Ok(Parsed::Print(format!(
                    "{} {} (iced interface)",
                    yapfast_lang::PRODUCT,
                    env!("CARGO_PKG_VERSION")
                )));
            }
            "--help" | "-h" => return Ok(Parsed::Print(HELP.into())),
            other => bail!("unknown argument {other:?}; run anomi --help"),
        }
    }
    Ok(Parsed::Run(start))
}

const HELP: &str = "Usage: anomi [--preview] [--data-dir PATH] [--renderer gpu|software] [--background] [LINK]\n       anomi [--version | --help]\n\nAnomi's iced interface, beside the Slint one, on the same core and data.\n--preview opens local sample conversations without connecting.\n--data-dir keeps the session and layout.json in PATH.\n--renderer is accepted for the Slint app's command line; this build draws on the CPU.\n--background starts in the system tray with a saved session, as starting at sign-in does.\nLINK opens a Discord server, channel, message, profile or invite link; with Anomi already running, it opens there.";

/// A link given as an argument: anything under `discord:` or `yapfast:`,
/// and Discord's web links this build opens (`link_arguments` in the Slint
/// app's `src/main.rs`).
fn link(argument: &str) -> bool {
    argument.split_once(':').is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("discord") || scheme.eq_ignore_ascii_case("yapfast")
    }) || written(argument).is_some()
}

/// A link as one word, when it names a place this build opens.
fn written(link: &str) -> Option<String> {
    #[cfg(feature = "discord")]
    return yapfast_core::discord::discord_link(link);
    #[cfg(not(feature = "discord"))]
    {
        let _ = link;
        None
    }
}

/// See `fix_mmap_threshold` in the Slint app's `src/main.rs`: a fixed mmap
/// threshold returns decoded pictures' memory as they go.
fn fix_mmap_threshold() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    if std::env::var_os("MALLOC_MMAP_THRESHOLD_").is_none()
        && !std::env::var("GLIBC_TUNABLES")
            .is_ok_and(|tunables| tunables.contains("glibc.malloc.mmap_threshold"))
    {
        const M_MMAP_THRESHOLD: std::ffi::c_int = -3;
        unsafe extern "C" {
            fn mallopt(parameter: std::ffi::c_int, value: std::ffi::c_int) -> std::ffi::c_int;
        }
        // SAFETY: this runs first in main, before any other thread starts.
        unsafe { mallopt(M_MMAP_THRESHOLD, 128 * 1024) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> impl Iterator<Item = String> {
        text.split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn takes_what_ui_session_passes() {
        let Ok(Parsed::Run(start)) = parse(args("--renderer software --data-dir /tmp/x")) else {
            panic!("a run");
        };
        assert_eq!(start.data_dir, Some(PathBuf::from("/tmp/x")));
        assert!(!start.preview);
        assert!(parse(args("--renderer vulkan")).is_err());
        assert!(parse(args("--nonsense")).is_err());
        let Ok(Parsed::Run(start)) = parse(args("--background https://discord.com/channels/1/2"))
        else {
            panic!("a run");
        };
        assert!(start.background);
        assert_eq!(
            start.link.as_deref(),
            Some("https://discord.com/channels/1/2")
        );
        assert!(parse(args("discord://a discord://b")).is_err());
    }
}
