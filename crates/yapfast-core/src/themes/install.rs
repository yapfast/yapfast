//! Installed themes are checked and pinned in a content-addressed store
//! under `<data>/themes`: `store/<sha256>/` holds each version's files and
//! `lock.json` names the version each theme is at, where it came from and
//! the one before, to roll back to. A theme comes from a local folder or, by
//! way of [`super::remote`], from a commit of a GitHub repository.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::check::{self, Files, Report, Strictness};
use super::{MANIFEST, Package};

const LOCK: &str = "lock.json";
const STORE: &str = "store";
/// The lock file version this reads and writes.
const LOCK_VERSION: u64 = 1;

/// A theme fetched from its repository and checked, waiting for the person to
/// install it. Nothing in it runs.
#[derive(Clone, Debug)]
pub struct Fetched {
    pub source: String,
    /// The folder in the repository, empty when `theme.json` is at its root.
    pub path: String,
    pub reference: String,
    /// The commit read, which is the pin.
    pub rev: String,
    pub files: Files,
    pub report: Report,
    /// It came from the marketplace's catalog, at the commit and digest it
    /// lists, rather than from an address the person gave.
    pub listed: bool,
}

impl Fetched {
    /// Checks what the helper read, refusing a source that isn't a public
    /// GitHub repository.
    pub fn new(
        source: String,
        path: String,
        reference: String,
        rev: String,
        files: Files,
    ) -> Result<Self, String> {
        if !check::github(&source) {
            return Err(
                "Themes come from public GitHub repositories for now; that address isn't one"
                    .into(),
            );
        }
        let report = check::check(&files, Strictness::Install);
        if let Some(reason) = report.reason() {
            return Err(format!("This theme can't be installed: {reason}"));
        }
        Ok(Fetched {
            source,
            path,
            reference,
            rev,
            files,
            report,
            listed: false,
        })
    }

    /// Reads and validates a local folder for installation.
    pub fn local(folder: &Path) -> Result<Self, String> {
        let folder = fs::canonicalize(folder).map_err(|error| error.to_string())?;
        let files = Files::read(&folder)?;
        let report = check::check(&files, Strictness::Install);
        if let Some(reason) = report.reason() {
            return Err(format!("This theme can't be installed: {reason}"));
        }
        Ok(Self {
            source: folder.to_string_lossy().into_owned(),
            path: String::new(),
            reference: String::new(),
            rev: String::new(),
            files,
            report,
            listed: false,
        })
    }

    pub fn package(&self) -> &Package {
        self.report
            .package
            .as_ref()
            .expect("a passed check has its package")
    }
}

/// One installed theme, as the lock file names it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Entry {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, rename = "ref")]
    pub reference: String,
    #[serde(default)]
    pub rev: String,
    #[serde(default)]
    pub store: String,
    #[serde(default)]
    pub manifest_sha256: String,
    #[serde(default)]
    pub origin: String,
    /// What it costs to run, as its check said at install: the gallery's
    /// line, which a version's files can't change.
    #[serde(default)]
    pub cost: String,
    #[serde(default)]
    pub previous: Option<Box<Entry>>,
}

/// An installed theme, read back from the store and checked again.
#[derive(Clone, Debug)]
pub struct Installed {
    pub id: String,
    pub entry: Entry,
    pub folder: PathBuf,
    pub package: Package,
}

/// The installed theme store.
pub struct Themes {
    root: PathBuf,
}

impl Themes {
    /// The store in a data folder.
    pub fn open(data_dir: &Path) -> Self {
        Themes {
            root: data_dir.join("themes"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every theme the lock file names, by id.
    pub fn entries(&self) -> Result<BTreeMap<String, Entry>> {
        let lock = self.lock_json()?;
        let Some(themes) = lock.get("themes") else {
            return Ok(BTreeMap::new());
        };
        serde_json::from_value(themes.clone()).context("the lock file's themes aren't readable")
    }

    /// Every installed theme whose files are where the lock file says and
    /// whose `theme.json` is the one installed. One that isn't is left out,
    /// with a warning; the rest still show.
    pub fn installed(&self) -> Vec<Installed> {
        self.installed_and_refused().0
    }

    /// The installed themes this Yapfast reads, and those it refuses with
    /// why, by id, so the window can say so instead of leaving one out
    /// without a word.
    pub fn installed_and_refused(&self) -> (Vec<Installed>, Vec<(String, String)>) {
        let entries = match self.entries() {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!(%error, "could not read the installed themes");
                return (Vec::new(), Vec::new());
            }
        };
        let (mut installed, mut refused) = (Vec::new(), Vec::new());
        for (id, entry) in entries {
            match self.load(&id, &entry) {
                Ok(theme) => installed.push(theme),
                Err(error) => {
                    tracing::warn!(%id, %error, "an installed theme was left out");
                    refused.push((id, format!("{error:#}")));
                }
            }
        }
        (installed, refused)
    }

    fn load(&self, id: &str, entry: &Entry) -> Result<Installed> {
        if !is_address(&entry.store) {
            anyhow::bail!("its store directory isn't one Yapfast names");
        }
        let folder = self.root.join(STORE).join(&entry.store);
        let text = fs::read_to_string(folder.join(MANIFEST))
            .with_context(|| format!("{} is missing", folder.display()))?;
        if hex(&Sha256::digest(text.as_bytes())) != entry.manifest_sha256 {
            anyhow::bail!("its theme.json changed since it was installed");
        }
        let package = Package::parse(&text).map_err(|error| anyhow::anyhow!(error.0))?;
        if package.id != id {
            anyhow::bail!("its theme.json is for {}", package.id);
        }
        Ok(Installed {
            id: id.to_owned(),
            entry: entry.clone(),
            folder,
            package,
        })
    }

    /// Writes a fetched theme into `store/` and `lock.json`: the version it
    /// replaces is kept to roll back to, the one before is removed.
    pub fn install(&self, fetched: &Fetched) -> Result<()> {
        let package = fetched.package();
        let id = package.id.as_str();
        let manifest_sha256 = hex(&Sha256::digest(fetched.files.manifest.as_bytes()));
        let address = digest(&fetched.files);
        let mut lock = self.lock_json()?;
        let installed = lock
            .get("themes")
            .and_then(|themes| themes.get(id))
            .cloned();
        if let Some(entry) = &installed {
            let source = entry.get("source").and_then(Value::as_str).unwrap_or("");
            let path = entry.get("path").and_then(Value::as_str).unwrap_or("");
            if source != fetched.source || path != fetched.path {
                anyhow::bail!("another theme already uses {id}");
            }
        }
        let mut entry = serde_json::json!({
            "source": fetched.source,
            "path": fetched.path,
            "ref": if fetched.reference.is_empty() { "HEAD" } else { fetched.reference.as_str() },
            "rev": fetched.rev,
            "origin": if fetched.listed {
                "marketplace"
            } else if check::github(&fetched.source) {
                "url"
            } else {
                "local"
            },
            "store": address,
            "manifest_sha256": manifest_sha256,
            "cost": fetched.report.cost.text(),
        });
        // The version replaced is kept to roll back to, unless this install
        // is the same files again.
        if let Some(mut previous) = installed
            && previous.get("store").and_then(Value::as_str) != Some(address.as_str())
        {
            if let Some(object) = previous.as_object_mut() {
                object.remove("previous");
            }
            entry
                .as_object_mut()
                .expect("built as an object")
                .insert("previous".into(), previous);
        }
        let before = self.store_dirs(&lock);
        self.write_folder(&fetched.files, &address)?;
        let object = lock
            .as_object_mut()
            .context("the lock file isn't an object")?;
        object
            .entry("themes")
            .or_insert_with(|| Value::Object(Default::default()))
            .as_object_mut()
            .context("the lock file's themes aren't an object")?
            .insert(id.to_owned(), entry);
        self.write_lock(&lock)?;
        self.prune(&before, &lock);
        Ok(())
    }

    /// Goes back to the version the last install replaced.
    pub fn roll_back(&self, id: &str) -> Result<()> {
        let mut lock = self.lock_json()?;
        let before = self.store_dirs(&lock);
        let themes = lock
            .get_mut("themes")
            .and_then(Value::as_object_mut)
            .context("no theme is installed")?;
        let entry = themes.get_mut(id).context("that theme isn't installed")?;
        let previous = entry
            .get("previous")
            .cloned()
            .context("there's no earlier version to go back to")?;
        *entry = previous;
        self.write_lock(&lock)?;
        self.prune(&before, &lock);
        Ok(())
    }

    /// Removes a theme and its files.
    pub fn uninstall(&self, id: &str) -> Result<()> {
        let mut lock = self.lock_json()?;
        let before = self.store_dirs(&lock);
        if let Some(themes) = lock.get_mut("themes").and_then(Value::as_object_mut) {
            themes.remove(id);
        }
        self.write_lock(&lock)?;
        self.prune(&before, &lock);
        Ok(())
    }

    fn lock_json(&self) -> Result<Value> {
        let path = self.root.join(LOCK);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(serde_json::json!({ "version": LOCK_VERSION, "themes": {} }));
            }
            Err(error) => {
                return Err(error).with_context(|| format!("could not read {}", path.display()));
            }
        };
        let lock: Value = serde_json::from_str(&text)
            .with_context(|| format!("{} isn't a lock file", path.display()))?;
        let version = lock.get("version").and_then(Value::as_u64).unwrap_or(0);
        if version > LOCK_VERSION {
            anyhow::bail!(
                "{} is a version {version} lock file and this build reads version {LOCK_VERSION}",
                path.display()
            );
        }
        Ok(lock)
    }

    fn write_lock(&self, lock: &Value) -> Result<()> {
        let text = serde_json::to_vec_pretty(lock).context("could not write the lock file")?;
        write_atomically(&self.root.join(LOCK), &text)
    }

    /// Every store directory the lock file's themes name.
    fn store_dirs(&self, lock: &Value) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        let entries = lock
            .get("themes")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|themes| themes.values());
        for entry in entries {
            for named in [Some(entry), entry.get("previous")].into_iter().flatten() {
                if let Some(store) = named.get("store").and_then(Value::as_str)
                    && is_address(store)
                {
                    let directory = self.root.join(STORE).join(store);
                    if !dirs.contains(&directory) {
                        dirs.push(directory);
                    }
                }
            }
        }
        dirs
    }

    fn prune(&self, before: &[PathBuf], lock: &Value) {
        let after = self.store_dirs(lock);
        for directory in before.iter().filter(|directory| !after.contains(directory)) {
            if let Err(error) = fs::remove_dir_all(directory)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%error, "could not remove a theme version from the store");
            }
        }
    }

    /// Writes the files into `store/<address>/` through a temporary
    /// directory renamed into place, so a half-written theme is never named.
    fn write_folder(&self, files: &Files, address: &str) -> Result<()> {
        let target = self.root.join(STORE).join(address);
        if target.join(MANIFEST).is_file() {
            return Ok(());
        }
        let temporary = self.root.join(STORE).join(format!(".{address}.tmp"));
        let _ = fs::remove_dir_all(&temporary);
        fs::create_dir_all(&temporary)
            .with_context(|| format!("could not create {}", temporary.display()))?;
        write_file(&temporary.join(MANIFEST), files.manifest.as_bytes())?;
        for (relative, bytes) in &files.files {
            let path = temporary.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("could not create {}", parent.display()))?;
            }
            write_file(&path, bytes)?;
        }
        if let Some((licence, bytes)) = &files.licence {
            write_file(&temporary.join(licence), bytes)?;
        }
        let _ = fs::remove_dir_all(&target);
        fs::rename(&temporary, &target)
            .with_context(|| format!("could not move the theme into {}", target.display()))
    }
}

/// A theme's files named by one digest: `theme.json`'s, then each file's path
/// and digest. Two versions that differ in one font have two; it names the
/// store directory a version is installed in, and is the `sha256` the
/// marketplace's catalog pins a listed theme to.
pub fn digest(files: &Files) -> String {
    let mut digest = Sha256::new();
    digest.update(b"theme.json:");
    digest.update(hex(&Sha256::digest(files.manifest.as_bytes())).as_bytes());
    for (path, bytes) in &files.files {
        digest.update(b"\n");
        digest.update(portable(path).as_bytes());
        digest.update(b":");
        digest.update(hex(&Sha256::digest(bytes)).as_bytes());
    }
    hex(&digest.finalize())
}

/// A path inside a theme as `/`-separated text, the same on every system.
fn portable(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// A store directory's name: 64 lowercase hex digits, so a lock file can't
/// name a directory outside `store/`.
fn is_address(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file =
        fs::File::create(path).with_context(|| format!("could not write {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("could not write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("could not flush {}", path.display()))
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn files(version: &str, chat: &str) -> Files {
        Files {
            manifest: format!(
                r##"{{ "theme_api": 1, "id": "com.example.plum", "name": "Plum", "version": "{version}",
                "variants": {{ "dark": {{ "base": "yap", "colours": {{ "chat": "{chat}" }} }} }} }}"##
            ),
            ..Files::default()
        }
    }

    fn fetched(version: &str, chat: &str) -> Fetched {
        Fetched::new(
            "https://github.com/ada/plum".into(),
            String::new(),
            "main".into(),
            format!("{version}-commit"),
            files(version, chat),
        )
        .unwrap()
    }

    #[test]
    fn local_folder_installs_into_the_configured_store() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join(MANIFEST), files("1.0.0", "#2a1a3a").manifest).unwrap();
        let themes = Themes::open(directory.path());
        assert_eq!(themes.root(), directory.path().join("themes"));
        themes.install(&Fetched::local(&source).unwrap()).unwrap();
        let installed = themes.installed();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].entry.origin, "local");
        assert_eq!(installed[0].package.id, "com.example.plum");
    }

    #[test]
    fn only_github_sources_install() {
        let error = Fetched::new(
            "https://codeberg.org/ada/plum".into(),
            String::new(),
            "main".into(),
            "abc".into(),
            files("1.0.0", "#2a1a3a"),
        )
        .unwrap_err();
        assert!(error.contains("GitHub"), "{error}");
    }

    #[test]
    fn installs_pins_rolls_back_and_uninstalls() {
        let directory = tempfile::tempdir().unwrap();
        let themes = Themes::open(directory.path());
        // Whatever else the lock file holds survives an install.
        fs::create_dir_all(themes.root()).unwrap();
        fs::write(
            themes.root().join(LOCK),
            r#"{ "version": 1, "themes": {}, "note": "kept" }"#,
        )
        .unwrap();
        themes.install(&fetched("1.0.0", "#2a1a3a")).unwrap();
        themes.install(&fetched("1.1.0", "#2b1b3b")).unwrap();
        let installed = themes.installed();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].package.version, "1.1.0");
        assert_eq!(installed[0].entry.rev, "1.1.0-commit");
        assert_eq!(
            installed[0].entry.previous.as_ref().unwrap().rev,
            "1.0.0-commit"
        );
        let lock: Value =
            serde_json::from_str(&fs::read_to_string(themes.root().join(LOCK)).unwrap()).unwrap();
        assert_eq!(lock["note"], "kept", "the rest of the lock file stays");

        themes.roll_back("com.example.plum").unwrap();
        assert_eq!(themes.installed()[0].package.version, "1.0.0");
        themes.uninstall("com.example.plum").unwrap();
        assert!(themes.installed().is_empty());
        let left: Vec<_> = fs::read_dir(themes.root().join(STORE)).unwrap().collect();
        assert!(left.is_empty(), "no theme's files stay behind");
    }

    #[test]
    fn a_changed_theme_json_in_the_store_is_left_out() {
        let directory = tempfile::tempdir().unwrap();
        let themes = Themes::open(directory.path());
        themes.install(&fetched("1.0.0", "#2a1a3a")).unwrap();
        let folder = themes.installed()[0].folder.clone();
        fs::write(folder.join(MANIFEST), files("1.0.0", "#000000").manifest).unwrap();
        assert!(themes.installed().is_empty());
    }

    #[test]
    fn an_exported_custom_theme_installs_with_its_colours_fonts_and_art() {
        use crate::themes::Mode;
        use crate::themes::package::{self, Paint, Role};
        let colours = [
            ("chat", 0xff2a_1a3a),
            ("sidebar", 0xff22_1430),
            ("rail", 0xff1a_0f26),
            ("accent", 0xffff_9e64),
            ("selection", 0x59ff_9e64),
        ];
        let font: &[u8] = include_bytes!("../../../../ui/fonts/DisplayName-Journal.ttf");
        let picture: &[u8] = include_bytes!("../../../../themes/template/preview.png");
        // The custom theme's own folder, as a theme creator keeps it: an
        // imported font, a background picture and their licences.
        let directory = tempfile::tempdir().unwrap();
        let own = directory.path().join("own");
        fs::create_dir_all(own.join("fonts")).unwrap();
        fs::create_dir_all(own.join("art")).unwrap();
        fs::write(own.join("fonts/Journal.ttf"), font).unwrap();
        fs::write(own.join("fonts/OFL.txt"), "SIL Open Font License 1.1").unwrap();
        fs::write(own.join("art/sky.png"), picture).unwrap();
        fs::write(own.join("notes.txt"), "not part of it").unwrap();
        let layers = r##"{ "license": "CC0-1.0 AND OFL-1.1",
            "type": { "body": { "font": "fonts/Journal.ttf", "size": 16 } },
            "art": { "surfaces": { "window": { "image": "art/sky.png" } } },
            "variants": { "dark": { "base": "nord", "colours": { "chat": "#000000", "link": "#7fb4ff" } } } }"##;
        let text =
            package::export_with("Plum Night", "Ada", "yap", true, &colours, Some(layers)).unwrap();
        let folder = directory.path().join(package::folder_name("Plum Night"));
        let written = package::write_export(&folder, &text, Some(&own)).unwrap();
        for path in [
            "fonts/Journal.ttf",
            "art/sky.png",
            "fonts/OFL.txt",
            "LICENSE",
        ] {
            assert!(
                written.contains(&PathBuf::from(path)),
                "{path}: {written:?}"
            );
        }
        assert!(!folder.join("notes.txt").exists(), "only what it names");

        let read = Files::read(&folder).unwrap();
        assert!(read.licence.is_some(), "a licence file to fill in");
        let report = check::check(&read, Strictness::Install);
        assert!(report.passed(), "{}", report.text());
        let package = report.package.clone().unwrap();
        assert_eq!(package.authors, ["Ada"]);
        assert_eq!(package.license.as_deref(), Some("CC0-1.0 AND OFL-1.1"));
        // As Add from URL would, once the folder is a repository.
        let fetched = Fetched::new(
            "https://github.com/ada/plum-night".into(),
            String::new(),
            "main".into(),
            "1-commit".into(),
            read,
        )
        .unwrap();
        let store = tempfile::tempdir().unwrap();
        let themes = Themes::open(store.path());
        themes.install(&fetched).unwrap();
        let installed = themes.installed();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].id, "local.plum-night");
        let resolved = installed[0].package.resolve(Mode::Dark).unwrap();
        assert_eq!(resolved.base, "yap", "the custom theme's base");
        for (token, colour) in colours {
            assert_eq!(
                resolved.paint(token),
                Some(&Paint::Colour(colour)),
                "{token} comes back as it was exported"
            );
        }
        assert_eq!(
            resolved.paint("link"),
            Some(&Paint::Colour(0xff7f_b4ff)),
            "the folder's own colours under the custom theme's"
        );
        let body = resolved.types[&Role::Body].font.clone().unwrap();
        assert_eq!(fs::read(installed[0].folder.join(&body)).unwrap(), font);
        assert_eq!(
            fs::read(installed[0].folder.join("art/sky.png")).unwrap(),
            picture
        );
        assert!(resolved.fonts().contains(&body.as_path()));
    }

    #[test]
    fn an_export_refuses_a_file_its_folder_lacks() {
        use crate::themes::package;
        let text = package::export_with(
            "Gone",
            "",
            "yap",
            true,
            &[],
            Some(r#"{ "art": { "surfaces": { "window": { "image": "art/gone.png" } } } }"#),
        )
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let error = package::write_export(
            &directory.path().join("gone"),
            &text,
            Some(directory.path()),
        )
        .unwrap_err();
        assert!(error.contains("art/gone.png"), "{error}");
    }

    #[test]
    fn a_lock_file_cannot_point_outside_the_store() {
        assert!(!is_address("../../etc"));
        assert!(is_address(&"a".repeat(64)));
    }
}
