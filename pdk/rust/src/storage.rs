//! The plugin's own key-value store.
//!
//! Storage is the only thing that outlives a plugin's instance: an instance may
//! be evicted between any two calls, and WebAssembly memory can't shrink, so
//! what a plugin remembers between calls lives here and not in its fields.
//!
//! Keys and values are the plugin's own: there is no schema, and the helper
//! only counts bytes against the quota the manifest asked for. Writes are
//! batched and debounced by the helper, so a key read back in the same call
//! reads what it last wrote.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Result;
use crate::call::{self, method};
use crate::json;
use crate::wire;

/// One value, by key.
///
/// `None` when the key isn't there. A stored JSON `null` reads as `None` too,
/// because the wire says an absent key is `null`; store an object or an array
/// if you need to tell the two apart.
pub fn get<T: DeserializeOwned>(key: &str) -> Result<Option<T>> {
    let value = call::ok(
        method::STORAGE_GET,
        wire::StorageGet {
            key: key.to_owned(),
        },
    )?;
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(call::parse(method::STORAGE_GET, value)?))
}

/// Stores one value under a key, replacing what was there.
///
/// The value is JSON, so it can be read back by a later instance of the plugin,
/// after an upgrade, or in the app's plugin page.
pub fn set<T: Serialize>(key: &str, value: &T) -> Result<()> {
    let value = json::to_value(value)?;
    call::ok(
        method::STORAGE_SET,
        wire::StorageSet {
            key: key.to_owned(),
            value,
        },
    )?;
    Ok(())
}

/// Forgets one key. Deleting a key that isn't there is not an error.
pub fn delete(key: &str) -> Result<()> {
    call::ok(
        method::STORAGE_DELETE,
        wire::StorageGet {
            key: key.to_owned(),
        },
    )?;
    Ok(())
}

/// Every key under a prefix, in no particular order.
///
/// An empty prefix lists everything the plugin holds.
pub fn list(prefix: &str) -> Result<Vec<String>> {
    let value = call::ok(
        method::STORAGE_LIST,
        wire::StorageList {
            prefix: prefix.to_owned(),
        },
    )?;
    let keys: Vec<String> = call::parse(method::STORAGE_LIST, value)?;
    Ok(keys)
}
