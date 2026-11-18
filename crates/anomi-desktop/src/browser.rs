//! Opens web pages in the browser people chose.

/// Opens `page` in the default browser, and logs why if it couldn't.
pub fn open(page: &str) {
    #[cfg(all(unix, not(target_os = "macos")))]
    match xdg_open(page) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return tracing::warn!(target: crate::LOG, %error, "could not open a page in the browser");
        }
        Ok(()) => return,
    }
    if let Err(error) = webbrowser::open(page) {
        tracing::warn!(target: crate::LOG, %error, "could not open a page in the browser");
    }
}

/// Starts `xdg-open` without `LD_LIBRARY_PATH`: the libraries Yapfast was
/// started to find, as from its development shell or a Nix wrapper, are
/// other versions than the browser's, and Firefox exits at once on them.
/// A thread waits for `xdg-open`, which waits for a browser that wasn't
/// running yet, so a failure shows in the log.
#[cfg(all(unix, not(target_os = "macos")))]
fn xdg_open(page: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("xdg-open")
        .arg(page)
        .env_remove("LD_LIBRARY_PATH")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::Builder::new()
        .name("xdg-open".into())
        .spawn(move || match child.wait() {
            Ok(status) if !status.success() => {
                tracing::warn!(target: crate::LOG, %status, "the browser did not open a page");
            }
            Err(error) => {
                tracing::warn!(target: crate::LOG, %error, "could not wait for the browser")
            }
            Ok(_) => {}
        })?;
    Ok(())
}
