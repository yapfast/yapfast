//! Later starts on a Unix socket beside the lock: each a line with its
//! activation token and link.

use std::path::Path;
use std::time::Duration;

use super::{Launch, SOCKET};

pub(super) fn ask(root: &Path, launch: &Launch) -> std::io::Result<()> {
    use std::io::Write;
    let mut stream = std::os::unix::net::UnixStream::connect(root.join(SOCKET))?;
    writeln!(stream, "{}", launch_line(launch))
}

/// A later start as one socket line: `open`, then its token and link,
/// with `-` for either it hasn't got. Neither has whitespace.
pub(super) fn launch_line(launch: &Launch) -> String {
    format!(
        "open {} {}",
        launch.token.as_deref().unwrap_or("-"),
        launch.link.as_deref().unwrap_or("-")
    )
}

/// Reads a socket line back, including the older `open [token]` shape.
pub(super) fn launch_from_line(line: &str) -> Option<Launch> {
    let mut words = line.split_whitespace();
    if words.next() != Some("open") {
        return None;
    }
    let field = |word: Option<&str>| match word {
        None | Some("-") => None,
        Some(word) => Some(word.to_owned()),
    };
    let token = field(words.next());
    let link = field(words.next());
    if words.next().is_some() {
        return None;
    }
    Some(Launch { token, link })
}

pub(super) struct Listener(Option<std::os::unix::net::UnixListener>);

impl Listener {
    pub(super) fn bind(root: &Path) -> Option<Self> {
        let path = root.join(SOCKET);
        // Holding the lock, this start is the only one: a socket there is a
        // gone Anomi's.
        let _ = std::fs::remove_file(&path);
        match std::os::unix::net::UnixListener::bind(&path) {
            Ok(listener) => Some(Self(Some(listener))),
            Err(error) => {
                tracing::warn!(target: crate::LOG, %error, "starting Anomi again can't show this window");
                None
            }
        }
    }

    pub(super) fn listen(&mut self, opened: impl Fn(Launch) + Send + 'static) {
        let Some(listener) = self.0.take() else {
            return;
        };
        let thread = std::thread::Builder::new()
            .name("anomi-instance".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                use std::io::Read;
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                    let mut request = String::new();
                    // An activation token is short; this caps a stranger's line.
                    if stream.take(4096).read_to_string(&mut request).is_err() {
                        continue;
                    }
                    if let Some(launch) = launch_from_line(&request) {
                        opened(launch);
                    }
                }
            });
        if let Err(error) = thread {
            tracing::warn!(target: crate::LOG, %error, "starting Anomi again can't show this window");
        }
    }
}
