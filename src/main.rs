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
    // First, so nothing a desktop handed over with the link is read as an option.
    let handed = handed_link(&mut arguments);
    let renderer = renderer_argument(&mut arguments)?;
    let background = flag_argument(&mut arguments, "--background");
    let mut links: Vec<String> = handed.clone().ok().flatten().into_iter().collect();
    if !arguments
        .first()
        .is_some_and(|argument| argument == "session")
    {
        links.extend(link_arguments(&mut arguments));
    }
    if links.len() > 1 {
        bail!("specify one Discord link");
    }

    #[cfg(all(windows, feature = "desktop"))]
    match data_directory(&arguments) {
        Some(directory) if !has_console() => logging::init_to_file(&directory.join("yapfast.log")),
        _ => logging::init(),
    }
    #[cfg(not(all(windows, feature = "desktop")))]
    logging::init();
    if let Err(refused) = handed {
        tracing::warn!(
            refused,
            "a handed link came as several arguments; starting without it"
        );
    }
    let open_link = links.pop().and_then(|link| {
        let written = written_link(&link);
        if written.is_none() {
            tracing::info!("Yapfast has no place for the link it was handed; starting without it");
        }
        written
    });
    yapfast_core::init_tls()?;

    let runtime = BackgroundRuntime::new().context("could not start background workers")?;
    #[cfg(feature = "matrix")]
    if arguments
        .first()
        .is_some_and(|argument| argument == "session")
    {
        return runtime.block_on(cli::session(&arguments[1..]));
    }
    let start = Start {
        renderer,
        background,
        arguments: &arguments,
        open_link,
    };
    match arguments.as_slice() {
        [] => launch(runtime, yapfast_core::default_directory()?, false, start),
        [argument] if argument == "--preview" => {
            launch(runtime, yapfast_core::default_directory()?, true, start)
        }
        [argument, path] if argument == "--data-dir" => launch(runtime, path.into(), false, start),
        [preview, argument, path] if preview == "--preview" && argument == "--data-dir" => {
            launch(runtime, path.into(), true, start)
        }
        #[cfg(feature = "desktop")]
        [argument] if argument == RELEASE_DISCORD_LINKS => {
            protocol::set_discord_enabled(&yapfast_core::default_directory()?, false)
                .context("could not give Discord links back")
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
                "Usage: yapfast [--preview] [--data-dir PATH] [--renderer gpu|software] [--background] [LINK]\n       yapfast [--smoke-test | --help | --release-discord-links]\n       yapfast --call-experiment [--resume PATH]\n       yapfast [--voice-demo | --audio-devices]\n       yapfast session login --homeserver URL --user USER --password-stdin [--data-dir PATH]\n       yapfast session status [--data-dir PATH]\n       yapfast session recover --key-stdin [--data-dir PATH]\n       yapfast session logout [--data-dir PATH]\n\nWithout arguments, opens native Slint chat and restores a saved session.\n--renderer selects Skia Software or Skia GPU for this start, over the setting.\nIf Skia GPU fails to start, Yapfast restarts with Skia Software.\n--background starts in the system tray with a saved session, as starting at sign-in does.\nLINK opens a Discord server, channel, message, profile or invite link in Yapfast.\n--release-discord-links gives discord:// links back to the app that had them, as uninstalling does.\n--preview opens local sample conversations without connecting to Matrix.\n--smoke-test checks the linked stack without signing in or opening audio devices.\n--call-experiment creates disposable local accounts and checks encrypted synthetic audio. Start scripts/test-stack up first.\n--voice-demo creates two local accounts and a server with encrypted text and voice channels.\n--audio-devices lists native audio devices and cameras without recording.\nSession commands support password sign-in. Secrets are read from stdin."
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

/// How Yapfast starts its window, besides its folder and the preview.
#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
struct Start<'a> {
    renderer: Option<&'static str>,
    background: bool,
    /// What a restart starts with: the arguments without `--renderer`,
    /// `--background` or a link.
    arguments: &'a [String],
    /// A link handed to this start, as one word.
    open_link: Option<String>,
}

#[cfg(feature = "desktop")]
fn launch(
    runtime: BackgroundRuntime,
    root: std::path::PathBuf,
    preview: bool,
    start: Start<'_>,
) -> Result<()> {
    let renderer = start.renderer.map(|renderer| match renderer {
        "gpu" => graphics::Renderer::Gpu,
        _ => graphics::Renderer::Software,
    });
    let fallback = std::env::var(GPU_FALLBACK_REASON).ok();
    #[cfg(feature = "discord")]
    yapfast_core::keep_discord_files(&root);
    match ui::run(
        runtime,
        root,
        preview,
        renderer,
        fallback,
        start.background,
        start.open_link.clone(),
    )? {
        ui::Exit::Closed => Ok(()),
        // The GPU failed before anything opened the link: the next start does.
        ui::Exit::UseSoftware(reason) => {
            relaunch(start.arguments, Some(&reason), start.open_link.as_deref())
        }
        ui::Exit::Restart => relaunch(start.arguments, None, None),
    }
}

/// The window system allows one renderer per process, so a failed GPU start,
/// or a changed renderer setting, continues in a new process after this one
/// released its session and runtime. `arguments` no longer hold `--renderer`,
/// so a restart follows the setting, nor `--background`: the window shows.
#[cfg(feature = "desktop")]
fn relaunch(arguments: &[String], gpu_failure: Option<&str>, link: Option<&str>) -> Result<()> {
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
    // Last, as nothing after it is read.
    if let Some(link) = link {
        command.args([OPEN_LINK, link]);
    }
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
fn launch(_: BackgroundRuntime, _: std::path::PathBuf, _: bool, _: Start<'_>) -> Result<()> {
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

/// Comes before the link a desktop hands Yapfast, as its registrations
/// write it: `yapfast --open-link LINK`.
const OPEN_LINK: &str = "--open-link";

/// Gives `discord:` back to the app that had it; the uninstaller runs it.
#[cfg(feature = "desktop")]
const RELEASE_DISCORD_LINKS: &str = "--release-discord-links";

/// The link after `--open-link`, taking both, and anything after them, out
/// of `arguments`. Windows writes a link into the registered command line
/// as it comes, so one holding a quote splits into more arguments there:
/// none of them is ever read as an option, and with more than one the link
/// goes too, `Err` holding how many were refused. Without a link, as a
/// desktop entry's `%u` is when the menu starts it, there's none.
fn handed_link(arguments: &mut Vec<String>) -> Result<Option<String>, usize> {
    let Some(index) = arguments.iter().position(|argument| argument == OPEN_LINK) else {
        return Ok(None);
    };
    let mut handed = arguments.split_off(index);
    match handed.len() {
        1 => Ok(None),
        2 => Ok(handed.pop()),
        count => Err(count - 1),
    }
}

/// Links given as arguments, taking them out of `arguments`: anything
/// under `discord:` or `yapfast:`, whether or not Yapfast has a place for
/// it, and Discord's web links that it has.
fn link_arguments(arguments: &mut Vec<String>) -> Vec<String> {
    arguments
        .extract_if(.., |argument| {
            argument.split_once(':').is_some_and(|(scheme, _)| {
                scheme.eq_ignore_ascii_case("discord") || scheme.eq_ignore_ascii_case("yapfast")
            }) || written_link(argument).is_some()
        })
        .collect()
}

/// A link as one word, when it names a place this Yapfast can open.
fn written_link(link: &str) -> Option<String> {
    #[cfg(all(feature = "desktop", feature = "discord"))]
    return yapfast_core::discord::discord_link(link);
    #[cfg(not(all(feature = "desktop", feature = "discord")))]
    {
        let _ = link;
        None
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

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(arguments: &[&str]) -> Vec<String> {
        arguments
            .iter()
            .map(|&argument| argument.to_owned())
            .collect()
    }

    #[test]
    fn a_handed_link_is_never_read_as_an_option() {
        let mut arguments = strings(&["--data-dir", "folder", OPEN_LINK, "--preview"]);
        assert_eq!(handed_link(&mut arguments), Ok(Some("--preview".into())));
        assert_eq!(arguments, ["--data-dir", "folder"]);
        // `discord://-/channels/1/2" --data-dir "\\host\share`, put
        // into `"yapfast.exe" --open-link "%1"` as Windows does.
        let mut arguments = strings(&[
            OPEN_LINK,
            "discord://-/channels/1/2",
            "--data-dir",
            r"\\host\share",
        ]);
        assert_eq!(handed_link(&mut arguments), Err(3));
        assert!(arguments.is_empty());
        let mut arguments = strings(&["--background", OPEN_LINK]);
        assert_eq!(handed_link(&mut arguments), Ok(None));
        assert_eq!(arguments, ["--background"]);
    }

    #[test]
    fn every_link_argument_is_taken_out() {
        let mut arguments = strings(&[
            "--data-dir",
            "folder",
            "discord://-/library",
            "YAPFAST://channels/@me",
            "https://example.com/channels/10/11",
        ]);
        assert_eq!(
            link_arguments(&mut arguments),
            ["discord://-/library", "YAPFAST://channels/@me"]
        );
        assert_eq!(
            arguments,
            ["--data-dir", "folder", "https://example.com/channels/10/11"]
        );
        let mut arguments = strings(&["session", "status"]);
        assert!(link_arguments(&mut arguments).is_empty());
        assert_eq!(arguments, ["session", "status"]);
    }

    #[cfg(all(feature = "desktop", feature = "discord"))]
    #[test]
    fn a_discord_web_link_is_taken_out_and_written_as_one_word() {
        let mut arguments = strings(&["https://discord.com/channels/10/11?a b"]);
        let links = link_arguments(&mut arguments);
        assert!(arguments.is_empty());
        assert_eq!(
            written_link(&links[0]).as_deref(),
            Some("https://discord.com/channels/10/11?a%20b")
        );
        assert_eq!(written_link("discord://-/settings/account"), None);
    }
}
