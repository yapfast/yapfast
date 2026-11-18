//! Commands from `scripts/ui-session`, on its private display only: it
//! appends lines to the file `YAPFAST_UI_TEST_CONTROL` names, as it does for
//! the Slint app (`test_control` in `src/ui.rs`). A thread of its own reads
//! what was added to the file twice a second and hands each line written to
//! its end to `update`, so the window's own thread stays asleep between
//! commands.
//!
//! Understood: `theme ID`, `language CODE`, `stage PATH`, `quit`, and the
//! window's (`window ...` in `scripts/ui-session`): `close` as its close
//! button, `open` or `create` as the tray's Open, `destroy` to the tray
//! without trimming, `hide`, `show`, `minimize`, `trim` and `stats`. The
//! Slint app's others (`pick`, ...) come with task Q04 and are refused
//! meanwhile.

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::futures::stream;
use iced::{Task, window};

use crate::app::App;
use crate::message::Message;

/// The control file, under the private test display alone.
pub fn path() -> Option<PathBuf> {
    let isolated = std::env::var("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY").as_deref() == Ok("1");
    isolated
        .then(|| std::env::var_os("YAPFAST_UI_TEST_CONTROL"))
        .flatten()
        .map(PathBuf::from)
}

/// Watches the file and brings each new line as [`Message::Control`].
pub fn watch(path: PathBuf) -> Task<Message> {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    let reader = move || {
        let mut handled = 0;
        loop {
            std::thread::sleep(Duration::from_millis(500));
            for line in finished_lines(&path, &mut handled) {
                if sender.send(line).is_err() {
                    return;
                }
            }
        }
    };
    if let Err(error) = std::thread::Builder::new()
        .name("anomi-control".into())
        .spawn(reader)
    {
        tracing::warn!(%error, "could not watch the test control file");
        return Task::none();
    }
    let lines = stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|line| (line, receiver))
    });
    Task::run(lines, Message::Control)
}

/// The lines added to the file past its first `handled` bytes and written
/// to their end, read without what came before: a line caught half written
/// waits for the next read, and a file begun again is read from its start.
fn finished_lines(path: &Path, handled: &mut u64) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    if file.metadata().is_ok_and(|file| file.len() < *handled) {
        *handled = 0;
    }
    let mut added = Vec::new();
    if file.seek(SeekFrom::Start(*handled)).is_err() || file.read_to_end(&mut added).is_err() {
        return Vec::new();
    }
    let finished = added
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |end| end + 1);
    *handled += finished as u64;
    String::from_utf8_lossy(&added[..finished])
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Runs one command.
pub fn run(app: &mut App, line: &str) -> Task<Message> {
    tracing::info!(command = line, "test window control");
    let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
    match verb {
        "theme" => {
            if !app.choose_theme(rest) {
                tracing::warn!(theme = rest, "no such theme");
            }
            Task::none()
        }
        "language" => {
            crate::language::choose(app, rest);
            Task::none()
        }
        // A file for the composer's next message, as a paste would.
        "stage" => app.dispatch(yapfast_core::application::Action::Stage(vec![
            PathBuf::from(rest),
        ])),
        "minimize" => app
            .window
            .map_or_else(Task::none, |id| window::minimize(id, true)),
        "hide" | "show" => app.window.map_or_else(Task::none, |id| {
            let mode = if verb == "hide" {
                window::Mode::Hidden
            } else {
                window::Mode::Windowed
            };
            window::set_mode(id, mode)
        }),
        "close" => crate::platform::window::close(app),
        "open" | "create" => crate::platform::window::open(app, None),
        // To the tray whether there's one or not; the trim waits for `trim`.
        "destroy" => crate::platform::forget::window(app),
        "trim" => {
            anomi_desktop::memory::trim();
            Task::none()
        }
        "stats" => {
            anomi_desktop::memory::stats();
            Task::none()
        }
        "quit" => app.quit(),
        _ => {
            tracing::warn!(
                command = line,
                "the iced interface doesn't take this command yet"
            );
            Task::none()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    #[test]
    fn a_line_caught_half_written_waits_for_its_end() {
        let path = std::env::temp_dir().join(format!("anomi-control-{}", std::process::id()));
        std::fs::write(&path, "theme sakura\nlang").unwrap();
        let mut handled = 0;
        assert_eq!(super::finished_lines(&path, &mut handled), ["theme sakura"]);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all("uage ru\r\nquit\n".as_bytes()).unwrap();
        assert_eq!(
            super::finished_lines(&path, &mut handled),
            ["language ru", "quit"]
        );
        assert!(super::finished_lines(&path, &mut handled).is_empty());
        // The session wrote the file anew: read from its start.
        std::fs::write(&path, "close\n").unwrap();
        assert_eq!(super::finished_lines(&path, &mut handled), ["close"]);
        std::fs::remove_file(path).unwrap();
    }
}
