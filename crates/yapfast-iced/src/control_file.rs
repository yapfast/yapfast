//! The command file the private test display drives the window through:
//! `scripts/ui-session` appends a line per command to the file named by
//! `YAPFAST_UI_TEST_CONTROL`, and the window carries out each new one.
//!
//! The commands it knows: `stats` (glibc's heap to the log), `trim` (free
//! heap back to the system), `close` (as the window's close button: to the
//! tray, or quit), `destroy` (the window dropped without giving its memory
//! back, which `trim` then does), `open` and `create` (as the tray's Open
//! Yapfast), `quit`, `hide` and `show`, `stage PATH` and `stage-aside PATH`
//! (a file for the next message), `pick KIND PATH` (a picture for a profile
//! or a server's settings), `theme ID` (a theme, built in or custom, as
//! Appearance chooses it).

use std::{collections::VecDeque, path::PathBuf, time::Duration};

use iced::{
    Task,
    futures::{Stream, stream},
};
use yapfast_core::{
    application::Action,
    chat::{ProfilePicture, ServerPicture},
};

use crate::app::{App, Message};

/// How often the file is read.
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
            waiting.extend(finished_lines(&text, &mut handled));
        }
    })
}

/// The lines of `text` past its first `handled` bytes that are written to
/// their end: a line caught half written waits for the next read.
fn finished_lines(text: &str, handled: &mut usize) -> Vec<String> {
    let Some(new) = text.get(*handled..) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for line in new.split_inclusive('\n') {
        let Some(command) = line.strip_suffix('\n') else {
            break;
        };
        *handled += line.len();
        lines.push(command.trim_end_matches('\r').to_owned());
    }
    lines
}

/// What a command asks of the window.
pub enum Command {
    Stats,
    Trim,
    Quit,
    Close,
    Destroy,
    Open,
    Hide,
    Show,
    /// An action for the application, as the window's own controls would
    /// dispatch.
    Act(Action),
    /// Show and keep a theme, by its id.
    Theme(String),
    Unknown,
}

pub fn parse(command: &str) -> Command {
    match command {
        "stats" => Command::Stats,
        "trim" => Command::Trim,
        "quit" => Command::Quit,
        "close" => Command::Close,
        "destroy" => Command::Destroy,
        "open" | "create" => Command::Open,
        "hide" => Command::Hide,
        "show" => Command::Show,
        _ => {
            if let Some(path) = command.strip_prefix("stage ") {
                Command::Act(Action::Stage(vec![PathBuf::from(path)]))
            } else if let Some(path) = command.strip_prefix("stage-aside ") {
                Command::Act(Action::StageAside(vec![PathBuf::from(path)]))
            } else if let Some(rest) = command.strip_prefix("pick ") {
                let (kind, path) = rest.split_once(' ').unwrap_or_default();
                Command::Act(pick(kind, PathBuf::from(path)))
            } else if let Some(id) = command.strip_prefix("theme ") {
                Command::Theme(id.to_owned())
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

/// A line of the test display's control file.
pub(crate) fn control(app: &mut App, command: &str) -> Task<Message> {
    tracing::info!(command, "test window control");
    match parse(command) {
        Command::Stats => allocator_stats(),
        Command::Trim => trim(),
        Command::Quit => return app.quit(),
        Command::Close => return app.close_requested(),
        Command::Destroy => {
            // The window goes without its memory given back: a `trim`
            // after shows what closing frees.
            return app.drop_window();
        }
        Command::Open => return app.open_window(),
        Command::Hide => {
            if let Some(id) = app.shell.window {
                return iced::window::set_mode(id, iced::window::Mode::Hidden);
            }
        }
        Command::Show => {
            if let Some(id) = app.shell.window {
                return iced::window::set_mode(id, iced::window::Mode::Windowed);
            }
        }
        Command::Act(action) => {
            app.client.dispatch(action);
            return app.sync();
        }
        Command::Theme(id) => {
            if !app.choose_theme(&id) {
                tracing::warn!(id, "no such theme");
            }
        }
        Command::Unknown => tracing::warn!(command, "unknown window control"),
    }
    Task::none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_caught_half_written_waits_for_its_end() {
        let mut handled = 0;
        assert_eq!(super::finished_lines("stats\ntri", &mut handled), ["stats"]);
        assert_eq!(
            super::finished_lines("stats\ntrim\n", &mut handled),
            ["trim"]
        );
        assert!(super::finished_lines("stats\ntrim\n", &mut handled).is_empty());
    }

    #[test]
    fn the_sessions_commands_are_read() {
        assert!(matches!(parse("stats"), Command::Stats));
        assert!(matches!(parse("close"), Command::Close));
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
        assert!(matches!(parse("theme sakura"), Command::Theme(id) if id == "sakura"));
        assert!(matches!(parse("open"), Command::Open));
        assert!(matches!(parse("dance"), Command::Unknown));
    }
}
