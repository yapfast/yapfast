//! The content-addressed store: one directory per install, written atomically.
//!
//! A version lives at `store/<address>/`, and the address is a hash of both the
//! module and the manifest, because both decide what runs: a changed manifest
//! must never be read beside an older module. `lock.json` sits beside those
//! directories, so a plugin is found by the lock rather than by walking the
//! store.
//!
//! Everything is written into a temporary directory inside the store and renamed
//! into place, and every file is synced first. A rename inside one directory is
//! atomic, so a reader either sees a whole install or no install at all, never
//! half of one: a module that was still being copied can't be the one that runs.
//!
//! The files inside keep the paths their manifest names — `plugin.wasm`,
//! `icon.png`, a folder of assets — so the manifest resolves the same way in the
//! store as it did in the repository; the manifest itself is also written as
//! `manifest.json`, which is the copy a reader parses.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, bail};
use yapfast_plugin_api::manifest::PluginManifest;

use super::Scratch;
use super::validate::sha256_bytes;

/// The manifest inside a version's directory.
pub const STORED_MANIFEST: &str = "manifest.json";

/// The store, at the directory that holds `lock.json` and one directory per
/// install.
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// Opens the store at `root`, creating it if it isn't there yet.
    pub fn new(root: impl Into<PathBuf>) -> std::io::Result<Store> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Store { root })
    }

    /// The store directory itself, where the lock file sits.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the files of the install at `address` live.
    pub fn dir(&self, address: &str) -> PathBuf {
        self.root.join(address)
    }

    /// Writes a staged plugin directory into the store at `address`.
    ///
    /// `staging` holds the plugin's files at their paths relative to it, and
    /// `manifest_name` is the file inside it to write as `manifest.json`. The
    /// address is content-derived, so committing bytes that are already stored
    /// leaves the existing directory alone.
    pub fn commit(
        &self,
        address: &str,
        staging: &Path,
        manifest_name: &str,
    ) -> anyhow::Result<PathBuf> {
        check_address(address)?;
        let destination = self.dir(address);
        if destination.is_dir() {
            return Ok(destination);
        }
        let mut scratch = Scratch::new(&self.root)
            .with_context(|| format!("`{}` can't be written to", self.root.display()))?;
        copy_tree(staging, scratch.path(), Some(manifest_name))?;
        let manifest = staging.join(manifest_name);
        let stored = scratch.path().join(STORED_MANIFEST);
        std::fs::copy(&manifest, &stored).with_context(|| {
            format!("`{}` couldn't be copied into the store", manifest.display())
        })?;
        sync_file(&stored)?;
        sync_directory(scratch.path());
        std::fs::rename(scratch.path(), &destination)
            .with_context(|| format!("`{}` couldn't be moved into place", destination.display()))?;
        scratch.keep();
        sync_directory(&self.root);
        Ok(destination)
    }

    /// Reads the manifest of the install at `address`.
    pub fn read(&self, address: &str) -> anyhow::Result<Stored> {
        check_address(address)?;
        let dir = self.dir(address);
        let path = dir.join(STORED_MANIFEST);
        let manifest_text = std::fs::read_to_string(&path)
            .with_context(|| format!("`{}` isn't installed", dir.display()))?;
        let manifest = PluginManifest::parse(&manifest_text).map_err(|error| {
            anyhow::anyhow!("`{}` doesn't hold a plugin: {error}", path.display())
        })?;
        Ok(Stored {
            dir,
            manifest,
            manifest_text,
        })
    }

    /// Removes the files of the install at `address`, if they are there.
    pub fn remove(&self, address: &str) -> anyhow::Result<()> {
        check_address(address)?;
        let dir = self.dir(address);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("`{}` couldn't be removed", dir.display()))
            }
        }
    }

    /// Every address the store holds, for finding what the lock doesn't name.
    pub fn versions(&self) -> anyhow::Result<Vec<String>> {
        let mut addresses = Vec::new();
        let entries = std::fs::read_dir(&self.root)
            .with_context(|| format!("`{}` couldn't be read", self.root.display()))?;
        for entry in entries {
            let entry =
                entry.with_context(|| format!("`{}` couldn't be read", self.root.display()))?;
            if !entry.path().is_dir() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str()
                && is_address(name)
            {
                addresses.push(name.to_owned());
            }
        }
        addresses.sort();
        Ok(addresses)
    }
}

/// A plugin's files in the store, with its manifest read.
pub struct Stored {
    /// The version's directory.
    pub dir: PathBuf,
    /// The manifest, parsed as the plugin declares it.
    pub manifest: PluginManifest,
    /// The manifest's text, for the hash the lock recorded.
    pub manifest_text: String,
}

impl Stored {
    /// The module's path, refusing a manifest that names one the store doesn't
    /// hold.
    ///
    /// The manifest's own path rules apply here too: a module is a file inside
    /// the version's directory, never a path that climbs out of it.
    pub fn module_path(&self) -> anyhow::Result<Option<PathBuf>> {
        let Some(wasm) = self
            .manifest
            .wasm
            .as_ref()
            .and_then(|path| self.manifest.resolve(path))
        else {
            return Ok(None);
        };
        let path = self.dir.join(&wasm);
        if !path.is_file() {
            bail!(
                "`{}` names `{}`, which isn't in `{}`",
                STORED_MANIFEST,
                wasm.display(),
                self.dir.display()
            );
        }
        Ok(Some(path))
    }
}

/// The address a set of hashes is stored under.
///
/// Both hashes go in, with a tag saying what the input means, so that a new
/// version of the store's layout can never collide with this one's addresses.
pub fn address(wasm_sha256: Option<&str>, manifest_sha256: &str) -> String {
    let mut text = String::from("yapfast-plugin-store-v1\n");
    text.push_str(wasm_sha256.unwrap_or("-"));
    text.push('\n');
    text.push_str(manifest_sha256);
    sha256_bytes(text.as_bytes())
}

/// Writes `bytes` to `path` through a synced temporary file, so a reader never
/// sees a partly written file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::File::create(&temp)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    if let Err(error) = written {
        drop(file);
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    drop(file);
    std::fs::rename(&temp, path)?;
    sync_directory(parent);
    Ok(())
}

/// Copies a staged plugin directory, keeping each file's path.
fn copy_tree(from: &Path, to: &Path, skip: Option<&str>) -> anyhow::Result<()> {
    let entries = std::fs::read_dir(from)
        .with_context(|| format!("`{}` couldn't be read", from.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("`{}` couldn't be read", from.display()))?;
        let name = entry.file_name();
        let metadata = entry
            .metadata()
            .with_context(|| format!("`{}` couldn't be measured", entry.path().display()))?;
        if metadata.is_dir() {
            let dir = to.join(&name);
            std::fs::create_dir(&dir)
                .with_context(|| format!("`{}` couldn't be created", dir.display()))?;
            copy_tree(&entry.path(), &dir, None)?;
        } else if metadata.is_file() {
            if skip.is_some_and(|skip| name.to_string_lossy() == skip) {
                continue;
            }
            let destination = to.join(&name);
            std::fs::copy(entry.path(), &destination).with_context(|| {
                format!(
                    "`{}` couldn't be copied into the store",
                    destination.display()
                )
            })?;
            sync_file(&destination)?;
        } else {
            bail!(
                "`{}` isn't a file or a directory, and a plugin holds only those",
                entry.path().display()
            );
        }
    }
    Ok(())
}

/// Refuses anything but an address this store could have written, so a lock file
/// can't name a path of its own.
fn check_address(address: &str) -> anyhow::Result<()> {
    if is_address(address) {
        return Ok(());
    }
    bail!("`{address}` isn't an address in the plugin store")
}

/// Whether a name is one of the store's addresses.
fn is_address(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Pushes a file's bytes to the disk.
fn sync_file(path: &Path) -> anyhow::Result<()> {
    std::fs::File::open(path)
        .and_then(|file| file.sync_all())
        .with_context(|| format!("`{}` couldn't be synced", path.display()))
}

/// Pushes a directory's own entries to the disk.
///
/// Windows can't open a directory as a file, so a failure there is not the
/// install's: the files themselves are already synced.
fn sync_directory(directory: &Path) {
    if let Ok(handle) = std::fs::File::open(directory) {
        let _ = handle.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_covers_both_hashes() {
        let one = address(Some("a"), "b");
        assert_eq!(one.len(), 64);
        assert_ne!(one, address(Some("a"), "c"));
        assert_ne!(one, address(Some("d"), "b"));
        assert_ne!(one, address(None, "b"));
        assert_eq!(one, address(Some("a"), "b"));
    }

    #[test]
    fn refuses_a_name_that_is_not_an_address() {
        for name in ["..", "abc", "../../etc", &"A".repeat(64)] {
            assert!(check_address(name).is_err(), "{name} should be refused");
        }
        assert!(check_address(&address(Some("a"), "b")).is_ok());
    }
}
