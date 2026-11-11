//! The plugin directory Yapfast owns: what is installed, and what each plugin
//! keeps between runs.
//!
//! Everything lives under `<data>/plugins`:
//!
//! ```text
//! lock.json            what is installed, and which plugins are on
//! grants.json          what the person allowed; read by `permissions`
//! store/<sha256>/      manifest.json and plugin.wasm, content-addressed
//! data/<id>/           a plugin's storage and settings
//! cache/               the helper's compile cache; safe to delete
//! ```
//!
//! Nothing here runs a plugin. A module is hashed against the digest the lock
//! file records before its path is handed to anything, so a `store/` directory
//! that changed underneath Yapfast is refused with a sentence instead of
//! executed.
//!
//! Writes are the other half of what this file is for. A plugin's storage is
//! held in memory and written once the plugin has been quiet for a moment: a
//! plugin that saves on every message would otherwise rewrite a file per
//! message, and the last write before a crash is worth less than the disk
//! traffic of all of them.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use yapfast_plugin_api::{PluginId, PluginManifest};

/// The file naming what is installed.
const LOCK: &str = "lock.json";
/// Where content-addressed plugin files sit, inside the plugin directory.
const STORE: &str = "store";
/// Where a plugin's storage and settings sit, inside the plugin directory.
const DATA: &str = "data";
/// One plugin's key-value store.
const STORAGE: &str = "storage.json";
/// One plugin's settings, as the person set them.
const SETTINGS: &str = "settings.json";

/// How long a plugin's storage waits after its last change before it is written.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

/// Bytes read from a module at a time while hashing it: enough to keep the hash
/// fast, small enough that an 8 MiB module is never in memory whole.
const HASH_CHUNK: usize = 64 << 10;

/// The plugin directory, and the storage it has read so far.
pub struct Store {
    root: PathBuf,
    /// Storage loaded for plugins that have used it, with its byte count and
    /// whether it is waiting to be written.
    storage: BTreeMap<PluginId, Stored>,
    /// When the oldest unwritten change was made, for the debounce.
    dirty_since: Option<Instant>,
}

impl Store {
    /// Points at `<data>/plugins`, creating nothing: a Yapfast with no plugins
    /// must not leave an empty directory behind for the looks of it.
    pub fn open(data_dir: &Path) -> Result<Store> {
        Ok(Store {
            root: data_dir.join("plugins"),
            storage: BTreeMap::new(),
            dirty_since: None,
        })
    }

    /// The plugin directory this store reads.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// What the lock file says is installed. A missing file is nothing
    /// installed, which is what a Yapfast that never met a plugin has.
    pub fn lock(&self) -> Result<Lock> {
        let path = self.root.join(LOCK);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Lock::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("could not read {}", path.display()));
            }
        };
        serde_json::from_str(&text).with_context(|| format!("{} isn't a lock file", path.display()))
    }

    /// Records that a plugin is on or off, leaving the rest of the lock file's
    /// text alone.
    ///
    /// The lock file is written by whatever installs plugins, so it is edited as
    /// JSON rather than parsed into this module's own types and printed back:
    /// fields another part of the plugin system adds must survive a toggle.
    pub fn set_enabled(&self, plugin: &PluginId, enabled: bool) -> Result<()> {
        let path = self.root.join(LOCK);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        let mut lock: Value = serde_json::from_str(&text)
            .with_context(|| format!("{} isn't a lock file", path.display()))?;
        let entry = lock
            .get_mut("plugins")
            .and_then(Value::as_object_mut)
            .and_then(|plugins| plugins.get_mut(plugin.as_str()))
            .and_then(Value::as_object_mut)
            .with_context(|| format!("{} isn't in the lock file", plugin))?;
        entry.insert("enabled".to_owned(), Value::Bool(enabled));
        let text = serde_json::to_vec_pretty(&lock).context("could not write the lock file")?;
        write_atomically(&path, &text)
    }

    /// The manifest a lock entry names, read from the content-addressed store.
    pub fn manifest(&self, entry: &LockEntry) -> Result<PluginManifest> {
        let directory = self
            .store_dir(entry)
            .context("its files are missing from the plugin store")?;
        let path = directory.join("manifest.json");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        PluginManifest::parse(&text).map_err(|error| anyhow::anyhow!("{error}"))
    }

    /// Where a lock entry's files are.
    ///
    /// The store is content-addressed by a digest the lock file records; the
    /// module's is the one that has to be checked, so it is tried first, and the
    /// manifest's digest is accepted for a layout that names the directory after
    /// it.
    fn store_dir(&self, entry: &LockEntry) -> Option<PathBuf> {
        // The installer names the directory, so this reads it rather than
        // guessing: the address is over the module and manifest together, and
        // one digest alone doesn't give it. The digests stay as a fallback for
        // lock files written before the address was recorded.
        let candidates = [&entry.store, &entry.wasm_sha256, &entry.manifest_sha256];
        candidates
            .into_iter()
            .filter(|sha| !sha.is_empty())
            .map(|sha| self.root.join(STORE).join(sha.to_ascii_lowercase()))
            .find(|directory| directory.join("manifest.json").is_file())
    }

    /// The verified module a lock entry and its manifest describe.
    ///
    /// The digest is checked here rather than when the file was installed,
    /// because the file on disk is what runs: an update that wrote half a
    /// module, or anything else that changed it, is caught before the helper
    /// ever opens it.
    pub fn module(&self, entry: &LockEntry, manifest: &PluginManifest) -> Result<Module, String> {
        let Some(wasm) = manifest.wasm.as_deref() else {
            return Err("it has no module to run".to_owned());
        };
        if entry.wasm_sha256.is_empty() {
            // Without a recorded digest there is nothing to check the file
            // against, and an unchecked module is one this build doesn't run.
            return Err("the lock file records no digest for its module".to_owned());
        }
        let directory = self
            .store_dir(entry)
            .ok_or_else(|| "its files are missing from the plugin store".to_owned())?;
        let relative = manifest
            .resolve(wasm)
            .ok_or_else(|| "its module is named outside its own directory".to_owned())?;
        let expected = entry.wasm_sha256.to_ascii_lowercase();
        let path = directory.join(relative);
        let mut module = Module {
            path,
            sha: expected.clone(),
            memory_pages: manifest.limits.memory_pages,
            wasi: manifest.wasi,
            stamp: Stamp::default(),
        };
        module.stamp = Stamp::of(&module.path)?;
        let digest = digest(&module.path)?;
        if digest != expected {
            return Err(format!(
                "its module isn't the one that was installed: {expected} was expected, {digest} is there"
            ));
        }
        Ok(module)
    }

    /// Checks that a module is still the file it was when it was verified,
    /// hashing it again only if it changed.
    pub fn recheck(&self, module: &Module) -> Result<(), String> {
        let stamp = Stamp::of(&module.path)?;
        if stamp == module.stamp {
            return Ok(());
        }
        let digest = digest(&module.path)?;
        if digest != module.sha {
            return Err(format!(
                "its module changed on disk: {} was verified, {digest} is there",
                module.sha
            ));
        }
        Ok(())
    }

    /// One key of a plugin's storage.
    pub fn storage_get(&mut self, plugin: &PluginId, key: &str) -> Option<Value> {
        self.stored(plugin).values.get(key).cloned()
    }

    /// Stores one key, refusing a write that would take the plugin past its
    /// manifest's storage limit.
    pub fn storage_set(
        &mut self,
        plugin: &PluginId,
        key: &str,
        value: Value,
        cap: u64,
    ) -> Result<(), String> {
        let stored = self.stored(plugin);
        let added = json_len(&value);
        let kept = stored.values.get(key).map(json_len).unwrap_or(0);
        let after = stored.bytes.saturating_sub(kept).saturating_add(added);
        if after > cap {
            return Err(format!(
                "this plugin's storage holds {cap} bytes, and that would make it {after}"
            ));
        }
        stored.values.insert(key.to_owned(), value);
        stored.bytes = after;
        stored.dirty = true;
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
        Ok(())
    }

    /// Removes one key, which is not an error when it was never there.
    pub fn storage_delete(&mut self, plugin: &PluginId, key: &str) -> Result<(), String> {
        let stored = self.stored(plugin);
        if let Some(value) = stored.values.remove(key) {
            stored.bytes = stored.bytes.saturating_sub(json_len(&value));
            stored.dirty = true;
            if self.dirty_since.is_none() {
                self.dirty_since = Some(Instant::now());
            }
        }
        Ok(())
    }

    /// The keys a plugin holds, optionally under a prefix.
    pub fn storage_list(&mut self, plugin: &PluginId, prefix: &str) -> Vec<String> {
        self.stored(plugin)
            .values
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// A plugin's settings: what the person set, over the manifest's defaults.
    ///
    /// The file is read on each call rather than cached, so a setting changed in
    /// the plugin's page is the one the plugin sees on its next question, and a
    /// plugin that never asks costs nothing.
    pub fn settings(&self, plugin: &PluginId, manifest: &PluginManifest) -> Value {
        let mut settings = serde_json::Map::new();
        for setting in &manifest.settings {
            settings.insert(setting.key.clone(), setting.default.clone());
        }
        let path = self.data_dir(plugin).join(SETTINGS);
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(Value::Object(set)) => {
                    for (key, value) in set {
                        // Only settings the manifest declares: the file belongs
                        // to the page, and a plugin asking for a key it never
                        // declared gets nothing.
                        if settings.contains_key(&key) {
                            settings.insert(key, value);
                        }
                    }
                }
                Ok(_) => tracing::warn!(%plugin, "the settings file isn't an object"),
                Err(error) => tracing::warn!(%plugin, %error, "the settings file isn't JSON"),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(%plugin, %error, "could not read the settings file"),
        }
        Value::Object(settings)
    }

    /// Writes the storage of every plugin that changed, if the newest change is
    /// old enough to be worth a flush.
    pub fn flush_due(&mut self, now: Instant) {
        let due = match self.pending_save() {
            Some(at) => now >= at,
            None => false,
        };
        if due {
            self.flush();
        }
    }

    /// When unwritten storage should be written, if any is waiting.
    ///
    /// The bridge needs this to set a deadline: a save with nothing else due
    /// would otherwise wait for the next event, and the last change before an
    /// exit is the one worth keeping.
    pub fn pending_save(&self) -> Option<Instant> {
        self.dirty_since.map(|since| since + SAVE_DEBOUNCE)
    }

    /// Writes every plugin's storage that changed, now.
    pub fn flush(&mut self) {
        self.dirty_since = None;
        let root = self.root.clone();
        for (plugin, stored) in &mut self.storage {
            if !stored.dirty {
                continue;
            }
            let path = root.join(DATA).join(plugin.as_str()).join(STORAGE);
            let text = match serde_json::to_vec(&stored.values) {
                Ok(text) => text,
                Err(error) => {
                    tracing::warn!(%plugin, %error, "could not serialize a plugin's storage");
                    continue;
                }
            };
            match write_atomically(&path, &text) {
                // Marked clean only once the bytes are on disk, so a failed
                // write is tried again on the next flush rather than lost.
                Ok(()) => stored.dirty = false,
                Err(error) => tracing::warn!(%plugin, %error, "could not save a plugin's storage"),
            }
        }
    }

    /// Where a plugin's storage and settings sit.
    pub fn data_dir(&self, plugin: &PluginId) -> PathBuf {
        self.root.join(DATA).join(plugin.as_str())
    }

    /// A plugin's storage, read from disk the first time it is asked for.
    fn stored(&mut self, plugin: &PluginId) -> &mut Stored {
        let root = self.root.clone();
        self.storage.entry(plugin.clone()).or_insert_with(|| {
            let path = root.join(DATA).join(plugin.as_str()).join(STORAGE);
            let mut stored = Stored::default();
            match fs::read_to_string(&path) {
                Ok(text) => match serde_json::from_str::<BTreeMap<String, Value>>(&text) {
                    Ok(values) => stored = Stored::new(values),
                    Err(error) => {
                        // A file that isn't a map is left on disk untouched and
                        // treated as empty: rewriting it would destroy whatever
                        // it was.
                        tracing::warn!(%plugin, %error, path = %path.display(), "a plugin's storage isn't JSON");
                    }
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(%plugin, %error, path = %path.display(), "could not read a plugin's storage");
                }
            }
            stored
        })
    }
}

/// One plugin's storage, in memory until it is written.
#[derive(Default)]
struct Stored {
    values: BTreeMap<String, Value>,
    /// The bytes its values take as JSON, kept as they change so a write never
    /// has to measure the whole store.
    bytes: u64,
    dirty: bool,
}

impl Stored {
    fn new(values: BTreeMap<String, Value>) -> Self {
        let bytes = values.values().map(json_len).sum();
        Stored {
            values,
            bytes,
            dirty: false,
        }
    }
}

/// A module in the store, verified against the digest the lock file records.
#[derive(Clone, Debug)]
pub struct Module {
    pub path: PathBuf,
    /// The SHA-256 of the file that was verified, in lowercase hex.
    pub sha: String,
    /// Pages of guest memory, 64 KiB each.
    pub memory_pages: u32,
    pub wasi: bool,
    /// The file as it was when it was hashed, so a check before each load is a
    /// stat rather than another pass over 8 MiB.
    stamp: Stamp,
}

#[cfg(test)]
impl Module {
    /// A module that was never on disk, for tests about everything else.
    pub(crate) fn stub() -> Module {
        Module {
            path: PathBuf::new(),
            sha: String::new(),
            memory_pages: 0,
            wasi: false,
            stamp: Stamp::default(),
        }
    }
}

/// A file's size and modification time, which is what changes when it is
/// replaced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl Stamp {
    fn of(path: &Path) -> Result<Stamp, String> {
        let metadata = fs::metadata(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        Ok(Stamp {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
}

/// The SHA-256 of a file, in lowercase hex, read in chunks.
fn digest(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0u8; HASH_CHUNK];
    loop {
        let read = file
            .read(&mut chunk)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Lowercase hex, which is how the lock file writes a digest.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(byte & 0xf), 16).unwrap_or('0'));
    }
    out
}

/// A value's size as JSON.
fn json_len(value: &Value) -> u64 {
    // A value that can't be serialized can't be written either, so measuring it
    // as nothing only makes the cap optimistic by one key's worth.
    serde_json::to_vec(value).map_or(0, |bytes| bytes.len() as u64)
}

/// Replaces a file in one step: a reader either sees the old bytes or the new
/// ones, never half of both.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)
        .with_context(|| format!("could not write {}", temporary.display()))?;
    fs::rename(&temporary, path).with_context(|| format!("could not replace {}", path.display()))
}

/// What is installed, from `lock.json`.
#[derive(Debug, Default, Deserialize)]
pub struct Lock {
    #[serde(default)]
    pub plugins: BTreeMap<PluginId, LockEntry>,
}

/// One installed plugin: where it came from, what was installed, and whether
/// the person turned it on.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct LockEntry {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, rename = "ref")]
    pub reference: String,
    /// The commit that was installed, which is the pin.
    #[serde(default)]
    pub rev: String,
    /// The store directory the installer wrote, named by the lock file rather
    /// than derived here: the address covers the module and the manifest
    /// together, so a side that has only one digest can't compute it.
    #[serde(default)]
    pub store: String,
    /// The SHA-256 of the module that runs.
    #[serde(default)]
    pub wasm_sha256: String,
    /// The SHA-256 of the manifest the module was reviewed with.
    #[serde(default)]
    pub manifest_sha256: String,
    /// Whether the person turned it on.
    #[serde(default)]
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let store = Store::open(directory.path()).expect("a store");
        (directory, store)
    }

    fn id(text: &str) -> PluginId {
        PluginId::parse(text).expect("a plugin id")
    }

    #[test]
    fn a_hex_digest_is_lowercase_and_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
    }

    #[test]
    fn storage_survives_a_flush_and_a_new_store() {
        let (directory, mut store) = store();
        let plugin = id("com.example.store");
        store
            .storage_set(&plugin, "seen", serde_json::json!(3), 4096)
            .expect("it fits");
        store.flush();
        let storage = store.data_dir(&plugin).join(STORAGE);
        assert!(storage.is_file());

        let mut reopened = Store::open(directory.path()).expect("the store again");
        assert_eq!(
            reopened.storage_get(&plugin, "seen"),
            Some(serde_json::json!(3))
        );
        assert_eq!(
            reopened.storage_list(&plugin, "se"),
            vec!["seen".to_owned()]
        );
    }

    #[test]
    fn a_write_past_the_cap_is_refused_and_changes_nothing() {
        let (_directory, mut store) = store();
        let plugin = id("com.example.cap");
        store
            .storage_set(&plugin, "k", serde_json::json!("x".repeat(64)), 128)
            .expect("it fits");
        let error = store
            .storage_set(&plugin, "k2", serde_json::json!("y".repeat(128)), 128)
            .expect_err("it can't fit");
        assert!(error.contains("128 bytes"), "{error}");
        assert_eq!(store.storage_get(&plugin, "k2"), None);
    }

    #[test]
    fn deleting_a_key_frees_its_bytes() {
        let (_directory, mut store) = store();
        let plugin = id("com.example.delete");
        store
            .storage_set(&plugin, "a", serde_json::json!("x".repeat(60)), 128)
            .expect("it fits");
        store.storage_delete(&plugin, "a").expect("it deletes");
        store
            .storage_set(&plugin, "b", serde_json::json!("y".repeat(100)), 128)
            .expect("the room is free again");
    }

    #[test]
    fn a_toggle_leaves_the_rest_of_the_lock_file_alone() {
        let (directory, store) = store();
        let root = directory.path().join("plugins");
        fs::create_dir_all(&root).expect("the plugin directory");
        fs::write(
            root.join(LOCK),
            r#"{"version":1,"plugins":{"com.example.lock":{"rev":"abc","origin":"url","enabled":false}}}"#,
        )
        .expect("a lock file");
        store
            .set_enabled(&id("com.example.lock"), true)
            .expect("it toggles");
        let text = fs::read_to_string(root.join(LOCK)).expect("the lock file");
        assert!(text.contains("\"enabled\": true"), "{text}");
        assert!(text.contains("\"origin\": \"url\""), "{text}");
    }

    #[test]
    fn a_module_is_refused_when_its_digest_does_not_match() {
        let (directory, store) = store();
        let directory = directory.path().join("plugins").join(STORE).join("aa");
        fs::create_dir_all(&directory).expect("a store directory");
        fs::write(directory.join("manifest.json"), "{}").expect("a manifest");
        fs::write(directory.join("plugin.wasm"), b"not the module").expect("a module");
        let entry = LockEntry {
            wasm_sha256: "aa".to_owned(),
            ..LockEntry::default()
        };
        let manifest = PluginManifest::parse(
            r#"{"api":1,"id":"com.example.module","name":"M","version":"1.0.0","wasm":"plugin.wasm"}"#,
        )
        .expect("a manifest");
        let error = store.module(&entry, &manifest).expect_err("a mismatch");
        assert!(
            error.contains("isn't the one that was installed"),
            "{error}"
        );
    }
}
