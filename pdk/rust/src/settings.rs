//! A plugin's settings: the values the person set in Yapfast's plugin page.
//!
//! A setting's key and type are declared in the plugin's manifest, and Yapfast
//! draws the form, checks it and stores it, so a plugin reads a setting the
//! same way whatever a person typed into it. [`set`] writes one of the
//! plugin's own, checked as the page checks the person's typing; something
//! that isn't a setting belongs in [`storage`](crate::storage).

use serde::de::DeserializeOwned;

use crate::Result;
use crate::call::{self, method};
use crate::wire;

/// One setting, by the key its manifest gave it.
///
/// `None` when the person left it unset, which is not the same as a setting
/// whose declared default is falsy: the manifest's default is what an unset
/// setting reads as once Yapfast fills it in, so a plugin can treat `None` as
/// "off".
pub fn get<T: DeserializeOwned>(key: &str) -> Result<Option<T>> {
    let value = call::ok(
        method::SETTINGS_GET,
        wire::SettingGet {
            key: key.to_owned(),
        },
    )?;
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(call::parse(method::SETTINGS_GET, value)?))
}

/// One setting as JSON, for a key whose type the plugin only knows at runtime.
pub fn value(key: &str) -> Result<Option<serde_json::Value>> {
    get::<serde_json::Value>(key)
}

/// Writes one of the plugin's own settings, as the person would on its page:
/// a select by its option's value. The page shows it, and `settings.on_change`
/// says so.
pub fn set(key: &str, value: impl serde::Serialize) -> Result<()> {
    call::ok(
        method::SETTINGS_SET,
        serde_json::json!({ "key": key, "value": value }),
    )?;
    Ok(())
}
