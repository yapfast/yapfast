//! Opens external links and Discord's captcha in the desktop's browser.
//! On the private test display, and in tests, no page opens.

#[cfg(all(unix, not(target_os = "macos")))]
use std::{
    io::ErrorKind,
    process::{Command, Stdio},
};

/// Opens `page` in the default browser, and logs why if it couldn't.
pub fn open(page: &str) {
    if cfg!(test) || std::env::var("YAPFAST_FRAMEWORK_ISOLATED_DISPLAY").as_deref() == Ok("1") {
        tracing::info!("not opening a page from the test display");
        return;
    }
    if let Err(error) = open_default(page) {
        tracing::warn!(%error, "could not open a page in the browser");
    }
}

#[cfg(target_os = "linux")]
fn open_default(page: &str) -> std::io::Result<()> {
    open_linux(page, start_opener)
}

#[cfg(not(target_os = "linux"))]
fn open_default(page: &str) -> std::io::Result<()> {
    #[cfg(all(unix, not(target_os = "macos")))]
    match start_opener(opener("xdg-open", &[page])) {
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        result => return result,
    }
    webbrowser::open(page)
}

/// Let a desktop opener resolve the association. GIO can do this when
/// xdg-utils is absent, as on some GNOME installations.
#[cfg(target_os = "linux")]
fn open_linux(
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

/// Give the browser the user's desktop settings without passing along
/// library paths from Yapfast's development shell or Nix wrapper.
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{ffi::OsStr, io::ErrorKind};

    use super::open_linux;

    #[test]
    fn external_and_loopback_urls_use_the_desktop_opener() {
        for url in ["https://example.com/", "http://127.0.0.1:9000/challenge"] {
            let mut starts = 0;
            open_linux(url, |command| {
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
        open_linux(url, |command| {
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
            let failed = open_linux("https://example.com/", |_| {
                starts += 1;
                Err(error.into())
            });
            assert_eq!(failed.unwrap_err().kind(), error);
            assert_eq!(starts, expected_starts);
        }
    }
}
