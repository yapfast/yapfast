//! Opens web pages in the browser people chose.

/// Opens `page` in the default browser, and logs why if it couldn't.
pub(super) fn open(page: &str) {
    #[cfg(target_os = "android")]
    {
        crate::android::open_url(page);
    }
    #[cfg(all(unix, not(any(target_os = "macos", target_os = "android"))))]
    match xdg_open(page) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return tracing::warn!(%error, "could not open a page in the browser"),
        Ok(()) => return,
    }
    #[cfg(feature = "desktop")]
    if let Err(error) = webbrowser::open(page) {
        tracing::warn!(%error, "could not open a page in the browser");
    }
    #[cfg(not(any(feature = "desktop", target_os = "android")))]
    let _ = page;
}

/// Starts `xdg-open` without `LD_LIBRARY_PATH`: the libraries Yapfast was
/// started to find, as from its development shell or a Nix wrapper, are
/// other versions than the browser's, and Firefox exits at once on them.
/// A thread waits for `xdg-open`, which waits for a browser that wasn't
/// running yet, so a failure shows in the log.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "android"))))]
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
                tracing::warn!(%status, "the browser did not open a page");
            }
            Err(error) => tracing::warn!(%error, "could not wait for the browser"),
            Ok(_) => {}
        })?;
    Ok(())
}
