//! The marketplace's registry, as a repository's CI and the helper read it.
//!
//! `PLAN.md` §8.6: a registry is a git repository holding one entry file per
//! plugin, its icons, and an index built by CI. Nothing about it is served:
//! browsing fetches the repository and reads files.
//!
//! ```text
//! yapfast-plugin-registry/
//!   plugins/com.example.word-filter.json   repo, path, rev, sha256, category, tags
//!   icons/com.example.word-filter.webp     128×128, ≤ 24 KiB
//!   index.json                             built by CI
//! ```
//!
//! Two subcommands use this module:
//!
//! * `catalog <URL>` reads a registry for the app to browse: the index, every
//!   entry it lists, and every icon, which it writes to a temporary directory
//!   for the parent to copy and delete. It checks nothing beyond what it must
//!   to know which files to read, because the app refuses a broken entry on
//!   its own and the rest of the catalog still lists.
//! * `verify-entry <FILE>` is what a registry's CI runs on a pull request, and
//!   what a maintainer runs before reading the source: it checks the entry's
//!   schema, its license and source link, fetches the commit it pins, hashes
//!   the module and compares, validates the manifest, refuses a bare `*` host,
//!   and measures the icon. Without it, *Reviewed* would mean nothing.
//!
//! The schema is mirrored in `yapfast-core`'s `plugins::marketplace`, which is
//! what the app reads a catalog with; a change here is a change there.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use yapfast_plugin_api::capability::HostPattern;
use yapfast_plugin_api::manifest::PluginManifest;
use yapfast_plugin_api::{API_VERSION, PluginId};

use super::git::{Cap, Snapshot};
use super::{Fetched, MANIFEST_CAP, MANIFEST_FILE, Report, Scratch};

/// The file at the registry's root that says what it lists.
pub const INDEX_FILE: &str = "index.json";
/// The folder the entry files sit in, one `<id>.json` each.
pub const ENTRY_DIR: &str = "plugins";
/// The folder the icons sit in, one `<id>.webp` each.
pub const ICON_DIR: &str = "icons";

/// An icon is 128 by 128, at most 24 KiB.
pub const ICON_SIDE: u32 = 128;
pub const ICON_BYTES: u64 = 24 << 10;

/// An index and an entry are metadata: kilobytes, like a manifest.
const INDEX_CAP: Cap = Cap {
    bytes: 1 << 20,
    label: "a registry index is at most 1 MiB",
};
const ENTRY_CAP: Cap = Cap {
    bytes: 1 << 20,
    label: "a registry entry is at most 1 MiB",
};
/// An icon is measured at the size the app draws it, and no larger.
const ICON_CAP: Cap = Cap {
    bytes: ICON_BYTES,
    label: "a registry icon is at most 24 KiB",
};

/// What `index.json` holds.
#[derive(Debug, Deserialize)]
pub struct Index {
    pub api: u32,
    /// The plugin ids this registry lists, in the order the tab shows them.
    pub plugins: Vec<String>,
    /// What it took down, and whether running it is what it was taken down
    /// for.
    #[serde(default)]
    pub delisted: Vec<Delisted>,
}

#[derive(Debug, Deserialize)]
pub struct Delisted {
    pub id: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub security: bool,
}

/// One entry, as `plugins/<id>.json` writes it.
#[derive(Debug, Deserialize)]
pub struct Entry {
    #[serde(default)]
    pub api: Option<u64>,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub repo: String,
    #[serde(default)]
    pub path: String,
    pub rev: String,
    pub sha256: String,
    /// What the row is filed under and searched by. The checker has no rule
    /// about either: a listing may be filed under a new category.
    #[serde(default)]
    #[allow(dead_code)]
    pub category: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub tags: Vec<String>,
    pub source: String,
    pub license: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub permissions: Permissions,
}

#[derive(Debug, Default, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub network: Vec<String>,
}

impl Entry {
    /// Reads one entry file and checks everything CI can check without
    /// fetching the plugin: its schema, its pin, its license, the source link
    /// a maintainer reads, the hosts it may reach, and which icon file is
    /// its own.
    pub fn parse(text: &str, file: &str) -> anyhow::Result<Entry> {
        let entry: Entry = serde_json::from_str(text)
            .with_context(|| format!("`{file}` isn't an entry Yapfast reads"))?;
        entry.check(file)?;
        Ok(entry)
    }

    fn check(&self, file: &str) -> anyhow::Result<()> {
        match self.api {
            Some(api) if api == u64::from(API_VERSION) => {}
            Some(api) => bail!(
                "`{file}` is for plugin API {api}, and this Yapfast speaks API {API_VERSION}"
            ),
            None => bail!("`{file}` doesn't name its plugin API"),
        }
        if PluginId::parse(&self.id).is_err() {
            bail!("`{}` isn't a plugin id", self.id);
        }
        if id_of(file) != Some(self.id.as_str()) {
            bail!(
                "`{file}` holds the entry for `{}`, and its file name says another plugin",
                self.id
            );
        }
        if self.name.trim().is_empty() || self.version.trim().is_empty() {
            bail!("`{file}` has no name or no version");
        }
        if !fetchable(&self.repo) {
            bail!(
                "`{}` isn't a repository Yapfast may fetch; use the https:// address",
                self.repo
            );
        }
        if !self.path.is_empty() && !inside(&self.path) {
            bail!(
                "`{}` isn't a folder inside the repository; it can't be absolute or climb out",
                self.path
            );
        }
        if !is_hex(&self.rev, 40) {
            bail!(
                "`{}` isn't a commit: an entry pins the full 40-character commit, not a branch",
                self.rev
            );
        }
        if !is_hex(&self.sha256, 64) {
            bail!(
                "`{}` isn't the SHA-256 of the module the entry pins",
                self.sha256
            );
        }
        if !osi_approved(&self.license) {
            bail!(
                "`{}` isn't an OSI-approved license, and a listed plugin needs one",
                self.license
            );
        }
        // Review read the code at `source`: if it isn't the repository the
        // plugin installs from, review read something else.
        if !names(&self.source, &self.repo) {
            bail!(
                "its `source` is `{}`, and `repo` is `{}`: a listed plugin's source is the code \
                 review read, so it is that repository",
                self.source,
                self.repo
            );
        }
        for host in &self.permissions.network {
            if HostPattern::parse(host).is_err() {
                bail!(
                    "`{host}` isn't a host a plugin may reach; a bare `*` is refused, and a \
                     wildcard is written `*.example.com`"
                );
            }
        }
        if let Some(icon) = &self.icon
            && icon != &format!("{ICON_DIR}/{}.webp", self.id)
        {
            bail!(
                "its icon is `{icon}`, and the registry holds its own: `{ICON_DIR}/{}.webp`",
                self.id
            );
        }
        Ok(())
    }

    /// The address `fetch` reads for this entry: the commit it pins, and the
    /// folder inside its repository. The app builds it; this side pins a fetch
    /// by setting the two fields, so a `file://` source works too.
    #[allow(dead_code)]
    pub fn url(&self) -> String {
        let mut url = String::with_capacity(self.repo.len() + self.rev.len() + 16);
        url.push_str(&self.repo);
        url.push('@');
        url.push_str(&self.rev);
        if !self.path.is_empty() {
            url.push_str("#path=");
            url.push_str(&self.path);
        }
        url
    }
}

/// A registry, read for browsing.
pub struct Catalog {
    pub index: String,
    /// Each entry file, by its path in the registry, as text.
    pub entries: Vec<(String, String)>,
    /// Each icon, by its path in the registry, and where it was written for
    /// the parent to copy.
    pub icons: Vec<(String, String, u64)>,
}

/// Reads a registry's index, its entries, and its icons.
pub fn catalog(snapshot: &Snapshot, out: &Scratch) -> anyhow::Result<Catalog> {
    let index = snapshot
        .file(Path::new(INDEX_FILE), INDEX_CAP)?
        .with_context(|| format!("this repository has no `{INDEX_FILE}` at its root"))?;
    let index = String::from_utf8(index)
        .map_err(|_| anyhow::anyhow!("`{INDEX_FILE}` isn't UTF-8 text"))?;
    let listed: Index = serde_json::from_str(&index)
        .with_context(|| format!("`{INDEX_FILE}` isn't an index Yapfast reads"))?;
    if listed.api != API_VERSION {
        bail!(
            "`{INDEX_FILE}` is for plugin API {}, and this Yapfast speaks {API_VERSION}",
            listed.api
        );
    }
    if listed.plugins.is_empty() {
        bail!("`{INDEX_FILE}` lists no plugin");
    }
    // What was taken down is checked here too: a person who has one of these
    // is told why, so an id that isn't one, or a security delisting with no
    // reason, is a registry bug found before anyone is told.
    for entry in &listed.delisted {
        if PluginId::parse(&entry.id).is_err() {
            bail!("`{INDEX_FILE}` delists `{}`, which isn't a plugin id", entry.id);
        }
        if listed.plugins.contains(&entry.id) {
            bail!("`{INDEX_FILE}` both lists and delists `{}`", entry.id);
        }
        if entry.security && entry.reason.trim().is_empty() {
            bail!(
                "`{INDEX_FILE}` delists `{}` for a security reason and doesn't say which",
                entry.id
            );
        }
    }
    let mut entries: Vec<(String, String)> = Vec::with_capacity(listed.plugins.len());
    let mut icons: Vec<(String, String, u64)> = Vec::with_capacity(listed.plugins.len());
    for id in &listed.plugins {
        // The app checks each entry; here the file only has to be read, so a
        // broken entry is one row the app refuses rather than a catalog that
        // doesn't load.
        let path = format!("{ENTRY_DIR}/{id}.json");
        let Some(text) = snapshot.file(Path::new(&path), ENTRY_CAP).ok().flatten() else {
            continue;
        };
        let text = match String::from_utf8(text) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let icon = serde_json::from_str::<Entry>(&text)
            .ok()
            .and_then(|entry| entry.icon);
        entries.push((path, text));
        if let Some(named) = icon {
            // A file the cap refuses, or one that isn't there, leaves this
            // entry without its picture; the app refuses the row on its own.
            let Some(bytes) = snapshot.file(Path::new(&named), ICON_CAP).ok().flatten() else {
                continue;
            };
            let size = bytes.len() as u64;
            let target = out.path().join(&named);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, &bytes)?;
            let file = target
                .to_str()
                .with_context(|| format!("`{}` isn't a path the parent can read", target.display()))?
                .to_owned();
            icons.push((named, file, size));
        }
    }
    Ok(Catalog {
        index,
        entries,
        icons,
    })
}

/// What a maintainer's CI gets from checking one entry.
pub struct Verified {
    pub id: String,
    pub rev: String,
    pub sha256: String,
}

/// Checks one entry file, and the plugin it pins.
///
/// This is the review's half that a machine can do: the schema, the license
/// and source link a listing promises, the commit the entry pins, the hash of
/// the module at that commit, the manifest's own validity and hosts, and the
/// icon's size. What is left for the person merging the pull request is
/// reading the source, which is what *Reviewed* then means.
pub fn verify_entry(file: &Path) -> anyhow::Result<Verified> {
    let text = std::fs::read_to_string(file)
        .with_context(|| format!("could not read {}", file.display()))?;
    let shown = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.display().to_string());
    let entry = Entry::parse(&text, &shown)?;

    // The commit the entry pins, in the folder it names: the same fetch an
    // install does, so what CI checks is what a person gets. The pin is set on
    // the source rather than written after an `@`, because a `file://` source
    // takes no ref — a directory's own name may hold an `@`.
    let work = Scratch::new(".yapfast-verify")?;
    let mut source = super::url::Source::parse(&entry.repo)?;
    source.reference = Some(entry.rev.clone());
    source.path = (!entry.path.is_empty()).then(|| entry.path.clone());
    let snapshot = Snapshot::open(&source, work.path())?;
    if !snapshot.commit.eq_ignore_ascii_case(&entry.rev) {
        bail!(
            "the entry pins `{}`, and its repository gave `{}`",
            entry.rev,
            snapshot.commit
        );
    }
    let manifest_text = snapshot
        .file(Path::new(MANIFEST_FILE), MANIFEST_CAP)?
        .with_context(|| {
            format!(
                "`{}` holds no `{MANIFEST_FILE}` at this commit",
                entry.repo
            )
        })?;
    let manifest_text = String::from_utf8(manifest_text)
        .map_err(|_| anyhow::anyhow!("`{MANIFEST_FILE}` isn't UTF-8 text"))?;
    // Parsing the manifest is what checks its schema, its limits and its
    // permission hosts: a bare `*` never gets past `HostPattern::parse`.
    let manifest = PluginManifest::parse(&manifest_text)
        .map_err(|error| anyhow::anyhow!("its `{MANIFEST_FILE}` can't be used: {error}"))?;
    if manifest.id != PluginId::parse(&entry.id)? {
        bail!(
            "the entry is for `{}`, and the manifest declares `{}`",
            entry.id,
            manifest.id
        );
    }
    // Browsing draws the row from the entry, so the two have to agree about
    // what the plugin is and what it reaches.
    if manifest.name != entry.name || manifest.version.to_string() != entry.version {
        bail!(
            "the entry says `{}` {} and the manifest says `{}` {}",
            entry.name,
            entry.version,
            manifest.name,
            manifest.version
        );
    }
    let capabilities: Vec<String> = manifest
        .permissions
        .capabilities
        .iter()
        .map(ToString::to_string)
        .collect();
    if capabilities != entry.permissions.capabilities {
        bail!("the entry's capabilities aren't the manifest's");
    }
    let network: Vec<String> = manifest
        .permissions
        .network
        .iter()
        .map(ToString::to_string)
        .collect();
    if network != entry.permissions.network {
        bail!("the entry's hosts aren't the manifest's");
    }
    if manifest.license.as_deref() != Some(entry.license.as_str()) {
        bail!(
            "the entry's license is `{}` and the manifest's is `{}`",
            entry.license,
            manifest.license.as_deref().unwrap_or("none")
        );
    }
    // The row also shows what the manifest says about itself, so the copied
    // words are checked as those the plugin would show.
    if manifest.description.as_deref().unwrap_or_default() != entry.description {
        bail!("the entry's description isn't the manifest's");
    }
    if manifest.authors != entry.authors {
        bail!("the entry's authors aren't the manifest's");
    }

    // The module the entry pins, hashed as the parent hashes what it installs.
    let Some(named) = &manifest.wasm else {
        bail!("a listed plugin has a module, and this manifest names none");
    };
    let relative = manifest
        .resolve(named)
        .with_context(|| format!("the manifest names `{}`, which isn't in the plugin", named.display()))?;
    let module = snapshot
        .file(&relative, super::MODULE_CAP)?
        .with_context(|| format!("the manifest names `{}`, which isn't in the plugin", named.display()))?;
    let sha256 = hex(Sha256::digest(&module).as_slice());
    if !sha256.eq_ignore_ascii_case(&entry.sha256) {
        bail!(
            "the entry pins `{}` and the module at that commit is `{sha256}`",
            entry.sha256
        );
    }
    // The imports are checked the same way an install checks them.
    super::imports::check(&module, &named.display().to_string(), manifest.wasi)?;

    if let Some(icon) = &entry.icon {
        // The icon is the registry's own file, beside the entry's folder: CI
        // runs this inside a checkout of the registry, and a missing picture
        // is a row with no face.
        let root = file
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."));
        let picture = root.join(icon);
        let bytes = std::fs::read(&picture)
            .with_context(|| format!("`{icon}` is the entry's icon and isn't in the registry"))?;
        if bytes.len() as u64 > ICON_BYTES {
            bail!(
                "`{icon}` is {} bytes, and a registry's icons are at most {ICON_BYTES}",
                bytes.len()
            );
        }
        match webp_size(&bytes) {
            Some((ICON_SIDE, ICON_SIDE)) => {}
            other => bail!(
                "`{icon}` is {other:?} and a registry's icons are {ICON_SIDE} by {ICON_SIDE} WebP \
                 pictures"
            ),
        }
    }
    Ok(Verified {
        id: entry.id,
        rev: snapshot.commit,
        sha256,
    })
}

/// `verify-entry`, and the one JSON object it prints.
pub fn run_verify(arguments: Vec<String>) -> i32 {
    let mut file: Option<PathBuf> = None;
    for argument in arguments {
        if argument.starts_with('-') {
            eprintln!("yapfast-plugins: unknown argument `{argument}`");
            return 1;
        }
        if let Some(first) = &file {
            eprintln!(
                "yapfast-plugins: `verify-entry` takes one file, not `{}` and `{argument}`",
                first.display()
            );
            return 1;
        }
        file = Some(PathBuf::from(argument));
    }
    let Some(file) = file else {
        println!(
            "{}",
            json!({"kind": "error", "error": "`verify-entry` needs the path of an entry file"})
        );
        return 1;
    };
    match verify_entry(&file) {
        Ok(verified) => {
            println!(
                "{}",
                json!({
                    "kind": "verified",
                    "id": verified.id,
                    "rev": verified.rev,
                    "sha256": verified.sha256,
                })
            );
            0
        }
        Err(error) => {
            println!("{}", json!({"kind": "error", "error": format!("{error:#}")}));
            1
        }
    }
}

/// `catalog`: reads a registry for the app to browse.
pub fn run_catalog(arguments: Vec<String>) -> i32 {
    let mut url: Option<String> = None;
    for argument in arguments {
        if argument.starts_with('-') {
            eprintln!("yapfast-plugins: unknown argument `{argument}`");
            return 1;
        }
        if let Some(first) = &url {
            eprintln!("yapfast-plugins: `catalog` takes one URL, not `{first}` and `{argument}`");
            return 1;
        }
        url = Some(argument);
    }
    let Some(url) = url else {
        println!(
            "{}",
            json!({"kind": "error", "error": "`catalog` needs the address of a registry"})
        );
        return 1;
    };
    match read_catalog(&url) {
        Ok(fetched) => {
            if let Err(error) = super::print(&fetched.report) {
                eprintln!("yapfast-plugins: the report couldn't be written: {error}");
                return 1;
            }
            fetched.keep();
            0
        }
        Err(error) => {
            println!(
                "{}",
                json!({"kind": "error", "error": format!("{error:#}")})
            );
            1
        }
    }
}

/// Fetches a registry and reads it, writing its icons where the parent copies
/// them from.
fn read_catalog(url: &str) -> anyhow::Result<Fetched> {
    let source = super::url::Source::parse(url)?;
    if source.path.is_some() {
        bail!("a registry is a repository, not a folder in one: paste its own address");
    }
    let work = Scratch::new(".yapfast-fetch")?;
    let snapshot = Snapshot::open(&source, work.path())?;
    let handoff = Scratch::new(".yapfast-plugin")?;
    let catalog = catalog(&snapshot, &handoff)?;
    Ok(Fetched {
        report: Report::Catalog {
            source: source.repo.clone(),
            rev: snapshot.commit.clone(),
            index: catalog.index,
            entries: catalog
                .entries
                .into_iter()
                .map(|(path, text)| super::EntryFile { path, text })
                .collect(),
            icons: catalog
                .icons
                .into_iter()
                .map(|(path, file, bytes)| super::IconFile { path, file, bytes })
                .collect(),
            scratch: handoff.path().to_str().map(str::to_owned),
        },
        handoff: Some(handoff),
    })
}

/// The id a file names: `plugins/com.example.word-filter.json` is
/// `com.example.word-filter`.
fn id_of(file: &str) -> Option<&str> {
    let name = file.rsplit('/').next()?;
    let id = name.strip_suffix(".json")?;
    (!id.is_empty()).then_some(id)
}

fn fetchable(url: &str) -> bool {
    (url.starts_with("https://") && url.len() > "https://".len())
        || (url.starts_with("file://") && url.len() > "file://".len())
}

fn names(source: &str, repo: &str) -> bool {
    let rest = source.strip_prefix(repo);
    match rest {
        Some(rest) => rest.is_empty() || rest.starts_with('#') || rest.starts_with('@'),
        None => false,
    }
}

fn inside(path: &str) -> bool {
    !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != ".." && !part.contains('\\'))
}

fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(byte & 0xf), 16).unwrap_or('0'));
    }
    out
}

/// The licenses a listing may name: OSI-approved ids, and the `-only`,
/// `-or-later` and `+` forms SPDX writes them in.
const OSI: [&str; 24] = [
    "AFL-3.0",
    "AGPL-3.0",
    "Apache-2.0",
    "Artistic-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "BSL-1.0",
    "CDDL-1.0",
    "EPL-1.0",
    "EPL-2.0",
    "EUPL-1.2",
    "GPL-2.0",
    "GPL-3.0",
    "ISC",
    "LGPL-2.1",
    "LGPL-3.0",
    "MIT",
    "MPL-2.0",
    "MS-PL",
    "NCSA",
    "OFL-1.1",
    "OSL-3.0",
    "PostgreSQL",
    "Zlib",
];

fn osi_approved(expression: &str) -> bool {
    let mut ids = 0;
    for part in expression.split([' ', '(', ')']) {
        let id = part.trim();
        if id.is_empty() || id == "OR" || id == "AND" || id == "WITH" {
            continue;
        }
        ids += 1;
        let base = id
            .strip_suffix("-or-later")
            .or_else(|| id.strip_suffix("-only"))
            .or_else(|| id.strip_suffix('+'))
            .unwrap_or(id);
        if !OSI.iter().any(|known| known.eq_ignore_ascii_case(base)) {
            return false;
        }
    }
    ids > 0
}

/// The width and height a WebP's first chunk gives, read without decoding the
/// picture: `VP8X` for an extended file, `VP8 ` for a lossy one, `VP8L` for a
/// lossless one.
pub fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 30 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    match &bytes[12..16] {
        b"VP8X" => Some((
            u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]) + 1,
            u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]) + 1,
        )),
        b"VP8 " if bytes[23..26] == [0x9d, 0x01, 0x2a] => Some((
            u32::from(u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3fff),
            u32::from(u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3fff),
        )),
        b"VP8L" if bytes[20] == 0x2f => {
            let bits = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_text(id: &str) -> String {
        format!(
            r#"{{
                "api": 1,
                "id": "{id}",
                "name": "Word Filter",
                "version": "1.4.0",
                "description": "Hides words.",
                "authors": ["Ada"],
                "repo": "https://codeberg.org/ada/word-filter",
                "path": "",
                "rev": "{}",
                "sha256": "{}",
                "category": "Chat",
                "tags": [],
                "source": "https://codeberg.org/ada/word-filter",
                "license": "MIT",
                "icon": "icons/{id}.webp",
                "permissions": {{ "capabilities": ["messages.read"], "network": ["api.example.com"] }}
            }}"#,
            "a".repeat(40),
            "0".repeat(64)
        )
    }

    #[test]
    fn an_entry_is_checked_for_what_a_listing_promises() {
        let file = "plugins/com.example.word-filter.json";
        assert!(Entry::parse(&entry_text("com.example.word-filter"), file).is_ok());

        // A license a listing may not carry.
        let unlicensed = entry_text("com.example.word-filter").replace("\"MIT\"", "\"WTFPL\"");
        let error = Entry::parse(&unlicensed, file).expect_err("not OSI-approved");
        assert!(error.to_string().contains("OSI-approved"), "{error}");

        // A source link review didn't read.
        let elsewhere = entry_text("com.example.word-filter").replace(
            "\"source\": \"https://codeberg.org/ada/word-filter\"",
            "\"source\": \"https://example.net/other\"",
        );
        let error = Entry::parse(&elsewhere, file).expect_err("not that repository");
        assert!(error.to_string().contains("review read"), "{error}");

        // A bare `*` is every host there is.
        let every = entry_text("com.example.word-filter")
            .replace("\"network\": [\"api.example.com\"]", "\"network\": [\"*\"]");
        let error = Entry::parse(&every, file).expect_err("a bare `*` is refused");
        assert!(error.to_string().contains("bare `*`"), "{error}");

        // A branch instead of a commit.
        let branch = entry_text("com.example.word-filter").replace(&"a".repeat(40), "main");
        let error = Entry::parse(&branch, file).expect_err("not a commit");
        assert!(error.to_string().contains("40-character"), "{error}");
    }

    #[test]
    fn an_entry_file_names_the_plugin_it_holds() {
        let error = Entry::parse(
            &entry_text("com.example.word-filter"),
            "plugins/com.example.other.json",
        )
        .expect_err("the file name says another plugin");
        assert!(error.to_string().contains("file name"), "{error}");
    }

    #[test]
    fn only_osi_approved_licenses_are_listed() {
        assert!(osi_approved("MIT"));
        assert!(osi_approved("GPL-3.0-or-later"));
        assert!(osi_approved("MIT OR Apache-2.0"));
        assert!(!osi_approved(""));
        assert!(!osi_approved("WTFPL"));
    }

    /// A WebP's header gives its size, which is how an icon is measured
    /// without being decoded.
    #[test]
    fn an_icons_size_is_read_from_its_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&22u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8X");
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&[127, 0, 0]);
        bytes.extend_from_slice(&[127, 0, 0]);
        assert_eq!(webp_size(&bytes), Some((128, 128)));
        assert_eq!(webp_size(b"\x89PNG\r\n\x1a\n"), None);
    }

    #[test]
    fn an_entry_installs_the_commit_it_pins() {
        let entry = Entry::parse(
            &entry_text("com.example.word-filter")
                .replace("\"path\": \"\"", "\"path\": \"plugins/word-filter\"")
                .replace(
                    "\"source\": \"https://codeberg.org/ada/word-filter\"",
                    "\"source\": \"https://codeberg.org/ada/word-filter#path=plugins/word-filter\"",
                ),
            "plugins/com.example.word-filter.json",
        )
        .expect("a folder inside a repository");
        assert_eq!(
            entry.url(),
            format!(
                "https://codeberg.org/ada/word-filter@{}#path=plugins/word-filter",
                "a".repeat(40)
            )
        );
    }
}
