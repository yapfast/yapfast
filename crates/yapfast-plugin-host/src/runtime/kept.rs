//! A plugin's settings and storage, kept in this process so a read is
//! answered at once.
//!
//! A read used to be a round trip to the parent, answered `pending` when the
//! parent took longer than the sync wait to answer: under load a plugin read
//! a setting inside a hook and got its default. The parent now sends the
//! whole of both with [`ToHelper::Kept`](yapfast_plugin_api::message::ToHelper)
//! before it hands over a module and again whenever the person changes a
//! setting; the plugin's own writes still go to the parent, which owns the
//! file and the quota, and are mirrored here when the parent took them.

use std::collections::{BTreeMap, HashMap};
use std::sync::{LazyLock, Mutex};

use serde_json::Value;
use yapfast_plugin_api::PluginId;
use yapfast_plugin_api::call::{HostCall, HostReturn, StorageGet, StorageList, StorageSet, method};

#[derive(Default)]
struct Kept {
    settings: serde_json::Map<String, Value>,
    /// None when the plugin may not keep anything, or keeps more than one
    /// frame carries: its reads then go to the parent as before.
    storage: Option<BTreeMap<String, Value>>,
}

static KEPT: LazyLock<Mutex<HashMap<PluginId, Kept>>> = LazyLock::new(Default::default);

fn kept() -> std::sync::MutexGuard<'static, HashMap<PluginId, Kept>> {
    KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What the parent sent: the plugin's settings, and its storage when this
/// process answers its reads.
pub fn keep(
    plugin: PluginId,
    settings: serde_json::Map<String, Value>,
    storage: Option<BTreeMap<String, Value>>,
) {
    kept().insert(plugin, Kept { settings, storage });
}

/// Forgets a plugin that was unloaded.
pub fn forget(plugin: &PluginId) {
    kept().remove(plugin);
}

/// The answer to a read this process can give, or none to ask the parent.
pub fn read(plugin: &PluginId, call: &HostCall) -> Option<HostReturn> {
    let kept = kept();
    let kept = kept.get(plugin)?;
    match call.method.as_str() {
        method::SETTINGS_GET => {
            let key = call.parse::<StorageGet>().ok()?.key;
            Some(match kept.settings.get(&key) {
                Some(value) => HostReturn::Ok(value.clone()),
                None => HostReturn::Error {
                    message: format!("{plugin} declares no setting named `{key}`"),
                },
            })
        }
        method::STORAGE_GET => {
            let storage = kept.storage.as_ref()?;
            let key = call.parse::<StorageGet>().ok()?.key;
            let Some(value) = storage.get(&key) else {
                return Some(HostReturn::Ok(Value::Null));
            };
            // A value larger than one answer goes to the parent, which says so.
            if value.to_string().len() > yapfast_plugin_api::frame::MAX_FRAME - 4096 {
                return None;
            }
            Some(HostReturn::Ok(value.clone()))
        }
        method::STORAGE_LIST => {
            let storage = kept.storage.as_ref()?;
            let prefix = call.parse::<StorageList>().ok()?.prefix;
            let keys: Vec<Value> = storage
                .keys()
                .filter(|key| key.starts_with(&prefix))
                .map(|key| Value::String(key.clone()))
                .collect();
            Some(HostReturn::Ok(Value::Array(keys)))
        }
        _ => None,
    }
}

/// A write a plugin made, mirrored here once the parent took it, or will:
/// `pending` means it is on its way, and a refusal changes nothing.
pub fn wrote(plugin: &PluginId, call: &HostCall, answer: &HostReturn) {
    if !matches!(answer, HostReturn::Ok(_) | HostReturn::Pending { .. }) {
        return;
    }
    let mut kept = kept();
    let Some(storage) = kept.get_mut(plugin).and_then(|kept| kept.storage.as_mut()) else {
        return;
    };
    match call.method.as_str() {
        method::STORAGE_SET => {
            if let Ok(set) = call.parse::<StorageSet>() {
                storage.insert(set.key, set.value);
            }
        }
        method::STORAGE_DELETE => {
            if let Ok(gone) = call.parse::<StorageGet>() {
                storage.remove(&gone.key);
            }
        }
        method::STORAGE_CLEAR => storage.clear(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(method: &str, payload: Value) -> HostCall {
        HostCall {
            method: method.to_owned(),
            payload,
        }
    }

    #[test]
    fn reads_are_answered_here_and_writes_mirrored() {
        let plugin = PluginId::parse("com.example.kept").unwrap();
        let get = call(method::SETTINGS_GET, serde_json::json!({ "key": "volume" }));
        assert_eq!(read(&plugin, &get), None, "nothing kept: ask the parent");
        let mut settings = serde_json::Map::new();
        settings.insert("volume".into(), Value::from(3));
        keep(plugin.clone(), settings, Some(BTreeMap::new()));
        assert_eq!(read(&plugin, &get), Some(HostReturn::Ok(Value::from(3))));
        let unknown = call(method::SETTINGS_GET, serde_json::json!({ "key": "nope" }));
        assert!(matches!(
            read(&plugin, &unknown),
            Some(HostReturn::Error { .. })
        ));
        let stored = call(method::STORAGE_GET, serde_json::json!({ "key": "a" }));
        assert_eq!(
            read(&plugin, &stored),
            Some(HostReturn::Ok(Value::Null)),
            "an absent key is answered absent"
        );
        let set = call(
            method::STORAGE_SET,
            serde_json::json!({ "key": "a", "value": 1 }),
        );
        wrote(
            &plugin,
            &set,
            &HostReturn::Denied {
                reason: "full".into(),
            },
        );
        assert_eq!(
            read(&plugin, &stored),
            Some(HostReturn::Ok(Value::Null)),
            "a refused write changes nothing"
        );
        wrote(&plugin, &set, &HostReturn::Pending { tag: String::new() });
        assert_eq!(read(&plugin, &stored), Some(HostReturn::Ok(Value::from(1))));
        let list = call(method::STORAGE_LIST, serde_json::json!({}));
        assert_eq!(
            read(&plugin, &list),
            Some(HostReturn::Ok(serde_json::json!(["a"])))
        );
        forget(&plugin);
        assert_eq!(read(&plugin, &get), None);
    }
}
