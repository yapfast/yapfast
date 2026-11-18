//! Commands from `scripts/ui-session`, on its private display only: it
//! appends lines to the file `YAPFAST_UI_TEST_CONTROL` names, as it does for
//! the Slint app (`test_control` in `src/ui.rs`). A thread of its own reads
//! the file twice a second and hands each new line to `update`, so the
//! window's own thread stays asleep between commands.
//!
//! Understood: `theme ID`, `language CODE`, `minimize`, `close`, `quit`.
//! The Slint app's others (`pick`, `stage`, `window open`, ...) come with
//! task Q04 and are refused meanwhile.

use std::path::PathBuf;
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
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for line in text.lines().skip(handled) {
                handled += 1;
                if sender.send(line.to_owned()).is_err() {
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
        "minimize" => app
            .window
            .map_or_else(Task::none, |id| window::minimize(id, true)),
        "close" | "quit" => app.quit(),
        _ => {
            tracing::warn!(
                command = line,
                "the iced interface doesn't take this command yet"
            );
            Task::none()
        }
    }
}
