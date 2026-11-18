//! Commands from `scripts/ui-session`, on its private display only: it
//! appends lines to the file `YAPFAST_UI_TEST_CONTROL` names, as it does for
//! the Slint app (`test_control` in `src/ui.rs`). A thread of its own reads
//! what was added to the file twice a second and hands each line written to
//! its end to `update`, so the window's own thread stays asleep between
//! commands.
//!
//! The commands are the Slint app's, each read once by [`parse`]: `theme
//! ID`; `stage PATH` and `stage-aside PATH`, a file for the channel's or the
//! thread beside's next message; `pick KIND PATH`, a file chosen as a file
//! dialog would choose it (the profile editor's `avatar` and `banner`, a
//! server's `icon`, `server-banner`, `splash`, `role-icon`, `emoji` and
//! `sticker`, Appearance's `font` and `theme`; any other kind is the
//! avatar's, as the Slint app takes it); the window's (`window ...` in
//! `scripts/ui-session`): `close` as its close button, `open` or `create` as
//! the tray's Open, `destroy` to the tray without trimming, `hide`, `show`,
//! `minimize`, `trim` and `stats`; and `quit`. And one of its own: `language
//! CODE`, which the Slint app ignores.

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::futures::stream;
use iced::{Task, window};

use yapfast_core::application::Action;
use yapfast_core::chat::{ProfilePicture, ServerPicture};

use crate::app::App;
use crate::intent::settings::Due;
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

/// A line of the file, read.
#[derive(Debug, PartialEq)]
enum Command {
    Theme(String),
    Language(String),
    /// A file for the channel's next message, as a paste would.
    Stage(PathBuf),
    /// A file for the next message of the thread beside the channel.
    StageAside(PathBuf),
    Pick(Pick, PathBuf),
    Window(Window),
    Quit,
}

/// What a picked file is for.
#[derive(Debug, PartialEq)]
enum Pick {
    Profile(ProfilePicture),
    Server(ServerPicture),
    /// A font file added in Appearance.
    Font,
    /// A theme file imported in Appearance.
    Theme,
}

/// What `window ...` asks of the window.
#[derive(Debug, PartialEq)]
enum Window {
    Close,
    Open,
    Destroy,
    Hide,
    Show,
    Minimize,
    Trim,
    Stats,
}

/// The command a line gives, or none for a line the Slint app ignores too.
fn parse(line: &str) -> Option<Command> {
    let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
    let path = || PathBuf::from(rest);
    Some(match verb {
        "theme" => Command::Theme(rest.to_owned()),
        "language" => Command::Language(rest.to_owned()),
        "stage" => Command::Stage(path()),
        "stage-aside" => Command::StageAside(path()),
        "pick" => {
            let (kind, path) = rest.split_once(' ').unwrap_or_default();
            Command::Pick(pick(kind), PathBuf::from(path))
        }
        "quit" => Command::Quit,
        _ => Command::Window(match line {
            "close" => Window::Close,
            "open" | "create" => Window::Open,
            "destroy" => Window::Destroy,
            "hide" => Window::Hide,
            "show" => Window::Show,
            "minimize" => Window::Minimize,
            "trim" => Window::Trim,
            "stats" => Window::Stats,
            _ => return None,
        }),
    })
}

fn pick(kind: &str) -> Pick {
    match kind {
        "banner" => Pick::Profile(ProfilePicture::Banner),
        "icon" => Pick::Server(ServerPicture::Icon),
        "server-banner" => Pick::Server(ServerPicture::Banner),
        "splash" => Pick::Server(ServerPicture::Splash),
        "role-icon" => Pick::Server(ServerPicture::RoleIcon),
        "emoji" => Pick::Server(ServerPicture::Emoji),
        "sticker" => Pick::Server(ServerPicture::Sticker),
        "font" => Pick::Font,
        "theme" => Pick::Theme,
        _ => Pick::Profile(ProfilePicture::Avatar),
    }
}

/// Runs one line.
pub fn run(app: &mut App, line: &str) -> Task<Message> {
    tracing::info!(command = line, "test window control");
    let Some(command) = parse(line) else {
        tracing::warn!(command = line, "no such test window command");
        return Task::none();
    };
    match command {
        Command::Theme(id) => {
            if !crate::intent::settings::choose_theme(app, &id) {
                tracing::warn!(theme = id, "no such theme");
            }
            Task::none()
        }
        Command::Language(code) => {
            crate::language::choose(app, &code);
            Task::none()
        }
        Command::Stage(path) => app.dispatch(Action::Stage(vec![path])),
        Command::StageAside(path) => app.dispatch(Action::StageAside(vec![path])),
        Command::Pick(Pick::Profile(kind), path) => {
            app.dispatch(Action::PickProfilePicture { kind, path })
        }
        Command::Pick(Pick::Server(kind), path) => {
            app.dispatch(Action::PickServerPicture { kind, path })
        }
        Command::Pick(Pick::Font, path) => {
            Task::done(Message::Settings(Due::FontsChosen(vec![path])))
        }
        Command::Pick(Pick::Theme, path) => Task::done(Message::Settings(Due::ThemeChosen(path))),
        Command::Window(command) => window(app, command),
        Command::Quit => app.quit(),
    }
}

fn window(app: &mut App, command: Window) -> Task<Message> {
    let mode = |mode| {
        app.window
            .map_or_else(Task::none, |id| window::set_mode(id, mode))
    };
    match command {
        Window::Close => crate::platform::window::close(app),
        Window::Open => crate::platform::window::open(app, None),
        // To the tray whether there's one or not; the trim waits for `trim`.
        Window::Destroy => crate::platform::forget::window(app),
        Window::Hide => mode(window::Mode::Hidden),
        Window::Show => mode(window::Mode::Windowed),
        Window::Minimize => app
            .window
            .map_or_else(Task::none, |id| window::minimize(id, true)),
        Window::Trim => {
            anomi_desktop::memory::trim();
            Task::none()
        }
        Window::Stats => {
            anomi_desktop::memory::stats();
            Task::none()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::path::PathBuf;

    use yapfast_core::chat::{ProfilePicture, ServerPicture};

    use super::{Command, Pick, Window, parse};

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

    /// Every line `scripts/ui-session do` writes is read as the Slint app
    /// reads it (`test_control` in `src/ui.rs`).
    #[test]
    fn each_command_of_the_session_is_read_as_the_slint_app_reads_it() {
        let at = |path: &str| PathBuf::from(path);
        let picks = [
            ("avatar", Pick::Profile(ProfilePicture::Avatar)),
            ("banner", Pick::Profile(ProfilePicture::Banner)),
            ("icon", Pick::Server(ServerPicture::Icon)),
            ("server-banner", Pick::Server(ServerPicture::Banner)),
            ("splash", Pick::Server(ServerPicture::Splash)),
            ("role-icon", Pick::Server(ServerPicture::RoleIcon)),
            ("emoji", Pick::Server(ServerPicture::Emoji)),
            ("sticker", Pick::Server(ServerPicture::Sticker)),
            ("font", Pick::Font),
            ("theme", Pick::Theme),
            // Any other kind is the avatar's, as the Slint app takes it.
            ("portrait", Pick::Profile(ProfilePicture::Avatar)),
        ];
        for (kind, pick) in picks {
            assert_eq!(
                parse(&format!("pick {kind} /home/a b.png")),
                Some(Command::Pick(pick, at("/home/a b.png"))),
                "{kind}"
            );
        }
        assert_eq!(
            parse("stage /home/a b.png"),
            Some(Command::Stage(at("/home/a b.png")))
        );
        assert_eq!(
            parse("stage-aside /x.png"),
            Some(Command::StageAside(at("/x.png")))
        );
        assert_eq!(
            parse("theme custom:1"),
            Some(Command::Theme("custom:1".into()))
        );
        assert_eq!(parse("language ru"), Some(Command::Language("ru".into())));
        assert_eq!(parse("quit"), Some(Command::Quit));
        let windows = [
            ("close", Window::Close),
            ("open", Window::Open),
            ("create", Window::Open),
            ("destroy", Window::Destroy),
            ("hide", Window::Hide),
            ("show", Window::Show),
            ("minimize", Window::Minimize),
            ("trim", Window::Trim),
            ("stats", Window::Stats),
        ];
        for (line, window) in windows {
            assert_eq!(parse(line), Some(Command::Window(window)), "{line}");
        }
        assert_eq!(parse("wobble"), None);
        assert_eq!(parse("close now"), None);
    }
}
