//! The marketplace: the registry repository, the entries it holds, and what
//! browsing it gives the page.
//!
//! The registry is a git repository and nothing else — no server, no API
//! (`PLAN.md` §8.6):
//!
//! ```text
//! yapfast-plugin-registry/
//!   plugins/com.example.word-filter.json   an entry, one per plugin
//!   icons/com.example.word-filter.webp     128×128, at most 24 KiB
//!   index.json                             what the repository lists
//! ```
//!
//! Browsing fetches that repository through the same machinery a URL install
//! uses — `yapfast-plugins catalog`, which runs for seconds in a process of its
//! own and prints what it read. Everything after the fetch happens here, on
//! plain JSON and bytes, so what a registry is allowed to contain is checked
//! where it can be tested without a repository ([`Catalog::read`]).
//!
//! **What a listing promises.** An entry pins one commit and the SHA-256 of the
//! module that commit holds, and names the public `source` a maintainer read
//! before merging it. The listing says *Reviewed* because a person read that
//! source and its permissions; [`Catalog::verify`] compares what an install
//! fetched against the entry, so a repository that changed under its pin is
//! refused rather than run.
//!
//! **Icons are the registry's.** A registry holds its own icons, so browsing
//! contacts one repository and no plugin author's image host, and no address is
//! leaked to them. The bytes are kept here, and the page asks for them by
//! [`icon_url`], which is what makes a picture decode only when its row is on
//! screen (see `application::plugin_page`). Nothing here is kept once the page
//! closes: the catalog is dropped, the icons with it.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Deserialize;
use yapfast_plugin_api::capability::HostPattern;
use yapfast_plugin_api::{API_VERSION, PluginId};

use super::installing::Fetched;
use super::page::{Installed, Listing, Origin, Update};

/// The file at the repository's root that says what it lists.
pub const INDEX: &str = "index.json";
/// The folder the per-plugin entry files sit in, one `<id>.json` each.
pub const ENTRIES: &str = "plugins";
/// The folder the icons sit in, one `<id>.webp` each.
pub const ICONS: &str = "icons";

/// An icon is 128 by 128 (`PLAN.md` §8.6).
pub const ICON_SIDE: u32 = 128;
/// An icon is at most 24 KiB (`PLAN.md` §8.6).
pub const ICON_BYTES: usize = 24 << 10;

/// The scheme a listing's icon is asked for under: `plugin-icon://<id>`.
pub const ICON_SCHEME: &str = "plugin-icon://";

/// The Yapfast organization's registry.
pub const REGISTRY: &str = "https://codeberg.org/yapfast/plugin-registry";

/// The variable that points browsing at another registry, which is how a
/// registry is tried before it is published and how the tests reach a
/// repository on this disk.
pub const REGISTRY_VARIABLE: &str = "YAPFAST_PLUGIN_REGISTRY";

/// The registry browsing reads: the published one, or the one this build was
/// pointed at.
pub fn registry() -> String {
    std::env::var(REGISTRY_VARIABLE)
        .ok()
        .map(|named| named.trim().to_owned())
        .filter(|named| !named.is_empty())
        .unwrap_or_else(|| REGISTRY.to_owned())
}

/// The address the page asks for an entry's icon by.
pub fn icon_url(id: &str) -> String {
    format!("{ICON_SCHEME}{id}")
}

/// The entry id an icon's address names, or `None` for an address that isn't
/// one of ours.
pub fn icon_id(url: &str) -> Option<&str> {
    url.strip_prefix(ICON_SCHEME).filter(|id| !id.is_empty())
}

/// One plugin's entry, as `plugins/<id>.json` holds it.
///
/// The fields the listing draws are the ones the plugin's own manifest
/// declares, copied here by the registry's CI so that browsing reads one
/// repository: the plugin's manifest is never fetched to draw a row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub authors: Vec<String>,
    pub version: String,
    /// The repository the plugin is installed from.
    pub repo: String,
    /// The folder in it, empty when its manifest is at the root.
    pub path: String,
    /// The commit the entry pins: a full git object name.
    pub rev: String,
    /// The SHA-256 of the module at that commit, checked again at install.
    pub sha256: String,
    pub category: String,
    pub tags: Vec<String>,
    /// Where a maintainer read the source being listed.
    pub source: String,
    pub license: String,
    /// The icon's path in the registry, `icons/<id>.webp`.
    pub icon: Option<String>,
    pub capabilities: Vec<String>,
    pub network: Vec<String>,
}

impl Entry {
    /// The address that installs exactly this entry: the commit it pins, and
    /// the folder inside its repository.
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

    /// The row the marketplace tab draws. Its icon is asked for by
    /// [`icon_url`], so a picture is decoded only where it is shown.
    pub fn listing(&self, reviewed: bool) -> Listing {
        Listing {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            author: self.authors.join(", "),
            version: self.version.clone(),
            category: self.category.clone(),
            tags: self.tags.clone(),
            icon: self
                .icon
                .as_ref()
                .map(|_| icon_url(&self.id))
                .filter(|_| reviewed),
            reviewed,
            capabilities: self.capabilities.clone(),
            network: self.network.clone(),
        }
    }

    /// What a listing would add to the version a person already has, and the
    /// version it would become; `None` when they have this one.
    pub fn update_over(&self, installed: &Installed) -> Option<Update> {
        if installed.commit.eq_ignore_ascii_case(&self.rev) {
            return None;
        }
        Some(Update {
            version: self.version.clone(),
            adds: self
                .capabilities
                .iter()
                .filter(|capability| !installed.capabilities.contains(capability))
                .cloned()
                .collect(),
        })
    }
}

/// A plugin the registry no longer lists. One taken down for a security reason
/// is a notice for whoever has it (`PLAN.md` §8.6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Delisted {
    pub id: String,
    pub reason: String,
    /// It was taken down because running it can't be trusted, rather than
    /// because its author stopped maintaining it.
    pub security: bool,
}

/// Why an entry, or the index, isn't one Yapfast can list. Each says what is
/// wrong with it, because the person who can fix it is the one who wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The file isn't a JSON object, and why.
    NotJson(String),
    /// It names an API version this Yapfast doesn't speak; `None` when it
    /// names none.
    Api(Option<u64>),
    /// Its `id` isn't the id its file name gives, or isn't a plugin id.
    Id(String),
    /// A field a listing can't be drawn without is missing.
    Missing(&'static str),
    /// A commit that isn't a full git object name.
    Rev(String),
    /// A hash that isn't a SHA-256.
    Sha256(String),
    /// No license, or one an OSI-approved list doesn't know.
    License(String),
    /// No `source` link, or one that doesn't name the repository listing
    /// fetched: review can't have read what nobody links to.
    Source(String),
    /// The repository the plugin comes from isn't one Yapfast may fetch.
    Repository(String),
    /// A folder inside the repository that isn't one.
    Path(String),
    /// A host a plugin may reach that isn't a host.
    Host(String),
    /// An icon that isn't the registry's own, or isn't the size one may be.
    Icon(String),
    /// One plugin listed twice.
    Duplicate(String),
    /// An index that lists no plugin.
    Empty,
}

impl Refusal {
    pub fn text(&self) -> String {
        match self {
            Refusal::NotJson(why) => format!("it isn't JSON Yapfast reads: {why}"),
            Refusal::Api(Some(api)) => format!(
                "it is for plugin API {api}, and this Yapfast speaks API {API_VERSION}"
            ),
            Refusal::Api(None) => "it doesn't name its plugin API".into(),
            Refusal::Id(written) => {
                format!("`{written}` isn't the id its file names, or isn't a plugin id")
            }
            Refusal::Missing(field) => format!("it has no `{field}`"),
            Refusal::Rev(written) => format!(
                "`{written}` isn't a commit: an entry pins the full 40-character commit, not a \
                 branch"
            ),
            Refusal::Sha256(written) => {
                format!("`{written}` isn't the SHA-256 of the module it pins")
            }
            Refusal::License(written) if written.is_empty() => {
                "it has no license, and a listed plugin needs an OSI-approved one".into()
            }
            Refusal::License(written) => format!(
                "`{written}` isn't an OSI-approved license, and a listed plugin needs one"
            ),
            Refusal::Source(written) if written.is_empty() => {
                "it has no `source` link, and review reads the source at that link".into()
            }
            Refusal::Source(written) => format!(
                "its `source` link `{written}` isn't the repository it names in `repo`, so it \
                 isn't the code review read"
            ),
            Refusal::Repository(written) => format!(
                "`{written}` isn't a repository Yapfast may fetch: an https:// address, or a \
                 file:// one for development"
            ),
            Refusal::Path(written) => format!(
                "`{written}` isn't a folder inside the repository: it can't be absolute or climb \
                 out with `..`"
            ),
            Refusal::Host(written) => format!(
                "`{written}` isn't a host a plugin may reach; a bare `*` is refused"
            ),
            Refusal::Icon(written) => format!(
                "its icon `{written}` isn't `icons/<id>.webp`, a 128 by 128 WebP of at most 24 \
                 KiB, in the registry itself"
            ),
            Refusal::Duplicate(written) => format!("`{written}` is listed twice"),
            Refusal::Empty => "it lists no plugin".into(),
        }
    }
}

/// The fields an entry file is read as, before they are checked.
#[derive(Deserialize)]
struct Written {
    #[serde(default)]
    api: Option<u64>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    rev: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    permissions: Permissions,
}

#[derive(Default, Deserialize)]
struct Permissions {
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    network: Vec<String>,
}

impl Entry {
    /// Reads one entry file, `plugins/<id>.json`, and checks everything a
    /// listing promises.
    pub fn parse(text: &str, file: &str) -> Result<Entry, Refusal> {
        let written: Written = serde_json::from_str(text)
            .map_err(|error| Refusal::NotJson(error.to_string()))?;
        match written.api {
            Some(api) if api == u64::from(API_VERSION) => {}
            other => return Err(Refusal::Api(other)),
        }
        let id = written
            .id
            .filter(|id| !id.is_empty())
            .ok_or(Refusal::Missing("id"))?;
        if PluginId::parse(&id).is_err() || id_of(file) != Some(id.as_str()) {
            return Err(Refusal::Id(format!("{id} in {file}")));
        }
        let name = non_empty(written.name).ok_or(Refusal::Missing("name"))?;
        let version = non_empty(written.version).ok_or(Refusal::Missing("version"))?;
        let repo = non_empty(written.repo).ok_or(Refusal::Missing("repo"))?;
        if !fetchable(&repo) {
            return Err(Refusal::Repository(repo));
        }
        let path = written.path.unwrap_or_default();
        if !path.is_empty() && !inside(&path) {
            return Err(Refusal::Path(path));
        }
        let rev = written.rev.unwrap_or_default();
        if !is_hex(&rev, 40) {
            return Err(Refusal::Rev(rev));
        }
        let sha256 = written.sha256.unwrap_or_default();
        if !is_hex(&sha256, 64) {
            return Err(Refusal::Sha256(sha256));
        }
        let license = written.license.unwrap_or_default();
        if !osi_approved(&license) {
            return Err(Refusal::License(license));
        }
        let source = written.source.unwrap_or_default();
        // A maintainer reviewed the code at `source`. If it isn't the
        // repository the plugin is installed from, review read something else.
        if !names(&source, &repo) {
            return Err(Refusal::Source(source));
        }
        for host in &written.permissions.network {
            if HostPattern::parse(host).is_err() {
                return Err(Refusal::Host(host.clone()));
            }
        }
        let icon = written.icon.filter(|icon| !icon.is_empty());
        if let Some(icon) = &icon
            && icon != &format!("{ICONS}/{id}.webp")
        {
            return Err(Refusal::Icon(icon.clone()));
        }
        Ok(Entry {
            id,
            name,
            description: written.description.unwrap_or_default(),
            authors: written.authors,
            version,
            repo,
            path,
            rev: rev.to_ascii_lowercase(),
            sha256: sha256.to_ascii_lowercase(),
            category: written.category.unwrap_or_default(),
            tags: written.tags,
            source,
            license,
            icon,
            capabilities: written.permissions.capabilities,
            network: written.permissions.network,
        })
    }
}

/// The id an entry's file names: `plugins/com.example.word-filter.json` is
/// `com.example.word-filter`.
fn id_of(file: &str) -> Option<&str> {
    let name = file.rsplit('/').next()?;
    let id = name.strip_suffix(".json")?;
    (!id.is_empty()).then_some(id)
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// Whether the address is one Yapfast may fetch: https over the network, or
/// this disk for development (`fetch::parse` says the same).
fn fetchable(url: &str) -> bool {
    (url.starts_with("https://") && url.len() > "https://".len())
        || (url.starts_with("file://") && url.len() > "file://".len())
}

/// Whether the source link names the repository the plugin comes from, which
/// is the code a maintainer read: `repo`, or `repo` with the folder or ref
/// the entry pins.
fn names(source: &str, repo: &str) -> bool {
    let source = source.trim();
    if !fetchable(source) {
        return false;
    }
    let rest = source.strip_prefix(repo).unwrap_or_default();
    source.starts_with(repo)
        && (rest.is_empty() || rest.starts_with('#') || rest.starts_with('@'))
}

/// A folder inside a repository: relative, `/`-separated, never climbing out.
fn inside(path: &str) -> bool {
    !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != ".." && !part.contains('\\'))
}

fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The licenses a listing may name: OSI-approved ids, and the `-only` and
/// `-or-later` forms SPDX writes them in. A registry's CI refuses anything
/// else, so a plugin under another license stays a URL install.
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

/// Whether a license expression names only OSI-approved licenses, as
/// `MIT`, `GPL-3.0-or-later` or `MIT OR Apache-2.0` do.
pub fn osi_approved(expression: &str) -> bool {
    let mut ids = 0;
    for part in expression.split([' ', '(', ')']) {
        let id = part.trim();
        if id.is_empty() || id == "OR" || id == "AND" || id == "WITH" {
            continue;
        }
        ids += 1;
        if !osi_id(id) {
            return false;
        }
    }
    ids > 0
}

fn osi_id(id: &str) -> bool {
    // SPDX writes the GNU licenses as `GPL-3.0-or-later` and the others as
    // they are, so what is left after the suffix is the id an OSI list has.
    let base = id
        .strip_suffix("-or-later")
        .or_else(|| id.strip_suffix("-only"))
        .or_else(|| id.strip_suffix('+'))
        .unwrap_or(id);
    OSI.iter().any(|known| known.eq_ignore_ascii_case(base))
}

/// What `index.json` holds: which plugin ids this registry lists, one entry
/// file each, and which it has taken down.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    pub plugins: Vec<String>,
    pub delisted: Vec<Delisted>,
}

#[derive(Deserialize)]
struct WrittenIndex {
    #[serde(default)]
    api: Option<u64>,
    #[serde(default)]
    plugins: Vec<String>,
    #[serde(default)]
    delisted: Vec<WrittenDelisted>,
}

#[derive(Deserialize)]
struct WrittenDelisted {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    security: bool,
}

impl Index {
    /// Reads `index.json`.
    pub fn parse(text: &str) -> Result<Index, Refusal> {
        let written: WrittenIndex = serde_json::from_str(text)
            .map_err(|error| Refusal::NotJson(error.to_string()))?;
        match written.api {
            Some(api) if api == u64::from(API_VERSION) => {}
            other => return Err(Refusal::Api(other)),
        }
        if written.plugins.is_empty() {
            return Err(Refusal::Empty);
        }
        let mut plugins: Vec<String> = Vec::with_capacity(written.plugins.len());
        for id in written.plugins {
            if PluginId::parse(&id).is_err() {
                return Err(Refusal::Id(id));
            }
            if plugins.contains(&id) {
                return Err(Refusal::Duplicate(id));
            }
            plugins.push(id);
        }
        let mut delisted: Vec<Delisted> = Vec::with_capacity(written.delisted.len());
        for entry in written.delisted {
            let id = entry.id.unwrap_or_default();
            if PluginId::parse(&id).is_err() {
                return Err(Refusal::Id(id));
            }
            if delisted.iter().any(|listed| listed.id == id) {
                return Err(Refusal::Duplicate(id));
            }
            delisted.push(Delisted {
                id,
                reason: entry.reason.unwrap_or_default(),
                security: entry.security,
            });
        }
        Ok(Index { plugins, delisted })
    }
}

/// A registry's listings, its entries, and the icons it holds.
///
/// It exists only while Settings → Plugins is open: [`Catalog`] is dropped when
/// the page closes, the icons' bytes with it.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    /// The repository this came from, as the page shows it.
    pub source: String,
    /// The commit it was read at.
    pub rev: String,
    /// The rows the marketplace tab draws, in the order the index lists them.
    pub listings: Arc<[Listing]>,
    /// What installing and updating read, one per listing.
    pub entries: Vec<Entry>,
    /// What the registry took down.
    pub delisted: Vec<Delisted>,
    /// Each listing's icon, by plugin id, as the registry holds it.
    icons: BTreeMap<String, Arc<[u8]>>,
    /// Entries the registry lists that Yapfast refused, in its own words, for
    /// the log. They aren't shown: a listing is a promise, and a broken one
    /// keeps nothing.
    pub refused: Vec<String>,
}

impl PartialEq for Catalog {
    /// Two catalogs are equal when they are the same read of the same
    /// registry: `Notice` compares what it carries, and nothing compares the
    /// bytes of two fetches.
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && self.rev == other.rev
            && self.listings.len() == other.listings.len()
    }
}

impl Catalog {
    /// Reads a registry's files, checking every entry and every icon.
    ///
    /// `entries` and `icons` are the files the fetch read, by their path in the
    /// registry, and the bytes of each. Nothing here touches the disk or the
    /// network.
    pub fn read(
        source: &str,
        rev: &str,
        index: &str,
        entries: &[(String, String)],
        icons: &[(String, Vec<u8>)],
    ) -> Result<Catalog, Refusal> {
        let index = Index::parse(index)?;
        let mut listings: Vec<Listing> = Vec::with_capacity(index.plugins.len());
        let mut kept: Vec<Entry> = Vec::with_capacity(index.plugins.len());
        let mut bytes: BTreeMap<String, Arc<[u8]>> = BTreeMap::new();
        let mut refused: Vec<String> = Vec::new();
        for id in &index.plugins {
            let file = format!("{ENTRIES}/{id}.json");
            let Some((_, text)) = entries.iter().find(|(path, _)| path == &file) else {
                refused.push(format!("`{file}` is listed and isn't in the registry"));
                continue;
            };
            let entry = match Entry::parse(text, &file) {
                Ok(entry) => entry,
                Err(refusal) => {
                    refused.push(format!("`{file}` isn't listed: {}", refusal.text()));
                    continue;
                }
            };
            if let Some(named) = &entry.icon {
                let Some((_, icon)) = icons.iter().find(|(path, _)| path == named) else {
                    refused.push(format!("`{}` isn't in the registry", named));
                    continue;
                };
                if let Err(refusal) = check_icon(named, icon) {
                    refused.push(format!("`{file}` isn't listed: {}", refusal.text()));
                    continue;
                }
                bytes.insert(entry.id.clone(), Arc::from(icon.as_slice()));
            }
            // A listing is reviewed because a maintainer read its source before
            // merging the entry that pins it (`PLAN.md` §8.6).
            listings.push(entry.listing(true));
            kept.push(entry);
        }
        Ok(Catalog {
            source: source.to_owned(),
            rev: rev.to_owned(),
            listings: Arc::from(listings),
            entries: kept,
            delisted: index.delisted,
            icons: bytes,
            refused,
        })
    }

    /// What installing reads: the entry for `id`.
    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// An entry's icon, for the page to draw: it asks for it by
    /// [`icon_url`], and a row off screen asks for nothing.
    pub fn icon(&self, url: &str) -> Option<Arc<[u8]>> {
        icon_id(url).and_then(|id| self.icons.get(id).cloned())
    }

    /// How many icons are held, which is what closing the page lets go of.
    pub fn icon_count(&self) -> usize {
        self.icons.len()
    }

    /// Checks what an install fetched against the entry that named it: the
    /// commit the registry pinned, and the hash of the module that runs. A
    /// repository that changed under its pin is refused, because the review
    /// read the other one.
    pub fn verify(&self, entry: &Entry, fetched: &Fetched) -> Result<(), String> {
        if !fetched.rev.eq_ignore_ascii_case(&entry.rev) {
            return Err(format!(
                "{} is pinned to commit {} in the registry, and its repository gave {}; nothing \
                 was installed",
                entry.name,
                &entry.rev[..8.min(entry.rev.len())],
                &fetched.rev[..8.min(fetched.rev.len())],
            ));
        }
        let module = fetched.module.as_deref().unwrap_or_default();
        let actual = super::store::sha256(module);
        if !actual.eq_ignore_ascii_case(&entry.sha256) {
            return Err(format!(
                "the module the registry lists for {} isn't the one its repository holds ({} \
                 instead of {}); nothing was installed",
                entry.name,
                &actual[..8.min(actual.len())],
                &entry.sha256[..8.min(entry.sha256.len())],
            ));
        }
        Ok(())
    }

    /// Says of each installed plugin whether the marketplace lists it, which
    /// is what makes its origin The marketplace rather than a git URL, and
    /// whether the registry lists a newer commit.
    pub fn describe(&self, installed: &mut [Installed]) {
        for plugin in installed.iter_mut() {
            let Some(entry) = self.entry(&plugin.id) else {
                continue;
            };
            // The pin is what an install wrote, so a plugin that came from this
            // registry is one whose source and folder the entry names.
            if entry.repo == plugin.source && entry.path == plugin.path {
                plugin.origin = Origin::Marketplace;
            }
            plugin.update = entry.update_over(plugin);
        }
    }

    /// What a person with one of these plugins has to hear about: an entry
    /// taken down because running it can't be trusted.
    pub fn warnings(&self, installed: &[Installed]) -> Vec<String> {
        let mut warnings = Vec::new();
        for delisted in &self.delisted {
            if !delisted.security {
                continue;
            }
            let Some(plugin) = installed.iter().find(|plugin| plugin.id == delisted.id) else {
                continue;
            };
            let reason = if delisted.reason.is_empty() {
                String::new()
            } else {
                format!(": {}", delisted.reason)
            };
            warnings.push(format!(
                "{} was taken down from the marketplace for a security reason{reason}. Remove it \
                 in Settings → Plugins unless you trust this version.",
                plugin.name,
            ));
        }
        warnings
    }
}

/// Checks an icon the registry holds: 128 by 128, a WebP, at most 24 KiB.
fn check_icon(path: &str, bytes: &[u8]) -> Result<(), Refusal> {
    if bytes.len() > ICON_BYTES {
        return Err(Refusal::Icon(path.to_owned()));
    }
    match webp_size(bytes) {
        Some((ICON_SIDE, ICON_SIDE)) => Ok(()),
        _ => Err(Refusal::Icon(path.to_owned())),
    }
}

/// The width and height a WebP's header gives, from its first chunk: `VP8X`
/// for an extended file, `VP8 ` for a lossy one, `VP8L` for a lossless one.
/// Only the header is read, so a listing's icon is measured without being
/// decoded.
pub fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 30 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    match &bytes[12..16] {
        b"VP8X" => Some((
            u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]) + 1,
            u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]) + 1,
        )),
        // The lossy header starts after a three-byte frame tag, with 14 bits
        // of width and of height, each followed by two bits of scaling.
        b"VP8 " if bytes[23..26] == [0x9d, 0x01, 0x2a] => Some((
            u32::from(u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3fff),
            u32::from(u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3fff),
        )),
        // The lossless header packs both, one less than the size, in 14 bits.
        b"VP8L" if bytes[20] == 0x2f => {
            let bits = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        _ => None,
    }
}

/// What the helper prints for `yapfast-plugins catalog`, one JSON object tagged
/// by `kind`; see `yapfast-plugin-host`'s `fetch` module for the other side.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Report {
    Catalog {
        source: String,
        rev: String,
        index: String,
        entries: Vec<FileText>,
        icons: Vec<FileIcon>,
        /// The directory the icons were written to, which is this side's to
        /// delete once they are read.
        scratch: Option<String>,
    },
    Error {
        error: String,
    },
}

#[derive(Deserialize)]
struct FileText {
    path: String,
    text: String,
}

#[derive(Deserialize)]
struct FileIcon {
    path: String,
    file: String,
    #[serde(default)]
    #[allow(dead_code)]
    bytes: u64,
}

/// Fetches a registry and reads it: the helper does the git work, in a process
/// that ends with it, and everything after that is JSON and bytes.
pub fn load(url: &str) -> Result<Catalog, String> {
    let program = super::spawn::program().map_err(|error| format!("{error:#}"))?;
    let output = std::process::Command::new(&program)
        .arg("catalog")
        .arg(url)
        .output()
        .map_err(|error| format!("could not start {}: {error}", program.display()))?;
    let printed = String::from_utf8_lossy(&output.stdout);
    let report: Report = serde_json::from_str(&printed).map_err(|_| {
        if output.status.success() {
            "the plugin helper printed something that isn't a catalog report".to_owned()
        } else {
            "the plugin helper couldn't read the marketplace".to_owned()
        }
    })?;
    let (source, rev, index, entries, icons, scratch) = match report {
        Report::Catalog {
            source,
            rev,
            index,
            entries,
            icons,
            scratch,
        } => (source, rev, index, entries, icons, scratch),
        Report::Error { error } => return Err(error),
    };
    // The icons' bytes are read here and nowhere else: the files were written
    // for this side, and the directory goes with them. An icon over the cap is
    // read too, so that the entry it belongs to is the one refused.
    let mut read: Vec<(String, Vec<u8>)> = Vec::with_capacity(icons.len());
    for icon in &icons {
        let bytes = std::fs::read(&icon.file)
            .map_err(|error| format!("could not read {}: {error}", icon.file))?;
        read.push((icon.path.clone(), bytes));
    }
    if let Some(directory) = scratch {
        let _ = std::fs::remove_dir_all(directory);
    }
    let entries: Vec<(String, String)> = entries
        .into_iter()
        .map(|file| (file.path, file.text))
        .collect();
    Catalog::read(&source, &rev, &index, &entries, &read).map_err(|refusal| refusal.text())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WebP's header for a 128 by 128 picture, which is all a listing's icon
    /// is measured by.
    fn icon_bytes(side: u32) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(30);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&22u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8X");
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&(side - 1).to_le_bytes()[..3]);
        bytes.extend_from_slice(&(side - 1).to_le_bytes()[..3]);
        bytes
    }

    fn entry_text(id: &str) -> String {
        format!(
            r#"{{
                "api": 1,
                "id": "{id}",
                "name": "Word Filter",
                "version": "1.4.0",
                "description": "Hides words you'd rather not read.",
                "authors": ["Ada"],
                "repo": "https://codeberg.org/ada/word-filter",
                "path": "",
                "rev": "051b765bb6abe4fe95d5e4f489f03ad7b137edbc",
                "sha256": "{}",
                "category": "Chat",
                "tags": ["filtering"],
                "source": "https://codeberg.org/ada/word-filter",
                "license": "MIT",
                "icon": "icons/{id}.webp",
                "permissions": {{ "capabilities": ["messages.read"], "network": ["api.example.com"] }}
            }}"#,
            "0".repeat(64)
        )
    }

    fn index_text(ids: &[&str]) -> String {
        let listed: Vec<String> = ids.iter().map(|id| format!("\"{id}\"")).collect();
        format!(
            r#"{{ "api": 1, "plugins": [{}], "delisted": [] }}"#,
            listed.join(", ")
        )
    }

    fn registry(ids: &[&str]) -> Catalog {
        let entries: Vec<(String, String)> = ids
            .iter()
            .map(|id| (format!("{ENTRIES}/{id}.json"), entry_text(id)))
            .collect();
        let icons: Vec<(String, Vec<u8>)> = ids
            .iter()
            .map(|id| (format!("{ICONS}/{id}.webp"), icon_bytes(ICON_SIDE)))
            .collect();
        Catalog::read(
            "https://codeberg.org/yapfast/plugin-registry",
            &"a".repeat(40),
            &index_text(ids),
            &entries,
            &icons,
        )
        .expect("a registry Yapfast reads")
    }

    #[test]
    fn a_registry_reads_as_listings_with_reviewed_entries_and_icons() {
        let catalog = registry(&["com.example.word-filter", "org.example.timestamps"]);
        assert_eq!(catalog.listings.len(), 2, "a row each");
        assert_eq!(catalog.listings[0].id, "com.example.word-filter");
        assert_eq!(catalog.listings[0].name, "Word Filter");
        assert_eq!(catalog.listings[0].author, "Ada");
        assert_eq!(catalog.listings[0].category, "Chat");
        assert_eq!(catalog.listings[0].reach(), "messages, 1 host");
        assert!(catalog.listings[0].reviewed, "a maintainer read it");
        // The icon is asked for by an address of our own, so a picture decodes
        // only where a row shows it.
        assert_eq!(
            catalog.listings[0].icon.as_deref(),
            Some("plugin-icon://com.example.word-filter")
        );
        assert_eq!(catalog.icon_count(), 2);
        assert!(catalog.icon(&icon_url("com.example.word-filter")).is_some());
        assert!(catalog.icon("plugin-icon://nobody").is_none());
        assert!(catalog.refused.is_empty());
        // What installs is the entry's own pin and folder.
        let entry = catalog.entry("com.example.word-filter").expect("an entry");
        assert_eq!(
            entry.url(),
            "https://codeberg.org/ada/word-filter@051b765bb6abe4fe95d5e4f489f03ad7b137edbc"
        );
    }

    /// A registry that lists a plugin whose entry is broken lists the others:
    /// one bad entry is a bad entry, not a bad marketplace.
    #[test]
    fn a_broken_entry_is_refused_and_the_rest_are_listed() {
        let entries = vec![
            (
                format!("{ENTRIES}/com.example.word-filter.json"),
                entry_text("com.example.word-filter"),
            ),
            (
                format!("{ENTRIES}/com.example.broken.json"),
                entry_text("com.example.broken").replace("\"MIT\"", "\"WTFPL\""),
            ),
        ];
        let icons = vec![(format!("{ICONS}/com.example.word-filter.webp"), icon_bytes(ICON_SIDE))];
        let catalog = Catalog::read(
            "https://codeberg.org/yapfast/plugin-registry",
            &"a".repeat(40),
            &index_text(&["com.example.word-filter", "com.example.broken"]),
            &entries,
            &icons,
        )
        .expect("a registry");
        assert_eq!(catalog.listings.len(), 1);
        assert_eq!(catalog.refused.len(), 1);
        assert!(catalog.refused[0].contains("OSI-approved"), "{:?}", catalog.refused);
    }

    /// A listing needs a license and a source link, and the source must be the
    /// code review read.
    #[test]
    fn an_entry_without_a_license_or_a_source_is_refused() {
        let file = format!("{ENTRIES}/com.example.word-filter.json");
        let text = entry_text("com.example.word-filter");

        let unlicensed = text.replace("\"license\": \"MIT\",", "");
        assert_eq!(
            Entry::parse(&unlicensed, &file),
            Err(Refusal::License(String::new()))
        );
        assert!(Refusal::License(String::new()).text().contains("OSI-approved"));

        let unlinked = text.replace(
            "\"source\": \"https://codeberg.org/ada/word-filter\",",
            "",
        );
        assert_eq!(
            Entry::parse(&unlinked, &file),
            Err(Refusal::Source(String::new()))
        );
        assert!(
            Refusal::Source(String::new())
                .text()
                .contains("review reads the source")
        );

        // A link to someone else's repository isn't the code that was listed.
        let elsewhere = text.replace(
            "\"source\": \"https://codeberg.org/ada/word-filter\"",
            "\"source\": \"https://codeberg.org/someone/other\"",
        );
        assert!(matches!(
            Entry::parse(&elsewhere, &file),
            Err(Refusal::Source(_))
        ));
    }

    /// A bare `*` is every host there is, which is what a capability is for.
    #[test]
    fn a_bare_star_host_is_refused() {
        let file = format!("{ENTRIES}/com.example.word-filter.json");
        let text = entry_text("com.example.word-filter").replace(
            "\"network\": [\"api.example.com\"]",
            "\"network\": [\"*\"]",
        );
        assert_eq!(
            Entry::parse(&text, &file),
            Err(Refusal::Host("*".into()))
        );
        assert!(Refusal::Host("*".into()).text().contains("bare `*`"));
        // A wildcard below a domain is a host pattern, and is allowed.
        let allowed = entry_text("com.example.word-filter").replace(
            "\"network\": [\"api.example.com\"]",
            "\"network\": [\"*.example.com\"]",
        );
        assert!(Entry::parse(&allowed, &file).is_ok());
    }

    /// An entry pins a commit and a hash; a branch, or a hash of another shape,
    /// leaves nothing to check an install against.
    #[test]
    fn an_entry_pins_a_commit_and_a_hash() {
        let file = format!("{ENTRIES}/com.example.word-filter.json");
        let text = entry_text("com.example.word-filter");
        let branch = text.replace(
            "051b765bb6abe4fe95d5e4f489f03ad7b137edbc",
            "main",
        );
        assert!(matches!(Entry::parse(&branch, &file), Err(Refusal::Rev(_))));
        let short = text.replace(
            "051b765bb6abe4fe95d5e4f489f03ad7b137edbc",
            "051b765b",
        );
        assert!(matches!(Entry::parse(&short, &file), Err(Refusal::Rev(_))));
        let hash = text.replace(&"0".repeat(64), "not a hash");
        assert!(matches!(
            Entry::parse(&hash, &file),
            Err(Refusal::Sha256(_))
        ));
    }

    /// What an install fetched is checked against the entry: the commit the
    /// registry pinned, and the hash of the module that runs. Either one
    /// changed is refused, because the review read the other.
    #[test]
    fn a_tampered_commit_or_hash_is_refused() {
        let catalog = registry(&["com.example.word-filter"]);
        let entry = catalog.entry("com.example.word-filter").expect("an entry");
        let module = b"\0asm\x01\0\0\0".to_vec();
        let honest = Fetched {
            id: PluginId::parse("com.example.word-filter").expect("an id"),
            source: entry.repo.clone(),
            path: entry.path.clone(),
            reference: entry.rev.clone(),
            rev: entry.rev.clone(),
            manifest: r#"{"api":1,"id":"com.example.word-filter","name":"Word Filter","version":"1.4.0"}"#.into(),
            module: Some(module.clone()),
            icon: None,
        };
        // The registry's hash is of another module.
        let error = catalog.verify(entry, &honest).expect_err("not that module");
        assert!(error.contains("isn't the one its repository holds"), "{error}");
        assert!(error.contains("nothing was installed"), "{error}");

        // A repository that moved under its pin.
        let moved = Fetched {
            rev: "b".repeat(40),
            ..honest.clone()
        };
        let error = catalog.verify(entry, &moved).expect_err("not that commit");
        assert!(error.contains("pinned to commit"), "{error}");

        // The entry that pins what the repository holds is accepted.
        let pinned = Entry {
            sha256: super::super::store::sha256(&module),
            ..entry.clone()
        };
        assert!(catalog.verify(&pinned, &honest).is_ok());
    }

    /// An installed plugin whose pin is behind the registry's has an update,
    /// and one that would gain a capability says so: that one needs consent
    /// again, and the others are one click.
    #[test]
    fn an_installed_plugin_behind_the_registry_has_an_update() {
        let catalog = registry(&["com.example.word-filter"]);
        let entry = catalog.entry("com.example.word-filter").expect("an entry");
        let mut installed = Installed {
            id: entry.id.clone(),
            name: "Word Filter".into(),
            version: "1.3.0".into(),
            origin: Origin::Url,
            source: entry.repo.clone(),
            path: entry.path.clone(),
            commit: "c".repeat(40),
            capabilities: Vec::new(),
            ..Installed::default()
        };
        catalog.describe(std::slice::from_mut(&mut installed));
        assert_eq!(installed.origin, Origin::Marketplace, "the registry lists it");
        let update = installed.update.clone().expect("an update");
        assert_eq!(update.version, "1.4.0");
        assert_eq!(update.adds, ["messages.read"], "it asks for more now");

        // At the pinned commit there is nothing to do.
        installed.commit = entry.rev.clone();
        catalog.describe(std::slice::from_mut(&mut installed));
        assert!(installed.update.is_none());

        // A plugin from a URL nobody lists keeps its origin and has no update.
        let mut stranger = Installed {
            id: "org.example.stranger".into(),
            source: "https://github.com/bea/translate".into(),
            commit: "d".repeat(40),
            ..Installed::default()
        };
        catalog.describe(std::slice::from_mut(&mut stranger));
        assert_eq!(stranger.origin, Origin::Url);
        assert!(stranger.update.is_none());
    }

    /// A plugin taken down for a security reason is a notice for whoever has
    /// it, and one merely delisted is not.
    #[test]
    fn a_security_delisting_warns_only_those_who_have_it() {
        let index = r#"{
            "api": 1,
            "plugins": ["com.example.word-filter"],
            "delisted": [
                { "id": "org.example.leaky", "reason": "it sent your token to a server", "security": true },
                { "id": "org.example.abandoned", "security": false }
            ]
        }"#;
        let catalog = Catalog::read(
            "https://codeberg.org/yapfast/plugin-registry",
            &"a".repeat(40),
            index,
            &[(
                format!("{ENTRIES}/com.example.word-filter.json"),
                entry_text("com.example.word-filter"),
            )],
            &[],
        )
        .expect("a registry");
        let leaky = Installed {
            id: "org.example.leaky".into(),
            name: "Leaky".into(),
            ..Installed::default()
        };
        let warnings = catalog.warnings(&[leaky]);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Leaky"), "{warnings:?}");
        assert!(warnings[0].contains("sent your token"), "{warnings:?}");
        assert!(warnings[0].contains("Remove it"), "it offers removal: {warnings:?}");
        // Nobody has the abandoned one, and the abandoned one isn't security.
        assert!(catalog.warnings(&[]).is_empty());
        assert!(
            catalog
                .warnings(&[Installed {
                    id: "org.example.abandoned".into(),
                    ..Installed::default()
                }])
                .is_empty()
        );
    }

    /// An icon is the registry's own, 128 by 128, at most 24 KiB: anything else
    /// is refused rather than drawn.
    #[test]
    fn an_icon_that_is_not_the_registrys_own_is_refused() {
        assert_eq!(webp_size(&icon_bytes(128)), Some((128, 128)));
        assert_eq!(check_icon("icons/a.webp", &icon_bytes(128)), Ok(()));
        assert!(check_icon("icons/a.webp", &icon_bytes(64)).is_err());
        assert!(check_icon("icons/a.webp", b"\x89PNG\r\n\x1a\n").is_err());
        let too_big = vec![0; ICON_BYTES + 1];
        assert!(check_icon("icons/a.webp", &too_big).is_err());

        // An entry's icon has to be the one file the registry would hold for it.
        let file = format!("{ENTRIES}/com.example.word-filter.json");
        let elsewhere = entry_text("com.example.word-filter").replace(
            "icons/com.example.word-filter.webp",
            "https://example.com/icon.webp",
        );
        assert!(matches!(
            Entry::parse(&elsewhere, &file),
            Err(Refusal::Icon(_))
        ));
    }

    #[test]
    fn an_index_lists_each_plugin_once_and_speaks_this_api() {
        assert_eq!(
            Index::parse(r#"{ "api": 2, "plugins": ["a.b"] }"#),
            Err(Refusal::Api(Some(2)))
        );
        assert_eq!(
            Index::parse(r#"{ "api": 1, "plugins": [] }"#),
            Err(Refusal::Empty)
        );
        assert_eq!(
            Index::parse(r#"{ "api": 1, "plugins": ["com.example.a", "com.example.a"] }"#),
            Err(Refusal::Duplicate("com.example.a".into()))
        );
        assert_eq!(
            Index::parse(r#"{ "api": 1, "plugins": ["not an id"] }"#),
            Err(Refusal::Id("not an id".into()))
        );
        assert!(Index::parse(r#"{ "api": 1, "plugins": ["com.example.a"] }"#).is_ok());
    }

    /// A license expression is OSI-approved only if every license in it is.
    #[test]
    fn only_osi_approved_licenses_are_listed() {
        assert!(osi_approved("MIT"));
        assert!(osi_approved("Apache-2.0"));
        assert!(osi_approved("GPL-3.0-or-later"));
        assert!(osi_approved("LGPL-2.1-only"));
        assert!(osi_approved("MIT OR Apache-2.0"));
        assert!(!osi_approved(""));
        assert!(!osi_approved("WTFPL"));
        assert!(!osi_approved("MIT OR WTFPL"));
        assert!(!osi_approved("Proprietary"));
    }

    #[test]
    fn a_repository_and_its_folder_are_what_a_listing_installs() {
        let file = format!("{ENTRIES}/com.example.word-filter.json");
        let in_a_monorepo = entry_text("com.example.word-filter")
            .replace("\"path\": \"\"", "\"path\": \"plugins/word-filter\"")
            .replace(
                "\"source\": \"https://codeberg.org/ada/word-filter\"",
                "\"source\": \"https://codeberg.org/ada/word-filter#path=plugins/word-filter\"",
            );
        let entry = Entry::parse(&in_a_monorepo, &file).expect("a folder inside a repository");
        assert_eq!(entry.path, "plugins/word-filter");
        assert!(entry.url().ends_with("#path=plugins/word-filter"));

        let climbing = entry_text("com.example.word-filter")
            .replace("\"path\": \"\"", "\"path\": \"../elsewhere\"");
        assert!(matches!(
            Entry::parse(&climbing, &file),
            Err(Refusal::Path(_))
        ));
    }
}
