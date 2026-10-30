// Matrix sync's encrypted-event futures exceed rustc's default trait depth.
#![recursion_limit = "256"]
// Opened from Explorer, Yapfast opens no console window beside its own; see
// `has_console`.
#![cfg_attr(all(windows, feature = "desktop"), windows_subsystem = "windows")]

#[cfg(feature = "desktop")]
mod autostart;
#[cfg(feature = "desktop")]
mod copy;
#[cfg(feature = "desktop")]
mod custom_emoji;
#[cfg(all(feature = "desktop", target_os = "linux"))]
mod desktop_entry;
#[cfg(feature = "desktop")]
mod drop;
#[cfg(feature = "desktop")]
mod emoji;
#[cfg(feature = "desktop")]
mod emoji_font;
#[cfg(feature = "desktop")]
mod graphics;
#[cfg(feature = "desktop")]
mod icon;
#[cfg(feature = "desktop")]
mod instance;
#[cfg(feature = "desktop")]
mod notify;
#[cfg(feature = "desktop")]
mod paste;
#[cfg(feature = "desktop")]
mod protocol;
#[cfg(feature = "desktop")]
mod push_to_talk;
#[cfg(feature = "desktop")]
mod tray;
#[cfg(feature = "desktop")]
mod ui;
#[cfg(feature = "desktop")]
mod zoom;

use anyhow::{Context, Result, bail};
use yapfast_core::{cli, logging, runtime::BackgroundRuntime};
#[cfg(feature = "matrix")]
use yapfast_core::{experiment, voice_demo};

/// glibc maps a block of 128 KiB or more on its own and unmaps it when freed,
/// but each such free raises that threshold to the block's size, up to 32 MiB:
/// after the first decoded picture, every later one stayed in the heap. With a
/// fixed threshold and glibc's own count of arenas, Yapfast held 9.3 MiB less in
/// a call than with two arenas, and 5.1 MiB less after it; see
/// docs/measurements/README.md. The environment's own setting wins.
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
        // SAFETY: this runs first in main, before Yapfast starts any other thread.
        unsafe { mallopt(M_MMAP_THRESHOLD, 128 * 1024) };
    }
}

fn main() -> Result<()> {
    fix_mmap_threshold();
    // A stream worker starts nothing else: no UI, runtime or TLS.
    #[cfg(feature = "discord")]
    if std::env::args().nth(1).as_deref() == Some(yapfast_core::stream_worker::ARGUMENT) {
        logging::init_to_stderr();
        return yapfast_core::stream_worker::run();
    }
    let mut arguments: Vec<_> = std::env::args().skip(1).collect();
    let renderer = renderer_argument(&mut arguments)?;
    let background = flag_argument(&mut arguments, "--background");
    let mut matched_arguments = arguments.clone();
    if !arguments
        .first()
        .is_some_and(|argument| argument == "session")
    {
        let _ = link_argument(&mut matched_arguments)?;
    }

    #[cfg(all(windows, feature = "desktop"))]
    match data_directory(&arguments) {
        Some(directory) if !has_console() => logging::init_to_file(&directory.join("yapfast.log")),
        _ => logging::init(),
    }
    #[cfg(not(all(windows, feature = "desktop")))]
    logging::init();
    yapfast_core::init_tls()?;

    let runtime = BackgroundRuntime::new().context("could not start background workers")?;
    #[cfg(feature = "matrix")]
    if arguments
        .first()
        .is_some_and(|argument| argument == "session")
    {
        return runtime.block_on(cli::session(&arguments[1..]));
    }
    match matched_arguments.as_slice() {
        [] => launch(
            runtime,
            yapfast_core::default_directory()?,
            false,
            renderer,
            background,
            &arguments,
        ),
        [argument] if argument == "--preview" => launch(
            runtime,
            yapfast_core::default_directory()?,
            true,
            renderer,
            background,
            &arguments,
        ),
        [argument, path] if argument == "--data-dir" => launch(
            runtime,
            path.into(),
            false,
            renderer,
            background,
            &arguments,
        ),
        [preview, argument, path] if preview == "--preview" && argument == "--data-dir" => {
            launch(runtime, path.into(), true, renderer, background, &arguments)
        }
        [argument] if argument == "--smoke-test" => {
            runtime.block_on(runtime.spawn(yapfast_core::check_build()))?;
            println!(
                "Yapfast {}: Tokio, LiveKit{}{} build check passed.",
                env!("CARGO_PKG_VERSION"),
                if cfg!(feature = "matrix") {
                    ", Matrix SDK, MatrixRTC"
                } else {
                    ""
                },
                if cfg!(feature = "discord") {
                    ", Discord voice"
                } else {
                    ""
                },
            );
            Ok(())
        }
        #[cfg(feature = "matrix")]
        [argument] if argument == "--voice-demo" => {
            runtime.block_on(voice_demo::provision()).map(|_| ())
        }
        [argument] if argument == "--audio-devices" => cli::audio_devices(),
        #[cfg(feature = "matrix")]
        [argument] if argument == "--call-experiment" => runtime.block_on(experiment::run()),
        #[cfg(feature = "matrix")]
        [argument, resume, path] if argument == "--call-experiment" && resume == "--resume" => {
            runtime.block_on(experiment::resume(path.into()))
        }
        [argument] if argument == "--help" || argument == "-h" => {
            println!(
                "Usage: yapfast [--preview] [--data-dir PATH] [--renderer gpu|software] [--background] [LINK]\n       yapfast [--smoke-test | --help]\n       yapfast --call-experiment [--resume PATH]\n       yapfast [--voice-demo | --audio-devices]\n       yapfast session login --homeserver URL --user USER --password-stdin [--data-dir PATH]\n       yapfast session status [--data-dir PATH]\n       yapfast session recover --key-stdin [--data-dir PATH]\n       yapfast session logout [--data-dir PATH]\n\nWithout arguments, opens native Slint chat and restores a saved session.\n--renderer selects Skia Software or Skia GPU for this start, over the setting.\nIf Skia GPU fails to start, Yapfast restarts with Skia Software.\n--background starts in the system tray with a saved session, as starting at sign-in does.\nLINK opens a Discord server, channel, message, profile or invite link in Yapfast.\n--preview opens local sample conversations without connecting to Matrix.\n--smoke-test checks the linked stack without signing in or opening audio devices.\n--call-experiment creates disposable local accounts and checks encrypted synthetic audio. Start scripts/test-stack up first.\n--voice-demo creates two local accounts and a server with encrypted text and voice channels.\n--audio-devices lists native audio devices and cameras without recording.\nSession commands support password sign-in. Secrets are read from stdin."
            );
            Ok(())
        }
        _ => bail!("unknown argument; run yapfast --help"),
    }
}

/// Whether Yapfast's output reaches someone: a file or pipe it was given, or
/// the console it was started from, which it joins. A Windows program
/// without a console of its own gets neither when opened from Explorer.
#[cfg(all(windows, feature = "desktop"))]
fn has_console() -> bool {
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE},
    };
    // SAFETY: neither call takes a pointer; Yapfast's other threads haven't started.
    unsafe {
        let output = GetStdHandle(STD_OUTPUT_HANDLE);
        (!output.is_null() && output != INVALID_HANDLE_VALUE)
            || AttachConsole(ATTACH_PARENT_PROCESS) != 0
    }
}

/// The folder Yapfast keeps its files in, which holds the log without a console.
#[cfg(all(windows, feature = "desktop"))]
fn data_directory(arguments: &[String]) -> Option<std::path::PathBuf> {
    match arguments
        .iter()
        .position(|argument| argument == "--data-dir")
    {
        Some(index) => arguments.get(index + 1).map(Into::into),
        None => yapfast_core::default_directory().ok(),
    }
}

/// Set only for a process that replaces a failed GPU start.
#[cfg(feature = "desktop")]
const GPU_FALLBACK_REASON: &str = "YAPFAST_GPU_FALLBACK_REASON";

#[cfg(feature = "desktop")]
fn launch(
    runtime: BackgroundRuntime,
    root: std::path::PathBuf,
    preview: bool,
    renderer: Option<&str>,
    background: bool,
    arguments: &[String],
) -> Result<()> {
    let renderer = renderer.map(|renderer| match renderer {
        "gpu" => graphics::Renderer::Gpu,
        _ => graphics::Renderer::Software,
    });
    let fallback = std::env::var(GPU_FALLBACK_REASON).ok();
    let mut relaunch_arguments = arguments.to_owned();
    let open_link = link_argument(&mut relaunch_arguments)?;
    #[cfg(feature = "discord")]
    yapfast_core::keep_discord_files(&root);
    match ui::run(
        runtime, root, preview, renderer, fallback, background, open_link,
    )? {
        ui::Exit::Closed => Ok(()),
        ui::Exit::UseSoftware(reason) => relaunch(&relaunch_arguments, Some(&reason)),
        ui::Exit::Restart => relaunch(&relaunch_arguments, None),
    }
}

/// The window system allows one renderer per process, so a failed GPU start,
/// or a changed renderer setting, continues in a new process after this one
/// released its session and runtime. `arguments` no longer hold `--renderer`,
/// so a restart follows the setting, nor `--background`: the window shows.
#[cfg(feature = "desktop")]
fn relaunch(arguments: &[String], gpu_failure: Option<&str>) -> Result<()> {
    let mut command = std::process::Command::new(
        std::env::current_exe().context("could not find Yapfast to restart it")?,
    );
    command.args(arguments);
    match gpu_failure {
        Some(reason) => command
            .args(["--renderer", "software"])
            .env(GPU_FALLBACK_REASON, reason),
        None => command.env_remove(GPU_FALLBACK_REASON),
    };
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec()).context("could not restart Yapfast")
    }
    #[cfg(not(unix))]
    {
        let status = command.status().context("could not restart Yapfast")?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

#[cfg(not(feature = "desktop"))]
fn launch(
    _: BackgroundRuntime,
    _: std::path::PathBuf,
    _: bool,
    _: Option<&str>,
    _: bool,
    _: &[String],
) -> Result<()> {
    bail!(
        "this build has no desktop UI; run --help for headless commands or rebuild with default features"
    )
}

/// Whether `flag` was given, taking it out of `arguments`.
fn flag_argument(arguments: &mut Vec<String>, flag: &str) -> bool {
    let before = arguments.len();
    arguments.retain(|argument| argument != flag);
    arguments.len() != before
}

/// A Discord link given as an argument, taking it out of `arguments`, so
/// a renderer restart doesn't open it again.
fn link_argument(arguments: &mut Vec<String>) -> Result<Option<String>> {
    #[cfg(all(feature = "desktop", feature = "discord"))]
    {
        let links: Vec<usize> = arguments
            .iter()
            .enumerate()
            .filter_map(|(index, argument)| {
                yapfast_core::discord::discord_route(argument).map(|_| index)
            })
            .collect();
        return match links.as_slice() {
            [] => Ok(None),
            [index] => Ok(Some(arguments.remove(*index))),
            _ => bail!("specify one Discord link"),
        };
    }
    #[cfg(not(all(feature = "desktop", feature = "discord")))]
    {
        let _ = arguments;
        Ok(None)
    }
}

#[cfg(all(test, feature = "desktop", feature = "discord"))]
mod tests {
    use super::link_argument;

    #[test]
    fn a_discord_link_argument_is_taken_out() {
        let mut arguments = vec![
            "--data-dir".to_owned(),
            "folder".to_owned(),
            "https://discord.com/channels/10/11".to_owned(),
        ];
        assert_eq!(
            link_argument(&mut arguments).unwrap().as_deref(),
            Some("https://discord.com/channels/10/11")
        );
        assert_eq!(arguments, ["--data-dir", "folder"]);
    }

    #[test]
    fn two_discord_link_arguments_are_refused() {
        let mut arguments = vec![
            "https://discord.gg/one".to_owned(),
            "discord://-/users/99".to_owned(),
        ];
        assert!(link_argument(&mut arguments).is_err());
    }

    #[test]
    fn other_arguments_are_left_alone() {
        let mut arguments = vec!["session".to_owned(), "status".to_owned()];
        assert_eq!(link_argument(&mut arguments).unwrap(), None);
        assert_eq!(arguments, ["session", "status"]);
    }
}

/// The renderer `--renderer` asks for; without it, the setting decides.
fn renderer_argument(arguments: &mut Vec<String>) -> Result<Option<&'static str>> {
    let Some(index) = arguments
        .iter()
        .position(|argument| argument == "--renderer")
    else {
        return Ok(None);
    };
    let renderer = match arguments.get(index + 1).map(String::as_str) {
        Some("gpu") => "gpu",
        Some("software") => "software",
        _ => bail!("--renderer requires gpu or software"),
    };
    arguments.drain(index..index + 2);
    if arguments.iter().any(|argument| argument == "--renderer") {
        bail!("specify --renderer once");
    }
    Ok(Some(renderer))
}
