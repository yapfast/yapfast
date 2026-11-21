//! The Screen Recording permission, which ScreenCaptureKit needs for any
//! screen, window or sound but Yapfast's own.

use anyhow::{Result, bail};
use objc2_core_graphics::{CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess};

/// What the person is told when the permission is missing.
pub(crate) const DENIED_MESSAGE: &str = "Screen sharing is turned off for Yapfast. Allow it in System Settings → Privacy & Security → Screen & System Audio Recording, then quit and reopen Yapfast and try again.";

/// ScreenCaptureKit's error code for a person who declined, or never allowed.
pub(crate) const USER_DECLINED_CODE: isize = -3801;

/// Whether Yapfast may capture the screen now, without asking.
pub(crate) fn granted() -> bool {
    let granted = CGPreflightScreenCaptureAccess();
    tracing::debug!(granted, "screen recording permission checked");
    granted
}

/// Fails unless Yapfast may capture the screen. When it may not, the system
/// is asked, which shows its prompt the first time and lists Yapfast in the
/// settings; the answer comes too late for this attempt, so it fails either way.
pub(crate) fn ensure() -> Result<()> {
    if granted() {
        return Ok(());
    }
    tracing::warn!("screen recording permission missing; asking the system");
    let granted_now = CGRequestScreenCaptureAccess();
    tracing::info!(
        granted_now,
        "the system answered the screen recording request"
    );
    bail!(DENIED_MESSAGE)
}

/// The message for a ScreenCaptureKit error: the permission's for the codes
/// that mean it is missing, else the system's own words.
pub(crate) fn explain(code: isize, description: &str) -> String {
    if code == USER_DECLINED_CODE {
        DENIED_MESSAGE.to_owned()
    } else {
        format!("Screen capture failed: {description} (code {code})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declined_permission_names_the_setting() {
        let message = explain(USER_DECLINED_CODE, "The user declined TCCs");
        assert!(message.contains("Privacy & Security"));
        assert!(message.contains("Screen & System Audio Recording"));
    }

    #[test]
    fn other_errors_keep_the_systems_words_and_code() {
        let message = explain(-3802, "Failed to start");
        assert!(message.contains("Failed to start"));
        assert!(message.contains("-3802"));
    }
}
