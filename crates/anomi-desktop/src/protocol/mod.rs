//! Opening Discord's own protocol links in Yapfast: a current-user
//! protocol registration on Windows, and the person's default
//! applications on Linux. Yapfast's own `yapfast:` scheme is registered by
//! its installer or desktop entry; this claims `discord:` only when
//! Settings says to, and gives the previous handler back when turned off.

use std::path::Path;

/// Comes before the link a desktop hands the app, as its registration
/// writes it: `yapfast --open-link LINK`. Both clients read it so.
pub const OPEN_LINK: &str = "--open-link";

/// Whether this platform can claim Discord's protocol links.
pub const SUPPORTED: bool = cfg!(any(windows, target_os = "linux"));

/// Whether Discord protocol links open in this Yapfast.
pub fn discord_enabled(root: &Path) -> bool {
    platform::enabled(root)
}

/// Claims Discord protocol links for this Yapfast, or gives them back.
pub fn set_discord_enabled(root: &Path, enabled: bool) -> std::io::Result<()> {
    platform::set(root, enabled)
}

/// Points a claimed registration at this Yapfast after its folder moved
/// or an update replaced it. A handler someone else chose since is left.
pub fn refresh(root: &Path) {
    if let Err(error) = platform::refresh(root) {
        tracing::warn!(target: crate::LOG, %error, "could not update Discord link handling");
    }
}

/// This Yapfast as a command: itself, its data folder when it isn't the
/// usual one, then the link the desktop hands it.
#[cfg(windows)]
fn command(root: &Path) -> std::io::Result<Vec<String>> {
    use crate::autostart::path_text;
    let mut command = vec![path_text(std::env::current_exe()?)?];
    let usual = yapfast_core::default_directory().ok();
    if usual.as_deref() != Some(root) {
        command.extend([
            "--data-dir".to_owned(),
            path_text(std::path::absolute(root)?)?,
        ]);
    }
    Ok(command)
}

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(any(windows, target_os = "linux")))]
use none as platform;
#[cfg(windows)]
use windows as platform;
// `crate::notify` registers the toasts with it.
#[cfg(windows)]
pub(crate) use windows::set_string;

#[cfg(not(any(windows, target_os = "linux")))]
mod none {
    use std::path::Path;

    pub fn enabled(_: &Path) -> bool {
        false
    }

    pub fn set(_: &Path, _: bool) -> std::io::Result<()> {
        Err(std::io::Error::other(
            "opening Discord links here isn't supported on this platform",
        ))
    }

    pub fn refresh(_: &Path) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux;
