//! Opens external links and Discord's captcha in the desktop's browser.
//! On the private test display, and in tests, no page opens: the host's
//! browser would show it.

#[cfg(all(unix, not(target_os = "macos")))]
use std::{
    io::ErrorKind,
    process::{Command, Stdio},
};

#[cfg(test)]
thread_local! {
    static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Browser requests on this test's thread, without launching a browser.
#[cfg(test)]
pub(crate) fn take_opened() -> Vec<String> {
    OPENED.with(|opened| std::mem::take(&mut *opened.borrow_mut()))
}

/// Opens `page` in the default browser, and logs why if it couldn't.
pub fn open(page: &str) {
    #[cfg(test)]
    OPENED.with(|opened| opened.borrow_mut().push(page.to_owned()));
    if cfg!(test) || std::env::var("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY").as_deref() == Ok("1") {
        tracing::info!(page, "not opening a page from the test display");
        return;
    }
    if let Err(error) = open_default(page) {
        tracing::warn!(%error, "could not open a page in the browser");
    }
}

/// The desktop's own openers first, which follow its browser association;
/// the `webbrowser` crate only where neither is installed, as on KDE or
/// XFCE without xdg-utils, or a setup that names its browser in $BROWSER.
fn open_default(page: &str) -> std::io::Result<()> {
    #[cfg(all(unix, not(target_os = "macos")))]
    match open_desktop(page, start_opener) {
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        result => return result,
    }
    webbrowser::open(page)
}

/// Lets a desktop opener resolve the association: `xdg-open`, or GIO where
/// xdg-utils is absent, as on some GNOME installations.
#[cfg(all(unix, not(target_os = "macos")))]
fn open_desktop(
    page: &str,
    mut start: impl FnMut(Command) -> std::io::Result<()>,
) -> std::io::Result<()> {
    match start(opener("xdg-open", &[page])) {
        Err(error) if error.kind() == ErrorKind::NotFound => start(opener("gio", &["open", page])),
        result => result,
    }
}

/// A thread waits for the opener, so a failure shows in the log.
#[cfg(all(unix, not(target_os = "macos")))]
fn start_opener(mut command: Command) -> std::io::Result<()> {
    let mut child = command.spawn()?;
    std::thread::Builder::new()
        .name("browser-opener".into())
        .spawn(move || match child.wait() {
            Ok(status) if !status.success() => {
                tracing::warn!(%status, "the browser did not open a page");
            }
            Err(error) => tracing::warn!(%error, "could not wait for the browser"),
            Ok(_) => {}
        })?;
    Ok(())
}

/// Gives the browser the user's desktop settings without `LD_LIBRARY_PATH`:
/// the libraries Yapfast was started to find, as from its development shell
/// or a Nix wrapper, are other versions than the browser's, and Firefox
/// exits at once on them.
#[cfg(all(unix, not(target_os = "macos")))]
fn opener(program: &str, arguments: &[&str]) -> Command {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .env_remove("LD_LIBRARY_PATH")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use std::{ffi::OsStr, io::ErrorKind};

    use super::open_desktop;

    #[test]
    fn external_and_loopback_urls_use_the_desktop_opener() {
        for url in ["https://example.com/", "http://127.0.0.1:9000/challenge"] {
            let mut starts = 0;
            open_desktop(url, |command| {
                starts += 1;
                assert_eq!(command.get_program(), "xdg-open");
                assert!(command.get_args().eq([OsStr::new(url)]));
                assert!(
                    command
                        .get_envs()
                        .eq([(OsStr::new("LD_LIBRARY_PATH"), None)])
                );
                Ok(())
            })
            .unwrap();
            assert_eq!(starts, 1);
        }
    }

    #[test]
    fn missing_xdg_open_falls_back_to_gio() {
        let url = "https://example.com/";
        let mut starts = 0;
        open_desktop(url, |command| {
            starts += 1;
            if starts == 1 {
                assert_eq!(command.get_program(), "xdg-open");
                return Err(ErrorKind::NotFound.into());
            }
            assert_eq!(command.get_program(), "gio");
            assert!(command.get_args().eq(["open", url].map(OsStr::new)));
            assert!(
                command
                    .get_envs()
                    .eq([(OsStr::new("LD_LIBRARY_PATH"), None)])
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(starts, 2);
    }

    #[test]
    fn reports_opener_failures() {
        for (error, expected_starts) in [(ErrorKind::NotFound, 2), (ErrorKind::PermissionDenied, 1)]
        {
            let mut starts = 0;
            let failed = open_desktop("https://example.com/", |_| {
                starts += 1;
                Err(error.into())
            });
            assert_eq!(failed.unwrap_err().kind(), error);
            assert_eq!(starts, expected_starts);
        }
    }
}
