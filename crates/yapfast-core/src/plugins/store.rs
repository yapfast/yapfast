//! The plugin directory Yapfast owns: what is installed, and what each plugin
//! keeps between runs.
//!
//! Everything lives under `<data>/plugins`:
//!
//! ```text
//! lock.json            what is installed, and which plugins are on
//! grants.json          what the person allowed; read by `permissions`
//! store/<sha256>/      manifest.json and plugin.wasm, content-addressed; the
//!                      current version and the one a roll back returns to
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
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use yapfast_plugin_api::{PluginId, PluginManifest};

/// The file naming what is installed.
const LOCK: &str = "lock.json";
/// Where content-addressed plugin files sit, inside the plugin directory.
const STORE: &str = "store";
/// Where a module and an icon go when the manifest names neither.
const MODULE_FILE: &str = "plugin.wasm";
const ICON_FILE: &str = "icon.png";
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

/// A plugin that has been fetched and checked, waiting to be written into the
/// store. Nothing here has run.
pub struct Package {
    pub id: PluginId,
    /// The repository it came from, as `fetch::Spec::source` names it.
    pub source: String,
    /// The folder in it, empty when the manifest is at the repository's root.
    pub path: String,
    /// The branch or tag it was fetched at; empty for the default branch.
    pub reference: String,
    /// The commit, which is the pin.
    pub rev: String,
    /// The manifest's text, written as `manifest.json`.
    pub manifest: String,
    /// The module, written as `plugin.wasm`.
    pub module: Vec<u8>,
    /// `icon.png`, when the manifest names one.
    pub icon: Option<Vec<u8>>,
}

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
        let lock = self.lock_json()?;
        serde_json::from_value(lock)
            .with_context(|| format!("{} isn't a lock file", path.display()))
    }

    /// The lock file as JSON, which is how it is written: it is edited as JSON
    /// rather than parsed into this module's own types and printed back, so
    /// fields another part of the plugin system adds survive an install as well
    /// as a toggle.
    ///
    /// A version this build can't read is refused here, before anything is
    /// written to it.
    fn lock_json(&self) -> Result<Value> {
        let path = self.root.join(LOCK);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(serde_json::json!({ "version": LOCK_VERSION, "plugins": {} }));
            }
            Err(error) => {
                return Err(error).with_context(|| format!("could not read {}", path.display()));
            }
        };
        let lock: Value = serde_json::from_str(&text)
            .with_context(|| format!("{} isn't a lock file", path.display()))?;
        let version = lock.get("version").and_then(Value::as_u64).unwrap_or(0);
        if version > u64::from(LOCK_VERSION) {
            anyhow::bail!(
                "{} is a version {version} lock file and this build reads version {LOCK_VERSION}; \
                 update Yapfast rather than forgetting what is installed",
                path.display(),
            );
        }
        Ok(lock)
    }

    /// Writes a checked package into `store/` and `lock.json`: the current
    /// version becomes the previous one, which Roll back returns to, and the
    /// one before that is dropped.
    pub fn install(&mut self, package: &Package) -> Result<()> {
        let id = package.id.as_str();
        // Digests are read through a borrow: a module is up to 64 MiB, and this
        // is the one pass over it that isn't the write itself.
        let wasm_sha256 = hex(&Sha256::digest(&package.module));
        let manifest_sha256 = hex(&Sha256::digest(package.manifest.as_bytes()));
        // The directory a version lives in is named after both, because both
        // decide what runs. Naming it after the module alone would let a
        // manifest-only update — the way a plugin asks for a new permission —
        // land in the same directory as the version it replaces, where the old
        // manifest would be read beside the new pin.
        let address = {
            let mut digest = Sha256::new();
            digest.update(wasm_sha256.as_bytes());
            digest.update(b":");
            digest.update(manifest_sha256.as_bytes());
            hex(&digest.finalize())
        };

        let mut lock = self.lock_json()?;
        let installed = lock
            .get("plugins")
            .and_then(Value::as_object)
            .and_then(|plugins| plugins.get(id))
            .cloned();
        if let Some(entry) = &installed {
            // One id, one install: two folders of a monorepo that declare the
            // same id are refused rather than overwriting each other, which is
            // how a copied template escapes review.
            let source = entry.get("source").and_then(Value::as_str).unwrap_or("");
            let path = entry.get("path").and_then(Value::as_str).unwrap_or("");
            if source != package.source || path != package.path {
                anyhow::bail!("another plugin already uses {id}");
            }
        }
        // A person's choice of whether it is on survives an update.
        let enabled = installed
            .as_ref()
            .and_then(|entry| entry.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let mut entry = serde_json::json!({
            "source": package.source,
            "path": package.path,
            "ref": package.reference,
            "rev": package.rev,
            "origin": "url",
            "store": address,
            "wasm_sha256": wasm_sha256,
            "manifest_sha256": manifest_sha256,
            "enabled": enabled,
        });
        if let Some(installed) = installed {
            // The version being replaced keeps its fields, since it is what a
            // roll back returns to, but not its own previous one: only two
            // versions stay in `store/`, and neither is two deep.
            let mut previous = installed;
            if let Some(object) = previous.as_object_mut() {
                object.remove("previous");
            }
            if let Some(object) = entry.as_object_mut() {
                object.insert("previous".to_owned(), previous);
            }
        }

        let before = lock_dirs(&self.root, &lock);
        self.write_version(package, &address)?;
        lock.get_mut("plugins")
            .and_then(Value::as_object_mut)
            .context("the lock file's plugins aren't an object")?
            .insert(id.to_owned(), entry);

        if let Err(error) = self.write_lock(&lock) {
            // No entry names the version that was just written, so nobody could
            // load it, update it or uninstall it: remove it rather than leave
            // the store holding files no lock entry mentions. A directory the
            // lock file already named is another version's and stays.
            let directory = self.root.join(STORE).join(&address);
            if !before.contains(&directory)
                && let Err(removal) = remove_all(&directory)
            {
                tracing::warn!(%removal, "could not remove a version no lock entry names");
            }
            return Err(error);
        }
        self.prune(&before, &lock);
        Ok(())
    }

    /// Goes back to the version the last install replaced.
    pub fn roll_back(&mut self, plugin: &PluginId) -> Result<()> {
        let mut lock = self.lock_json()?;
        let entry = lock
            .get_mut("plugins")
            .and_then(Value::as_object_mut)
            .and_then(|plugins| plugins.get_mut(plugin.as_str()))
            .with_context(|| format!("{plugin} isn't installed"))?;
        let previous = entry
            .get("previous")
            .filter(|value| !value.is_null())
            .cloned()
            .with_context(|| format!("{plugin} has no previous version to roll back to"))?;
        // The version that is current becomes the one a roll back returns to,
        // so rolling back twice undoes the first: the files of both are already
        // in the store, and this is only which of them the lock file names.
        let mut replaced = entry.clone();
        if let Some(object) = replaced.as_object_mut() {
            object.remove("previous");
        }
        let mut rolled = previous;
        let object = rolled.as_object_mut().with_context(|| {
            format!("the version {plugin} keeps to roll back to isn't a lock entry")
        })?;
        // A roll back is about the code it runs, not about whether the person
        // wants it on, so the switch stays where they left it.
        if let Some(enabled) = entry.get("enabled").cloned() {
            object.insert("enabled".to_owned(), enabled);
        }
        object.insert("previous".to_owned(), replaced);
        *entry = rolled;
        self.write_lock(&lock)
    }

    /// Removes a plugin's files and its lock entry; `delete_data` also removes
    /// `data/<id>/`, its storage and settings.
    pub fn uninstall(&mut self, plugin: &PluginId, delete_data: bool) -> Result<()> {
        let mut lock = self.lock_json()?;
        let before = lock_dirs(&self.root, &lock);
        lock.get_mut("plugins")
            .and_then(Value::as_object_mut)
            .context("the lock file's plugins aren't an object")?
            .remove(plugin.as_str())
            .with_context(|| format!("{plugin} isn't installed"))?;
        // The entry goes first: a `store/` directory no entry names is found by
        // the next install, while an entry whose files are gone is a plugin that
        // fails to load on every start from now on.
        self.write_lock(&lock)?;
        self.prune(&before, &lock);
        if delete_data {
            remove_all(&self.data_dir(plugin))?;
            // Storage read into memory goes too, or the next flush would write
            // it back into the directory that was just removed.
            self.storage.remove(plugin);
        }
        Ok(())
    }

    /// Writes the lock file, in one step, so a reader sees the old entries or
    /// the new ones.
    fn write_lock(&self, lock: &Value) -> Result<()> {
        let path = self.root.join(LOCK);
        let text = serde_json::to_vec_pretty(lock).context("could not write the lock file")?;
        write_atomically(&path, &text)
    }

    /// Writes one version's files, addressed by the module's digest, in a
    /// directory that is renamed into place once its bytes are on disk: a
    /// `store/` directory that is there is whole, so nothing ever runs half a
    /// module.
    /// Writes one version at `address`, which names the module and the manifest
    /// together: the same address is the same bytes, so an install that finds it
    /// already there has nothing to do.
    fn write_version(&self, package: &Package, address: &str) -> Result<()> {
        let store = self.root.join(STORE);
        fs::create_dir_all(&store)
            .with_context(|| format!("could not create {}", store.display()))?;
        let directory = store.join(address);
        // At the names its manifest gives them, not names of this side's
        // choosing: `module` reads `manifest.wasm` beside `manifest.json`, so a
        // plugin whose module is `ping.wasm` has to find a `ping.wasm` there.
        let (module_name, icon_name) = names(package);
        if directory.join("manifest.json").is_file()
            && directory.join(&module_name).is_file()
            && (package.icon.is_none() || directory.join(&icon_name).is_file())
        {
            // Content-addressed: this address is these bytes and every file they
            // name is here, so the version is complete. Keep it, and leave
            // whichever entry names it alone. One missing a file was written by
            // a build that named them differently, and is rewritten rather than
            // left to fail the way it did.
            return Ok(());
        }
        // Named after what is being written, so an install that failed partway
        // leaves one directory the next attempt at the same version reuses.
        let temporary = store.join(format!(".{address}.tmp"));
        remove_all(&temporary)?;
        fs::create_dir_all(&temporary)
            .with_context(|| format!("could not create {}", temporary.display()))?;
        write_file(
            &temporary.join("manifest.json"),
            package.manifest.as_bytes(),
        )?;
        if let Some(parent) = temporary.join(&module_name).parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        write_file(&temporary.join(&module_name), &package.module)?;
        if let Some(icon) = &package.icon {
            if let Some(parent) = temporary.join(&icon_name).parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("could not create {}", parent.display()))?;
            }
            write_file(&temporary.join(&icon_name), icon)?;
        }
        // The files are flushed, and then their names, before the directory is
        // renamed: a version that appears in `store/` survives a power cut.
        sync_dir(&temporary)?;
        if directory.is_dir() {
            // There, but not whole: replace it.
            remove_all(&directory)?;
        }
        fs::rename(&temporary, &directory)
            .with_context(|| format!("could not write {}", directory.display()))?;
        sync_dir(&store)
    }

    /// Removes the `store/` directories the lock file no longer names: the
    /// version before the one kept to roll back to, or an uninstalled plugin's.
    fn prune(&self, before: &[PathBuf], lock: &Value) {
        let after = lock_dirs(&self.root, lock);
        for directory in before.iter().filter(|directory| !after.contains(directory)) {
            if let Err(error) = remove_all(directory) {
                // What was asked for is written; a directory left behind is a
                // few MiB of garbage, not a failed install.
                tracing::warn!(%error, "could not remove a version from the plugin store");
            }
        }
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

    /// The icon a plugin's manifest names, read from the store: what the page
    /// draws beside it, and `None` when it declares none or the file is gone.
    pub fn icon(&self, entry: &LockEntry, manifest: &PluginManifest) -> Option<Vec<u8>> {
        let icon = manifest.icon.as_ref()?;
        let directory = self.store_dir(entry)?;
        let name = manifest.resolve(icon)?;
        fs::read(directory.join(name)).ok()
    }

    /// The manifest a lock entry names, read from the content-addressed store.
    pub fn manifest(&self, entry: &LockEntry) -> Result<PluginManifest> {
        let text = self.manifest_text(entry)?;
        PluginManifest::parse(&text).map_err(|error| anyhow::anyhow!("{error}"))
    }

    /// The manifest's own text, which the interface reads with
    /// [`crate::plugins::Manifest`]: it holds the `ui` and `settings` sections
    /// that the runtime's manifest leaves out.
    pub fn manifest_text(&self, entry: &LockEntry) -> Result<String> {
        let directory = self
            .store_dir(entry)
            .context("its files are missing from the plugin store")?;
        let path = directory.join("manifest.json");
        fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))
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

/// Writes a file and flushes it to the disk, so what a later rename makes
/// visible is bytes that are already there.
/// Where a package's module and icon go inside its store directory: the paths
/// its own manifest names, or the ones this side would pick when it names none.
fn names(package: &Package) -> (PathBuf, PathBuf) {
    let manifest = match PluginManifest::parse(&package.manifest) {
        Ok(manifest) => manifest,
        // A manifest that can't be read is refused when the registry reads it;
        // the bytes still go where a plugin with no module named would expect.
        Err(_) => return (PathBuf::from(MODULE_FILE), PathBuf::from(ICON_FILE)),
    };
    let module = manifest
        .wasm
        .as_ref()
        .and_then(|wasm| manifest.resolve(wasm))
        .unwrap_or_else(|| PathBuf::from(MODULE_FILE));
    let icon = manifest
        .icon
        .as_ref()
        .and_then(|icon| manifest.resolve(icon))
        .unwrap_or_else(|| PathBuf::from(ICON_FILE));
    (module, icon)
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file =
        fs::File::create(path).with_context(|| format!("could not write {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("could not write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("could not flush {}", path.display()))
}

/// Flushes a directory's names, so the files it lists survive a power cut.
fn sync_dir(path: &Path) -> Result<()> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("could not flush {}", path.display()))
}

/// Removes a directory and what is in it, taking one that is already gone as
/// removed: what matters is that it isn't there afterwards.
fn remove_all(directory: &Path) -> Result<()> {
    match fs::remove_dir_all(directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("could not remove {}", directory.display()))
        }
    }
}

/// Every `store/` directory a lock file names, for a plugin's current version
/// and for the one it keeps to roll back to.
///
/// What the lock file says rather than what is on disk: it is what an install
/// has to leave, and what an uninstall has to remove.
fn lock_dirs(root: &Path, lock: &Value) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let entries = lock
        .get("plugins")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|plugins| plugins.values());
    for entry in entries {
        for named in [Some(entry), entry.get("previous")].into_iter().flatten() {
            for key in ["store", "wasm_sha256", "manifest_sha256"] {
                let sha = named.get(key).and_then(Value::as_str).unwrap_or("");
                if sha.is_empty() {
                    continue;
                }
                let directory = root.join(STORE).join(sha.to_ascii_lowercase());
                if !dirs.contains(&directory) {
                    dirs.push(directory);
                }
            }
        }
    }
    dirs
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

/// The lock file's shape, as the installer writes it.
///
/// The installer owns the file; this side reads it. Both agree on a version so
/// that a later format is refused with both numbers rather than read as an
/// empty one — a lock file this build can't parse is not a lock file with
/// nothing installed, and treating it that way would forget every plugin.
pub const LOCK_VERSION: u32 = 1;

/// What is installed, from `lock.json`.
#[derive(Debug, Default, Deserialize)]
pub struct Lock {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub plugins: BTreeMap<PluginId, LockEntry>,
}

/// One installed plugin: where it came from, what was installed, and whether
/// the person turned it on.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
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
    /// The version the install before this one wrote: what Roll back returns
    /// to, with the fields it had then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Box<LockEntry>>,
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

    fn package(id_text: &str, version: &str, module: &[u8]) -> Package {
        Package {
            id: id(id_text),
            source: "https://codeberg.org/ada/word-filter".to_owned(),
            path: String::new(),
            reference: "main".to_owned(),
            rev: version.to_owned(),
            manifest: format!(
                r#"{{"api":1,"id":"{id_text}","name":"M","version":"{version}","wasm":"plugin.wasm"}}"#
            ),
            module: module.to_vec(),
            icon: None,
        }
    }

    /// The version directories in the store, sorted, so a test can count them.
    fn versions(store: &Store) -> Vec<String> {
        let Ok(entries) = fs::read_dir(store.root().join(STORE)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// The lock entry for one plugin, read from the file as `page` would.
    fn entry(store: &Store, plugin: &PluginId) -> LockEntry {
        store
            .lock()
            .expect("the lock file")
            .plugins
            .remove(plugin)
            .expect("its entry")
    }

    /// A module is stored at the name its manifest uses, because that is the
    /// name the registry reads it back by: a plugin installed from `Ping.wasm`
    /// declares `ping.wasm`, and writing `plugin.wasm` left it installed and
    /// unable to run.
    #[test]
    fn a_module_keeps_the_name_its_manifest_gives_it() {
        let (_directory, mut store) = store();
        let mut package = package("local.ping", "0.1.0", b"module one");
        package.manifest = package.manifest.replace("\"plugin.wasm\"", "\"ping.wasm\"");
        store.install(&package).expect("it installs");

        let entry = entry(&store, &package.id);
        let directory = store.root().join(STORE).join(&entry.store);
        assert!(
            directory.join("ping.wasm").is_file(),
            "{:?}",
            versions(&store)
        );
        assert!(!directory.join("plugin.wasm").exists());
        // And what the registry reads is there, which is what enabling needs.
        let module = store.module(&entry, &store.manifest(&entry).expect("its manifest"));
        assert!(module.is_ok(), "{module:?}");
    }

    /// A version a build wrote with the wrong file names is rewritten rather
    /// than kept: installing the same `.wasm` again is how a person fixes the
    /// plugin that kept crashing, and the bytes are the same either way.
    #[test]
    fn installing_again_repairs_a_version_missing_its_named_module() {
        let (_directory, mut store) = store();
        let mut package = package("local.ping", "0.1.0", b"module one");
        package.manifest = package.manifest.replace("\"plugin.wasm\"", "\"ping.wasm\"");
        store.install(&package).expect("it installs");
        let entry = entry(&store, &package.id);
        let directory = store.root().join(STORE).join(&entry.store);
        // What the build that named every module `plugin.wasm` left behind.
        fs::remove_file(directory.join("ping.wasm")).expect("its module");
        fs::write(directory.join("plugin.wasm"), b"module one").expect("the old name");

        store.install(&package).expect("it installs again");
        assert!(directory.join("ping.wasm").is_file(), "named again");
        assert!(
            store
                .module(&entry, &store.manifest(&entry).expect("its manifest"))
                .is_ok(),
            "and the registry can read it"
        );
    }

    #[test]
    fn an_install_writes_its_files_under_one_content_addressed_name() {
        let (_directory, mut store) = store();
        let mut package = package("com.example.install", "1.0.0", b"module one");
        package.icon = Some(b"png".to_vec());
        store.install(&package).expect("it installs");

        // The name covers the module and the manifest together: a manifest that
        // changed with the module unchanged must not share a directory.
        let address = entry(&store, &package.id).store;
        let wasm = hex(&Sha256::digest(&package.module));
        assert_ne!(address, wasm, "the manifest is part of the name");
        let directory = store.root().join(STORE).join(&address);
        assert_eq!(
            fs::read(directory.join("plugin.wasm")).expect("the module"),
            package.module
        );
        assert_eq!(
            fs::read_to_string(directory.join("manifest.json")).expect("the manifest"),
            package.manifest
        );
        assert_eq!(
            fs::read(directory.join("icon.png")).expect("the icon"),
            b"png"
        );
        // One directory, whole: no temporary one is left behind.
        assert_eq!(versions(&store), vec![address.clone()]);

        let installed = entry(&store, &package.id);
        assert_eq!(installed.source, package.source);
        assert_eq!(installed.path, "");
        assert_eq!(installed.reference, "main");
        assert_eq!(installed.rev, "1.0.0");
        assert_eq!(installed.store, address);
        assert_eq!(installed.wasm_sha256, wasm);
        assert_eq!(
            installed.manifest_sha256,
            hex(&Sha256::digest(package.manifest.as_bytes()))
        );
        assert!(!installed.enabled, "a new plugin starts off");
        assert!(installed.previous.is_none());

        let text = fs::read_to_string(store.root().join(LOCK)).expect("the lock file");
        assert!(text.contains("\"origin\": \"url\""), "{text}");
    }

    #[test]
    fn an_update_keeps_one_version_to_roll_back_to() {
        let (_directory, mut store) = store();
        let plugin = id("com.example.roll");
        let first = package("com.example.roll", "1.0.0", b"module one");
        store.install(&first).expect("it installs");
        store.set_enabled(&plugin, true).expect("it is on");
        store
            .install(&package("com.example.roll", "2.0.0", b"module two"))
            .expect("it updates");

        let updated = entry(&store, &plugin);
        assert!(
            updated.enabled,
            "the person's choice of on survives an update"
        );
        assert_eq!(
            updated.previous.as_ref().expect("the version before").rev,
            "1.0.0"
        );

        store
            .install(&package("com.example.roll", "3.0.0", b"module three"))
            .expect("it updates again");
        let entry_three = entry(&store, &plugin);
        assert_eq!(entry_three.rev, "3.0.0");
        assert_eq!(
            entry_three
                .previous
                .as_ref()
                .expect("the version before")
                .rev,
            "2.0.0"
        );
        // Current and previous, and nothing older.
        assert_eq!(versions(&store).len(), 2, "{:?}", versions(&store));

        store.roll_back(&plugin).expect("it rolls back");
        let rolled = entry(&store, &plugin);
        assert_eq!(rolled.rev, "2.0.0");
        assert_eq!(
            rolled
                .previous
                .as_ref()
                .expect("the version it replaced")
                .rev,
            "3.0.0"
        );
        // The entry names files that are still there, and the module is checked
        // against its digest as it is on every load.
        let manifest = store.manifest(&rolled).expect("its manifest");
        let module = store.module(&rolled, &manifest).expect("its module");
        assert_eq!(fs::read(&module.path).expect("the module"), b"module two");

        // A roll back is undone by rolling back again.
        store.roll_back(&plugin).expect("it rolls forward again");
        assert_eq!(entry(&store, &plugin).rev, "3.0.0");
        assert_eq!(versions(&store).len(), 2, "{:?}", versions(&store));
    }

    #[test]
    fn a_roll_back_with_nothing_before_it_is_refused() {
        let (_directory, mut store) = store();
        let package = package("com.example.only", "1.0.0", b"module one");
        store.install(&package).expect("it installs");
        let error = store
            .roll_back(&package.id)
            .expect_err("there is nothing before it");
        assert!(error.to_string().contains("no previous version"), "{error}");
    }

    #[test]
    fn one_id_from_two_folders_is_refused() {
        let (_directory, mut store) = store();
        let first = package("com.example.monorepo", "1.0.0", b"module one");
        store.install(&first).expect("it installs");

        let mut copied = package("com.example.monorepo", "1.0.0", b"module one");
        copied.path = "plugins/copied".to_owned();
        let error = store.install(&copied).expect_err("two folders, one id");
        assert_eq!(
            error.to_string(),
            "another plugin already uses com.example.monorepo"
        );

        // The same folder under the same source is an update, not a second
        // plugin.
        store.install(&first).expect("it updates");
    }

    #[test]
    fn uninstalling_removes_its_files_and_its_data_only_when_asked() {
        let (_directory, mut store) = store();
        let plugin = id("com.example.remove");
        store
            .install(&package("com.example.remove", "1.0.0", b"module one"))
            .expect("it installs");
        store
            .install(&package("com.example.remove", "2.0.0", b"module two"))
            .expect("it updates");
        store
            .storage_set(&plugin, "seen", serde_json::json!(1), 4096)
            .expect("it fits");
        store.flush();
        let data = store.data_dir(&plugin);
        assert!(data.join(STORAGE).is_file());

        store.uninstall(&plugin, false).expect("it uninstalls");
        assert!(versions(&store).is_empty(), "{:?}", versions(&store));
        assert!(store.lock().expect("the lock file").plugins.is_empty());
        assert!(
            data.join(STORAGE).is_file(),
            "storage and settings stay unless they are asked for"
        );

        store
            .install(&package("com.example.remove", "1.0.0", b"module one"))
            .expect("it installs again");
        let error = store
            .uninstall(&id("com.example.never"), true)
            .expect_err("it was never installed");
        assert!(error.to_string().contains("isn't installed"), "{error}");

        store.uninstall(&plugin, true).expect("it uninstalls");
        assert!(!data.is_dir(), "its storage and settings go with it");
        assert!(versions(&store).is_empty(), "{:?}", versions(&store));
    }

    #[test]
    fn a_failed_lock_write_leaves_no_version_behind() {
        let (directory, mut store) = store();
        let root = directory.path().join("plugins");
        // A directory where `write_atomically` writes its temporary file: the
        // version's files are written, then the lock file can't be.
        fs::create_dir_all(root.join("lock.tmp")).expect("an obstruction");
        let package = package("com.example.fail", "1.0.0", b"module one");
        store
            .install(&package)
            .expect_err("the lock can't be written");
        assert!(versions(&store).is_empty(), "{:?}", versions(&store));
        assert!(!root.join(LOCK).exists());
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
