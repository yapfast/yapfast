//! Which Yapfast is running the plugin.
//!
//! A plugin that needs a method added after api 1 asks for the app's version
//! before using it, rather than calling something this build doesn't have.
//! Asking is one host call, so ask once in `activate` and keep the answer in a
//! field.

use crate::call::{self, method};
use crate::Result;

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
