//! A plugin's settings: the values the person set in Yapfast's plugin page.
//!
//! A setting's key and type are declared in the plugin's manifest, and Yapfast
//! draws the form, checks it and stores it, so a plugin reads a setting the
//! same way whatever a person typed into it. There is no write side: settings
//! belong to the person, and a plugin that wants to keep something writes to
//! [`storage`](crate::storage).

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
