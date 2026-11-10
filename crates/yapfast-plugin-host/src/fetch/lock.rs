//! `lock.json`: what is installed, from where, and at which commit.
//!
//! The lock is the only thing that says a plugin exists. A version in the store
//! is a bag of content-addressed files; the lock names which of them is the
//! current version of which plugin, where it came from, and the SHA-256 it had
//! when a person reviewed it, so reading it back later can tell that it hasn't
//! changed since.
//!
//! The current and one previous version of each plugin are kept, and older ones
//! are removed once the lock no longer names them: a person can roll back one
//! release without a network, and the disk doesn't fill with every version ever
//! installed. A rewritten lock replaces the old one by rename, so a fetch that
//! dies halfway leaves the previous lock intact.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use yapfast_plugin_api::PluginId;

use super::store::{self, Store};
use super::validate::{self, sha256_file};

/// The version of the lock file's shape.
pub const LOCK_VERSION: u32 = 1;

/// The file's name, beside the store's version directories.
const LOCK_FILE: &str = "lock.json";

/// How many versions before the current one are kept.
const MAX_PREVIOUS: usize = 1;

/// One version of a plugin: where it came from and what it hashed to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pinned {
    /// The URL as the person pasted it.
    pub source: String,
    /// The directory inside the repository, or `null` for its root.
    #[serde(default)]
    pub path: Option<String>,
    /// The branch, tag or commit that was asked for, or `HEAD`.
    #[serde(rename = "ref")]
    pub reference: String,
    /// The commit that was actually read.
    pub rev: String,
    /// The module's SHA-256; `null` for a plugin that is only a manifest.
    #[serde(default)]
    pub wasm_sha256: Option<String>,
    /// The SHA-256 of the manifest's text.
    pub manifest_sha256: String,
    /// The store directory this version lives in.
    pub store: String,
    /// The repository URL without the ref or path, so it can be fetched again.
    pub origin: String,
    /// The version the plugin declares, for the page.
    #[serde(default)]
    pub version: String,
}

/// A plugin's current install, whether it runs, and the version before it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockEntry {
    #[serde(flatten)]
    pub current: Pinned,
    /// Whether the person turned it on. Installing never does.
    #[serde(default)]
    pub enabled: bool,
    /// The version this one replaced, kept so a rollback needs no network.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous: Vec<Pinned>,
}

impl LockEntry {
    /// Whether this entry names the install at `address`, as current or as the
    /// version before.
    pub fn names(&self, address: &str) -> bool {
        self.current.store == address || self.previous.iter().any(|pinned| pinned.store == address)
    }
}

/// What is installed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockFile {
    /// The shape's version, so a newer Yapfast's lock is never read as this
    /// build's.
    pub version: u32,
    /// A plugin by id.
    #[serde(default)]
    pub plugins: BTreeMap<PluginId, LockEntry>,
}

impl Default for LockFile {
    fn default() -> Self {
        LockFile {
            version: LOCK_VERSION,
            plugins: BTreeMap::new(),
        }
    }
}

impl LockFile {
    /// Reads the lock beside the store's versions, or an empty one when there is
    /// none yet.
    pub fn load(store_root: &Path) -> anyhow::Result<LockFile> {
        let path = store_root.join(LOCK_FILE);
        let text = match std::fs::read(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LockFile::default());
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("`{}` couldn't be read", path.display()));
            }
        };
        let lock: LockFile = serde_json::from_slice(&text)
            .with_context(|| format!("`{}` isn't a lock file this build reads", path.display()))?;
        if lock.version > LOCK_VERSION {
            bail!(
                "`{}` was written by a newer Yapfast (its lock file version is {}); update \
                 Yapfast before installing plugins",
                path.display(),
                lock.version
            );
        }
        Ok(lock)
    }

    /// Replaces the lock file in one step, once everything it names is written.
    pub fn save(&self, store_root: &Path) -> anyhow::Result<()> {
        let path = store_root.join(LOCK_FILE);
        let mut text =
            serde_json::to_vec_pretty(self).context("the lock file couldn't be written as JSON")?;
        text.push(b'\n');
        store::write_atomic(&path, &text)
            .with_context(|| format!("`{}` couldn't be written", path.display()))
    }

    /// Records `entry` as `id`'s install, and returns the addresses it no longer
    /// names, which the caller removes from the store.
    ///
    /// The entry's `enabled` isn't taken from the argument: installing the same
    /// content again keeps what the person decided about it, and installing
    /// different content starts off, because turning a plugin on is consent to
    /// the code that was reviewed and this is other code. [`LockFile::set_enabled`]
    /// is how the page turns a plugin on.
    pub fn record(&mut self, id: PluginId, mut entry: LockEntry) -> Vec<String> {
        let mut unneeded: Vec<String> = Vec::new();
        if let Some(old) = self.plugins.remove(&id) {
            if old.current.store == entry.current.store {
                entry.enabled = old.enabled;
                entry.previous = old.previous;
            } else {
                entry.enabled = false;
                let mut kept = Vec::with_capacity(MAX_PREVIOUS);
                for pinned in std::iter::once(old.current).chain(old.previous) {
                    if kept.len() < MAX_PREVIOUS && pinned.store != entry.current.store {
                        kept.push(pinned);
                    } else {
                        unneeded.push(pinned.store);
                    }
                }
                entry.previous = kept;
            }
        }
        self.plugins.insert(id, entry);
        // A version another plugin still names stays, whatever this install
        // dropped: two plugins may share one module.
        unneeded.retain(|address| !self.plugins.values().any(|entry| entry.names(address)));
        unneeded.dedup();
        unneeded
    }

    /// A plugin's entry, if it is installed.
    pub fn entry(&self, id: &PluginId) -> Option<&LockEntry> {
        self.plugins.get(id)
    }

    /// Turns an installed plugin on or off, the way its page does, and says
    /// whether that changed anything.
    pub fn set_enabled(&mut self, id: &PluginId, enabled: bool) -> bool {
        match self.plugins.get_mut(id) {
            Some(entry) if entry.enabled != enabled => {
                entry.enabled = enabled;
                true
            }
            _ => false,
        }
    }
}

/// The module of a stored version, after checking it against what the lock
/// recorded.
///
/// A module is loaded by path and never through the lock alone: a file that was
/// changed on disk after its install would otherwise run with the permissions a
/// person agreed to for different bytes. The manifest is checked the same way,
/// since a changed manifest changes what the interface offers and what the
/// module is asked to do.
pub fn verified_module(store: &Store, pinned: &Pinned) -> anyhow::Result<PathBuf> {
    let stored = store.read(&pinned.store)?;
    let manifest_sha256 = validate::sha256_bytes(stored.manifest_text.as_bytes());
    if manifest_sha256 != pinned.manifest_sha256 {
        bail!(
            "the stored `{}` doesn't hash to what the lock recorded for `{}`, so it was changed \
             on disk; reinstall the plugin",
            store::STORED_MANIFEST,
            pinned.source
        );
    }
    let Some(path) = stored.module_path()? else {
        bail!(
            "`{}` records no module, so there is nothing to load for `{}`",
            LOCK_FILE,
            pinned.source
        );
    };
    let Some(expected) = pinned.wasm_sha256.as_deref() else {
        bail!(
            "`{}` records no module hash for `{}`, so its module can't be trusted",
            LOCK_FILE,
            pinned.source
        );
    };
    let actual = sha256_file(&path)
        .with_context(|| format!("`{}` couldn't be read for its hash", path.display()))?;
    if actual != expected {
        bail!(
            "`{}` doesn't hash to what the lock recorded, so it was changed on disk since it was \
             reviewed; reinstall the plugin",
            path.display()
        );
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pinned(store: &str) -> Pinned {
        Pinned {
            source: format!("https://example.com/plugin@{store}"),
            path: None,
            reference: "HEAD".into(),
            rev: "0".repeat(40),
            wasm_sha256: Some(store.to_owned()),
            manifest_sha256: "1".repeat(64),
            store: store.to_owned(),
            origin: "https://example.com/plugin".into(),
            version: "1.0.0".into(),
        }
    }

    fn entry(store: &str) -> LockEntry {
        LockEntry {
            current: pinned(store),
            enabled: false,
            previous: Vec::new(),
        }
    }

    fn id() -> PluginId {
        PluginId::parse("com.example.plugin").unwrap()
    }

    #[test]
    fn a_new_version_keeps_the_one_before_it_and_drops_the_third() {
        let mut lock = LockFile::default();
        assert!(lock.record(id(), entry("a")).is_empty());
        assert!(lock.record(id(), entry("b")).is_empty());
        let dropped = lock.record(id(), entry("c"));
        assert_eq!(dropped, vec!["a".to_owned()]);
        let kept = lock.entry(&id()).unwrap();
        assert_eq!(kept.current.store, "c");
        assert_eq!(kept.previous.len(), 1);
        assert_eq!(kept.previous[0].store, "b");
    }

    #[test]
    fn the_same_content_again_keeps_what_the_person_decided() {
        let mut lock = LockFile::default();
        assert!(lock.record(id(), entry("a")).is_empty());
        assert!(lock.set_enabled(&id(), true));
        assert!(lock.plugins[&id()].enabled);
        assert!(!lock.set_enabled(&id(), true), "nothing changed");
        assert!(lock.record(id(), entry("a")).is_empty());
        assert!(
            lock.plugins[&id()].enabled,
            "re-fetching the same content keeps the decision"
        );
    }

    #[test]
    fn a_new_version_is_not_enabled_by_being_installed() {
        let mut lock = LockFile::default();
        lock.record(id(), entry("a"));
        lock.set_enabled(&id(), true);
        lock.record(id(), entry("b"));
        assert!(!lock.plugins[&id()].enabled);
    }

    #[test]
    fn an_address_another_plugin_still_names_is_kept() {
        let mut lock = LockFile::default();
        let other = PluginId::parse("com.example.other").unwrap();
        lock.record(id(), entry("shared"));
        lock.record(other, entry("shared"));
        let dropped = lock.record(id(), entry("new"));
        assert!(dropped.is_empty(), "the other plugin still names it");
    }

    #[test]
    fn round_trips_through_json_with_the_fields_a_reader_expects() {
        let mut lock = LockFile::default();
        lock.record(id(), entry("a"));
        let text = serde_json::to_string(&lock).unwrap();
        assert!(text.contains("\"ref\""));
        assert!(text.contains("\"com.example.plugin\""));
        let back: LockFile = serde_json::from_str(&text).unwrap();
        assert_eq!(back, lock);
    }

    #[test]
    fn a_newer_lock_file_is_refused() {
        let scratch = super::super::Scratch::new(&std::env::temp_dir()).unwrap();
        let root = scratch.path();
        std::fs::write(root.join(LOCK_FILE), r#"{"version": 2, "plugins": {}}"#).unwrap();
        let error = LockFile::load(root).unwrap_err().to_string();
        assert!(error.contains("newer Yapfast"), "{error}");
    }

    #[test]
    fn a_missing_lock_file_is_an_empty_one() {
        let scratch = super::super::Scratch::new(&std::env::temp_dir()).unwrap();
        assert_eq!(LockFile::load(scratch.path()).unwrap(), LockFile::default());
    }
}
