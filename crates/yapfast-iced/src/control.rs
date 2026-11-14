//! The command file the private test display drives the window through, as
//! the Slint client's `test_control` in `src/ui.rs` reads it:
//! `scripts/ui-session` appends a line per command to the file named by
//! `YAPFAST_UI_TEST_CONTROL`, and the window carries out each new one.
//!
//! The commands it knows: `stats` (glibc's heap to the log), `trim` (free
//! heap back to the system), `close` and `quit` (no tray yet, so both end
//! the app), `hide` and `show`, `stage PATH` and `stage-aside PATH` (a file
//! for the next message), `pick KIND PATH` (a picture for a profile or a
//! server's settings). `open`, `create`, `destroy` and `theme ID` need the
//! tray and the themes, which the iced client doesn't have yet; it logs
//! them.

use std::{collections::VecDeque, path::PathBuf, time::Duration};

use iced::futures::{Stream, stream};
use yapfast_core::{
    application::Action,
    chat::{ProfilePicture, ServerPicture},
};

/// How often the file is read: as the Slint client reads it.
const EVERY: Duration = Duration::from_millis(500);

/// The command file, only on the private test display.
pub fn path() -> Option<PathBuf> {
    (std::env::var("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY").as_deref() == Ok("1"))
        .then(|| std::env::var_os("YAPFAST_UI_TEST_CONTROL"))
        .flatten()
        .map(PathBuf::from)
}

/// Each new line of the file, as it's written.
pub fn commands(path: PathBuf) -> impl Stream<Item = String> + Send + 'static {
    let state = (path, 0usize, VecDeque::new());
    stream::unfold(state, |(path, mut handled, mut waiting)| async move {
        loop {
            if let Some(command) = waiting.pop_front() {
                return Some((command, (path, handled, waiting)));
            }
            tokio::time::sleep(EVERY).await;
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for command in text.lines().skip(handled) {
                handled += 1;
                waiting.push_back(command.to_owned());
            }
        }
    })
}

/// What a command asks of the window.
pub enum Command {
    Stats,
    Trim,
    Quit,
    Hide,
    Show,
    /// An action for the application, as the window's own controls would
    /// dispatch.
    Act(Action),
    /// A command for what the iced client doesn't have yet.
    Later,
    Unknown,
}

pub fn parse(command: &str) -> Command {
    match command {
        "stats" => Command::Stats,
        "trim" => Command::Trim,
        "close" | "quit" => Command::Quit,
        "hide" => Command::Hide,
        "show" => Command::Show,
        "open" | "create" | "destroy" => Command::Later,
        _ => {
            if let Some(path) = command.strip_prefix("stage ") {
                Command::Act(Action::Stage(vec![PathBuf::from(path)]))
            } else if let Some(path) = command.strip_prefix("stage-aside ") {
                Command::Act(Action::StageAside(vec![PathBuf::from(path)]))
            } else if let Some(rest) = command.strip_prefix("pick ") {
                let (kind, path) = rest.split_once(' ').unwrap_or_default();
                Command::Act(pick(kind, PathBuf::from(path)))
            } else if command.starts_with("theme ") {
                Command::Later
            } else {
                Command::Unknown
            }
        }
    }
}

/// `pick KIND PATH`: a picture chosen as the file chooser would.
fn pick(kind: &str, path: PathBuf) -> Action {
    let server = |kind| Action::PickServerPicture {
        kind,
        path: path.clone(),
    };
    match kind {
        "icon" => server(ServerPicture::Icon),
        "server-banner" => server(ServerPicture::Banner),
        "splash" => server(ServerPicture::Splash),
        "role-icon" => server(ServerPicture::RoleIcon),
        "emoji" => server(ServerPicture::Emoji),
        "sticker" => server(ServerPicture::Sticker),
        "banner" => Action::PickProfilePicture {
            kind: ProfilePicture::Banner,
            path,
        },
        _ => Action::PickProfilePicture {
            kind: ProfilePicture::Avatar,
            path,
        },
    }
}

/// Gives freed heap pages back to the system, to tell memory in use from
/// memory glibc keeps for reuse.
pub fn trim() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        // SAFETY: malloc_trim only releases free memory held by the allocator.
        unsafe { malloc_trim(0) };
    }
}

/// Writes glibc's heap totals to standard error, which the session logs.
pub fn allocator_stats() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        unsafe extern "C" {
            fn malloc_stats();
        }
        // SAFETY: malloc_stats only reads the allocator's own counters.
        unsafe { malloc_stats() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sessions_commands_are_read_as_the_slint_client_reads_them() {
        assert!(matches!(parse("stats"), Command::Stats));
        assert!(matches!(parse("close"), Command::Quit));
        assert!(matches!(
            parse("stage testing/a.png"),
            Command::Act(Action::Stage(paths)) if paths == [PathBuf::from("testing/a.png")]
        ));
        assert!(matches!(
            parse("pick banner b.png"),
            Command::Act(Action::PickProfilePicture {
                kind: ProfilePicture::Banner,
                ..
            })
        ));
        assert!(matches!(parse("theme sakura"), Command::Later));
        assert!(matches!(parse("dance"), Command::Unknown));
    }
}
