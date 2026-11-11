//! `yapfast-plugins fetch`: what a plugin's repository holds, read once and
//! handed to the parent.
//!
//! It runs for seconds in a process of its own, so `gix` and its 18 MiB leave
//! with it. Nothing a fetched repository contains is executed: the manifest is
//! read as JSON, the module and the icon are read as bytes from git's objects,
//! and no file the repository names ever becomes a path this process opens for
//! writing outside a temporary directory of its own.
//!
//! ## How the two sides talk
//!
//! This is the whole protocol: **one JSON object on stdout**, tagged by `kind`.
//!
//! ```json
//! {"kind":"plugin","source":"…","path":"plugins/foo","reference":"main",
//!  "rev":"051b765b…","manifest":"{…}","wasm":"/tmp/.yapfast-plugin-1-0/plugin.wasm",
//!  "icon":null}
//! {"kind":"index","source":"…","paths":["plugins/foo","plugins/quiet-hours"]}
//! {"kind":"error","error":"That repository has no yapfast-plugin.json at its root."}
//! ```
//!
//! The module and the icon are written to a temporary directory under the
//! system temp directory, whose path the object carries; the parent copies them
//! into the store and deletes the directory. Nothing is copied into the store
//! from here, and a fetch that prints nothing successful leaves nothing behind.
//! The directory is [`std::mem::forget`]-left in place only after the object
//! naming it has been written, because a report that never reached the parent
//! would otherwise name files this process deleted.
//!
//! It is JSON and files rather than one self-contained object because a module
//! is up to 8 MiB and the parent wants to write it into the store once, not
//! carry it through a pipe; and it is a separate process at all because this
//! binary links the git machinery the app must never hold.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use yapfast_plugin_api::manifest::PluginManifest;
use yapfast_plugin_api::{API_VERSION, limits};

mod git;
mod imports;
mod registry;
mod url;

pub use registry::{run_catalog, run_verify};

use git::Cap;
use url::Source;

/// The manifest a plugin's folder holds.
pub const MANIFEST_FILE: &str = "yapfast-plugin.json";

/// The index a repository's root may hold, so a URL that names no folder can
/// still offer what the repository carries.
pub const INDEX_FILE: &str = "yapfast-plugins.json";

/// The largest manifest read: a manifest is metadata, which runs to kilobytes.
const MANIFEST_CAP: Cap = Cap {
    bytes: 1 << 20,
    label: "a manifest is at most 1 MiB",
};

/// An index is a handful of folder names, and is bounded like a manifest.
const INDEX_CAP: Cap = Cap {
    bytes: 1 << 20,
    label: "an index is at most 1 MiB",
};

/// A module's cap, the same number the parent refuses one at.
const MODULE_CAP: Cap = Cap {
    bytes: limits::MAX_WASM_BYTES,
    label: "a module is at most 8 MiB",
};

/// An icon's cap, the size a registry may hold one at: a picture the review
/// dialog draws at 64×64 to 256×256, and the marketplace at 128×128.
const ICON_CAP: Cap = Cap {
    bytes: limits::MAX_ICON_BYTES,
    label: "an icon is at most 64 KiB",
};

/// What a fetch found, as the parent reads it.
///
/// The parent mirrors this shape in its own crate; the field names are the
/// protocol, so changing one changes both sides.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Report {
    /// One plugin: its folder, its commit, and where its files were written.
    Plugin {
        /// The repository's address, as the lock file records it: no ref and no
        /// folder in it, since those are the two fields below.
        source: String,
        /// The folder inside the repository, empty when the plugin is at its
        /// root. It travels with the pin, so an update reads the same folder.
        path: String,
        /// The branch, tag or commit asked for, or `HEAD`.
        reference: String,
        /// The full id of the commit that was read.
        rev: String,
        /// `yapfast-plugin.json`'s text, exactly as the repository holds it.
        manifest: String,
        /// The module, written under the temporary directory; `null` for a
        /// plugin that only sets tweaks, commands or settings.
        wasm: Option<String>,
        /// The icon, when the manifest names one and it was read.
        icon: Option<String>,
    },
    /// A repository that holds several plugins and no manifest at its root: the
    /// folders its index lists, each reviewed and installed on its own.
    Index {
        /// The repository's address, as the lock file records it.
        source: String,
        /// The folders the index lists, each holding a `yapfast-plugin.json`.
        paths: Vec<String>,
    },
    /// Why there is nothing to install, as one sentence a person reads.
    Error {
        /// The sentence itself.
        error: String,
    },
    /// A registry, read for the marketplace tab: its index, its entries, and
    /// where its icons were written.
    Catalog {
        /// The registry's address, as the page shows it.
        source: String,
        /// The commit it was read at.
        rev: String,
        /// `index.json`'s text, which the app reads the ids and the delistings
        /// from.
        index: String,
        /// Each entry file, by its path in the registry.
        entries: Vec<EntryFile>,
        /// Each icon, by its path in the registry and where it was written.
        icons: Vec<IconFile>,
        /// The directory the icons were written to, which the parent deletes
        /// once it has read them; `null` when there are none.
        scratch: Option<String>,
    },
    /// One entry, checked against the plugin it pins. This is what a
    /// registry's CI prints for an entry it accepts.
    Verified {
        /// The plugin's id, as the entry declares it.
        id: String,
        /// The commit that was read, which is the entry's pin.
        rev: String,
        /// The SHA-256 of the module at that commit, which is the entry's.
        sha256: String,
    },
}

/// One entry file of a registry, as text.
#[derive(Debug, Serialize)]
pub struct EntryFile {
    /// Its path in the registry, as `plugins/<id>.json`.
    pub path: String,
    /// The file's text, exactly as the repository holds it.
    pub text: String,
}

/// One icon of a registry, written where the parent copies it from.
#[derive(Debug, Serialize)]
pub struct IconFile {
    /// Its path in the registry, as `icons/<id>.webp`.
    pub path: String,
    /// The file it was written to.
    pub file: String,
    /// Its size in bytes, which the parent caps.
    pub bytes: u64,
}

/// What a fetch found, and the files it wrote, which go when this does.
#[derive(Debug)]
pub struct Fetched {
    /// What to print for the parent.
    pub report: Report,
    /// The directory the module and icon were written to, if any.
    handoff: Option<Scratch>,
}

impl Fetched {
    /// Leaves the written files in place, for the parent to copy and delete.
    pub fn keep(self) {
        if let Some(handoff) = self.handoff {
            handoff.keep();
        }
    }
}

/// Runs `fetch` with the arguments after the subcommand, and returns the exit
/// status: zero when a plugin or an index was printed, one when an error was.
pub fn run(arguments: Vec<String>) -> i32 {
    match url(arguments).and_then(|url| fetch(&url)) {
        Ok(fetched) => {
            // The files stay only now: a report that failed to print has told
            // the parent nothing, so nothing would come for them.
            if let Err(error) = print(&fetched.report) {
                eprintln!("yapfast-plugins: the report couldn't be written: {error}");
                return 1;
            }
            fetched.keep();
            0
        }
        Err(error) => {
            let report = Report::Error {
                error: error.to_string(),
            };
            if let Err(write) = print(&report) {
                eprintln!("yapfast-plugins: the error couldn't be written: {write}");
            }
            1
        }
    }
}

/// Fetches what `url` names, and writes the files it read to a directory of
/// their own.
///
/// On success the caller owns the files: they stay until the [`Fetched`] is
/// dropped, and [`Fetched::keep`] leaves them for the parent. On failure
/// everything this made is gone before the error returns.
pub fn fetch(url: &str) -> anyhow::Result<Fetched> {
    let source = Source::parse(url)?;
    // The scratch directory sits in the system temp directory, and holds the
    // bare repository alone; it is gone before this returns either way.
    let work = Scratch::new(".yapfast-fetch")?;
    let snapshot = git::Snapshot::open(&source, work.path())?;

    let Some(text) = snapshot.file(Path::new(MANIFEST_FILE), MANIFEST_CAP)? else {
        return index_or_nothing(&source, &snapshot);
    };
    let text = String::from_utf8(text)
        .map_err(|_| anyhow::anyhow!("`{MANIFEST_FILE}` isn't UTF-8 text"))?;
    let manifest = PluginManifest::parse(&text)
        .map_err(|error| anyhow::anyhow!("Its `{MANIFEST_FILE}` can't be used: {error}."))?;

    let handoff = Scratch::new(".yapfast-plugin")?;
    let wasm = module(&snapshot, &manifest, handoff.path())?;
    let icon = icon(&snapshot, &manifest, handoff.path())?;
    Ok(Fetched {
        report: Report::Plugin {
            source: source.repo.clone(),
            path: source.path.clone().unwrap_or_default(),
            reference: source.reference_name().to_owned(),
            rev: snapshot.commit.clone(),
            manifest: text,
            wasm,
            icon,
        },
        handoff: Some(handoff),
    })
}

/// A repository's root index, or the sentence that says why there is neither a
/// plugin nor one.
fn index_or_nothing(source: &Source, snapshot: &git::Snapshot) -> anyhow::Result<Fetched> {
    if let Some(path) = &source.path {
        bail!(
            "`{path}` in `{}` has no `{MANIFEST_FILE}`; a plugin's folder holds one",
            source.repo
        );
    }
    let Some(text) = snapshot.file(Path::new(INDEX_FILE), INDEX_CAP)? else {
        bail!(
            "`{}` has no `{MANIFEST_FILE}` at its root and no `{INDEX_FILE}` either; name one of \
             its folders with `#path=plugins/<name>`",
            source.repo
        );
    };
    let text =
        String::from_utf8(text).map_err(|_| anyhow::anyhow!("`{INDEX_FILE}` isn't UTF-8 text"))?;
    Ok(Fetched {
        report: Report::Index {
            source: source.repo.clone(),
            paths: index_paths(snapshot, &text)?,
        },
        handoff: None,
    })
}

/// The index's folders, each checked to be a folder of this commit that holds a
/// plugin.
fn index_paths(snapshot: &git::Snapshot, text: &str) -> anyhow::Result<Vec<String>> {
    let index: IndexText = serde_json::from_str(text)
        .map_err(|error| anyhow::anyhow!("`{INDEX_FILE}` isn't an index Yapfast reads: {error}"))?;
    if index.api != API_VERSION {
        bail!(
            "`{INDEX_FILE}` is for plugin API {}, and this Yapfast speaks {API_VERSION}",
            index.api
        );
    }
    let mut paths = Vec::with_capacity(index.plugins.len());
    for listed in index.plugins {
        // A folder that climbs out of the repository is refused before it is
        // looked for, and the index says what the repository holds, so a folder
        // it names that holds nothing is the repository's own bug: said once,
        // here, instead of at every install. The tree is already fetched, so
        // checking costs no second clone.
        let path = url::clean_path(&listed)?;
        // One folder listed twice is one plugin offered twice; an index holds
        // tens of folders, so a scan costs less than a set.
        if paths.contains(&path) {
            bail!("`{INDEX_FILE}` lists `{path}` twice");
        }
        let manifest = Path::new(&path).join(MANIFEST_FILE);
        if snapshot.file(&manifest, MANIFEST_CAP)?.is_none() {
            bail!("`{INDEX_FILE}` lists `{listed}`, which holds no `{MANIFEST_FILE}`");
        }
        paths.push(path);
    }
    Ok(paths)
}

/// A repository's index, as `{ "api": 1, "plugins": ["plugins/foo"] }`.
#[derive(Deserialize)]
struct IndexText {
    api: u32,
    plugins: Vec<String>,
}

/// Writes the module the manifest names under `out`, and returns its path.
fn module(
    snapshot: &git::Snapshot,
    manifest: &PluginManifest,
    out: &Path,
) -> anyhow::Result<Option<String>> {
    let Some(named) = &manifest.wasm else {
        // A plugin that only sets named points or changes the interface has no
        // module; there is nothing to write and nothing to load.
        return Ok(None);
    };
    let shown = named.display().to_string();
    let relative = manifest.resolve(named).with_context(|| {
        format!("`{MANIFEST_FILE}` names `{shown}`, which isn't a path inside the plugin's folder")
    })?;
    let bytes = snapshot
        .file(&relative, MODULE_CAP)?
        .with_context(|| format!("`{MANIFEST_FILE}` names `{shown}`, which isn't in the plugin"))?;
    // What it can reach is checked before the parent ever hears of it: the
    // magic, the version, and every import it declares.
    imports::check(&bytes, &shown, manifest.wasi)?;
    write(out, &relative, &bytes).map(Some)
}

/// Writes the icon the manifest names under `out`, and returns its path.
fn icon(
    snapshot: &git::Snapshot,
    manifest: &PluginManifest,
    out: &Path,
) -> anyhow::Result<Option<String>> {
    let Some(named) = &manifest.icon else {
        return Ok(None);
    };
    let shown = named.display().to_string();
    let relative = manifest.resolve(named).with_context(|| {
        format!("`{MANIFEST_FILE}` names `{shown}`, which isn't a path inside the plugin's folder")
    })?;
    let bytes = snapshot
        .file(&relative, ICON_CAP)?
        .with_context(|| format!("`{MANIFEST_FILE}` names `{shown}`, which isn't in the plugin"))?;
    write(out, &relative, &bytes).map(Some)
}

/// Writes one file where the parent can copy it from, at its path inside the
/// plugin, and returns the absolute path as text.
fn write(out: &Path, relative: &Path, bytes: &[u8]) -> anyhow::Result<String> {
    let target = out.join(relative);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("`{}` couldn't be made", parent.display()))?;
    }
    std::fs::write(&target, bytes)
        .with_context(|| format!("`{}` couldn't be written", target.display()))?;
    let path = target.to_str().with_context(|| {
        format!(
            "`{}` isn't a path the parent can read as text",
            target.display()
        )
    })?;
    Ok(path.to_owned())
}

/// The one URL `fetch` was given.
fn url(arguments: Vec<String>) -> anyhow::Result<String> {
    let mut url = None;
    for argument in arguments {
        if argument.starts_with('-') {
            bail!("unknown argument `{argument}`; try `yapfast-plugins --help`");
        }
        if let Some(first) = &url {
            bail!("`fetch` takes one URL, not `{first}` and `{argument}`");
        }
        url = Some(argument);
    }
    url.context("`fetch` needs the URL of a plugin's repository")
}

/// Writes the one object the parent reads, on one line, and flushes it: the
/// process may exit before a buffer would have gone out.
fn print(report: &Report) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, report).context("the report isn't writable as JSON")?;
    writeln!(out).context("the report couldn't be written")?;
    out.flush().context("the report couldn't be written")
}

/// A directory of work that is removed when it goes out of scope.
///
/// `tempfile` isn't a dependency of this crate and the name only has to be
/// unique within one process, so the process id and a counter are enough.
#[derive(Debug)]
struct Scratch {
    path: PathBuf,
    /// Set once the directory is the parent's to delete, so it must be left
    /// alone here.
    keep: bool,
}

impl Scratch {
    fn new(prefix: &str) -> std::io::Result<Scratch> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        for _ in 0..64 {
            let path = parent.join(format!(
                "{prefix}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Scratch { path, keep: false });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::other(
            "a temporary directory couldn't be made",
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// Hands the directory over: the parent has been told where it is, and
    /// deletes it after copying what it holds.
    fn keep(self) {
        std::mem::forget(self);
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caps_are_the_ones_the_parent_refuses_at() {
        assert_eq!(MODULE_CAP.bytes, limits::MAX_WASM_BYTES);
        assert_eq!(ICON_CAP.bytes, 64 << 10);
        assert_eq!(ICON_CAP.bytes, limits::MAX_ICON_BYTES);
    }

    #[test]
    fn a_plugin_report_names_its_fields() {
        let report = Report::Plugin {
            source: "https://example.com/ada/word-filter".into(),
            path: String::new(),
            reference: "main".into(),
            rev: "051b765b".into(),
            manifest: "{}".into(),
            wasm: Some("/tmp/x/plugin.wasm".into()),
            icon: None,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&report).expect("a report")).expect("JSON");
        assert_eq!(json["kind"], "plugin");
        assert_eq!(json["path"], "");
        assert_eq!(json["reference"], "main");
        assert_eq!(json["icon"], serde_json::Value::Null);
        let mut fields: Vec<&str> = json
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "icon",
                "kind",
                "manifest",
                "path",
                "reference",
                "rev",
                "source",
                "wasm"
            ]
        );
    }

    #[test]
    fn an_index_report_is_a_list_of_folders() {
        let report = Report::Index {
            source: "https://example.com/ada/plugins".into(),
            paths: vec!["plugins/word-filter".into(), "plugins/quiet-hours".into()],
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&report).expect("a report")).expect("JSON");
        assert_eq!(json["kind"], "index");
        assert_eq!(json["paths"][1], "plugins/quiet-hours");
    }

    #[test]
    fn an_error_report_is_a_sentence() {
        let report = Report::Error {
            error: "That repository has no yapfast-plugin.json.".into(),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&report).expect("a report")).expect("JSON");
        assert_eq!(json["kind"], "error");
        assert_eq!(json["error"], "That repository has no yapfast-plugin.json.");
    }

    #[test]
    fn one_url_and_no_flags() {
        assert_eq!(
            url(vec!["https://example.com/a/b".into()]).expect("a URL"),
            "https://example.com/a/b"
        );
        assert!(url(vec![]).is_err());
        assert!(url(vec!["a".into(), "b".into()]).is_err());
        assert!(url(vec!["--store".into()]).is_err());
    }

    #[test]
    fn a_scratch_directory_goes_when_it_is_dropped() {
        let scratch = Scratch::new(".yapfast-test").expect("a scratch directory");
        let path = scratch.path().to_path_buf();
        assert!(path.is_dir());
        drop(scratch);
        assert!(!path.exists(), "nothing is left behind");
    }
}
