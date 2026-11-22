//! Which Yapfast is running the plugin.
//!
//! A plugin that needs a method added after api 1 asks for the app's version
//! before using it, rather than calling something this build doesn't have.
//! Asking is one host call, so ask once in `activate` and keep the answer in a
//! field.

use crate::Result;
use crate::call::{self, method};

pub use crate::wire::AppInfo;

/// The running Yapfast's version, the person's language, and the theme mode
/// they chose.
pub fn version() -> Result<AppInfo> {
    call::call(method::APP_VERSION, ())
}

/// The running Yapfast's version on its own, as a string to compare against.
pub fn version_string() -> Result<String> {
    Ok(version()?.version)
}

/// The time now, in Unix milliseconds. A plugin has no clock of its own, and
/// the call is answered while it runs.
pub fn now_ms() -> Result<u64> {
    call::call(method::APP_NOW, ())
}

/// Asks Yapfast to ask this plugin again what it draws under the messages on
/// screen, for when its answer changed: decorations are kept, not polled.
pub fn redecorate() -> Result<()> {
    call::ok(method::UI_REDECORATE, serde_json::json!({}))?;
    Ok(())
}

/// Asks Yapfast to ask again how `person` looks, or everyone with `None`:
/// identities are kept, not polled.
pub fn refresh_identity(person: Option<&str>) -> Result<()> {
    call::ok(
        method::IDENTITY_REFRESH,
        serde_json::json!({ "person": person }),
    )?;
    Ok(())
}
