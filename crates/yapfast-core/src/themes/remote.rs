//! Themes from GitHub, and the marketplace's catalog of them.
//!
//! A theme is installed from one commit of a public GitHub repository: the
//! address a person pastes is read into a [`Repository`], the commit its
//! branch or tag is at is asked for once and becomes the pin, and the archive
//! GitHub serves for that commit is read into the theme's files by
//! [`unpack`], under the same limits a local folder is read with and more of
//! its own. The marketplace lists what [`CATALOG_URL`] names: each theme at a
//! commit, with the [`super::install::digest`] of its files, so what installs
//! is what was listed.
//!
//! Nothing here touches the network. The desktop app fetches the addresses
//! these name, hands the bytes back, and runs the same checks an install from
//! a folder runs before anything is kept.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use super::MANIFEST;
use super::check::{self, Files, LICENCE_FILES, MAX_FONT_BYTES, MAX_IMAGE_BYTES, MAX_THEME_BYTES};
use super::install::digest;

/// Where the marketplace's catalog is read from: one JSON file in a
/// repository of its own, which a theme's author asks to be listed in with a
/// pull request (docs/themes.md).
pub const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/6gah/yapfast-themes-catalog/main/catalog.json";

/// The catalog version this Yapfast reads. A later one is refused whole, as
/// its entries may mean something this build doesn't know.
pub const CATALOG_VERSION: u64 = 1;

/// The largest catalog.
pub const MAX_CATALOG_BYTES: u64 = 1 << 20;

/// The largest archive downloaded for a theme, compressed.
pub const MAX_ARCHIVE_BYTES: u64 = 64 << 20;

/// The most an archive may unpack to, all of it read, so a small download
/// can't unpack without end.
pub const MAX_UNPACKED_BYTES: u64 = 128 << 20;

/// The most entries an archive may hold.
pub const MAX_ARCHIVE_ENTRIES: usize = 4096;

/// The most files a theme from GitHub may name, its cursors' frames and
/// their `@2x` copies counted.
pub const MAX_THEME_FILES: usize = 256;

/// The largest preview picture the marketplace downloads.
pub const MAX_PREVIEW_BYTES: u64 = MAX_IMAGE_BYTES;

/// A public GitHub repository, and the folder and branch, tag or commit of it
/// a theme is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
    pub owner: String,
    pub name: String,
    /// A branch, tag or commit; `None` for the repository's default branch.
    pub reference: Option<String>,
    /// The folder `theme.json` is in, `/`-separated; empty for the root.
    pub path: String,
}

impl Repository {
    /// Reads `https://github.com/<owner>/<name>`, perhaps with `.git` or a
    /// trailing `/`, or `.../tree/<ref>/<folder>` as GitHub's own links to a
    /// folder are. A ref with a `/` in it can't be told from its folder in
    /// such a link, so the first part is the ref.
    pub fn parse(address: &str) -> Result<Self, String> {
        let not_one = || {
            "Themes come from public GitHub repositories, as https://github.com/<owner>/<repository>; that address isn't one".to_owned()
        };
        let rest = address
            .trim()
            .strip_prefix("https://github.com/")
            .ok_or_else(not_one)?;
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let mut parts = rest.trim_end_matches('/').split('/');
        let owner = parts
            .next()
            .filter(|part| name_part(part))
            .ok_or_else(not_one)?;
        let name = parts
            .next()
            .map(|part| part.strip_suffix(".git").unwrap_or(part))
            .filter(|part| name_part(part))
            .ok_or_else(not_one)?;
        let (mut reference, mut path) = (None, Vec::new());
        match parts.next() {
            None => {}
            Some("tree") => {
                let named = parts
                    .next()
                    .filter(|part| ref_part(part))
                    .ok_or_else(not_one)?;
                reference = Some(named.to_owned());
                for part in parts {
                    if !folder_part(part) {
                        return Err(not_one());
                    }
                    path.push(part);
                }
            }
            Some(_) => return Err(not_one()),
        }
        Ok(Repository {
            owner: owner.to_owned(),
            name: name.to_owned(),
            reference,
            path: path.join("/"),
        })
    }

    /// The repository's address, as the lock file and the review name it.
    pub fn source(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }

    /// The address that answers with the commit the ref is at, as 40 hex
    /// digits (GitHub's API, asked with `Accept: application/vnd.github.sha`).
    pub fn commit_url(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/{}/commits/{}",
            self.owner,
            self.name,
            self.reference.as_deref().unwrap_or("HEAD")
        )
    }

    /// The archive of the repository at a commit.
    pub fn archive_url(&self, commit: &str) -> String {
        format!(
            "https://codeload.github.com/{}/{}/tar.gz/{commit}",
            self.owner, self.name
        )
    }
}

fn name_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

fn ref_part(part: &str) -> bool {
    name_part(part) && !part.starts_with('-')
}

fn folder_part(part: &str) -> bool {
    name_part(part)
}

/// Whether text is a full commit hash: 40 lowercase hex digits.
pub fn is_commit(text: &str) -> bool {
    is_hex(text, 40)
}

/// Whether text is a SHA-256 digest: 64 lowercase hex digits.
pub fn is_digest(text: &str) -> bool {
    is_hex(text, 64)
}

fn is_hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The commit GitHub's API answered with, checked.
pub fn read_commit(body: &[u8]) -> Result<String, String> {
    let text = String::from_utf8_lossy(body).trim().to_ascii_lowercase();
    if is_commit(&text) {
        Ok(text)
    } else {
        Err("GitHub didn't say which commit that is".into())
    }
}

/// Reads the theme in `folder` of a repository's `.tar.gz` archive, as GitHub
/// serves one: every entry under one top folder, named after the repository
/// and commit, which is left out.
///
/// An entry whose path is absolute or climbs out with `..` refuses the whole
/// archive. Links and anything else that isn't a file or a folder are left
/// out, as a theme's files are its own. Of what's in `folder`, the same
/// files a local folder's reading keeps are kept, `theme.json`, the files it
/// names and a licence, under the same size limits.
pub fn unpack(archive: &[u8], folder: &str) -> Result<Files, String> {
    let folder: PathBuf = folder.split('/').filter(|part| !part.is_empty()).collect();
    // First `theme.json` alone, for what it names; then only those files and
    // a licence are kept, so the rest of a repository is never held.
    let mut manifest = BTreeMap::new();
    let mut too_large: Vec<PathBuf> = Vec::new();
    walk(
        archive,
        &folder,
        |relative| relative == Path::new(MANIFEST),
        &mut manifest,
        &mut too_large,
    )?;
    let named: std::collections::HashSet<PathBuf> = manifest
        .get(Path::new(MANIFEST))
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(|text| super::Package::parse(text).ok())
        .map(|package| {
            package
                .files()
                .into_iter()
                .take(MAX_THEME_FILES + 1)
                .map(|relative| relative.components().collect())
                .collect()
        })
        .unwrap_or_default();
    let mut kept = BTreeMap::new();
    too_large.clear();
    walk(
        archive,
        &folder,
        |relative| {
            relative == Path::new(MANIFEST)
                || named.contains(relative)
                || LICENCE_FILES.iter().any(|name| relative == Path::new(name))
        },
        &mut kept,
        &mut too_large,
    )?;
    files_of(kept, &too_large)
}

/// Reads the archive's files under the theme's folder that `wanted` takes,
/// by their path there, under the archive's limits; the names of those too
/// large to keep go in `too_large`, so naming one says so.
fn walk(
    archive: &[u8],
    folder: &Path,
    wanted: impl Fn(&Path) -> bool,
    kept: &mut BTreeMap<PathBuf, Vec<u8>>,
    too_large: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let broken = |error: std::io::Error| format!("The theme's archive couldn't be read: {error}");
    let decoder = flate2::read::GzDecoder::new(archive).take(MAX_UNPACKED_BYTES + 1);
    let mut tar = tar::Archive::new(decoder);
    let (mut entries, mut unpacked) = (0usize, 0u64);
    for entry in tar.entries().map_err(broken)? {
        let mut entry = entry.map_err(broken)?;
        entries += 1;
        if entries > MAX_ARCHIVE_ENTRIES {
            return Err(format!(
                "The repository holds more than {MAX_ARCHIVE_ENTRIES} files; a theme's should hold only the theme"
            ));
        }
        // The entry's size, as a PAX record may give it, not the header's.
        let size = entry.size();
        unpacked = unpacked.saturating_add(size);
        if unpacked > MAX_UNPACKED_BYTES {
            return Err(
                "The repository is too large to be a theme's: it unpacks to more than 128 MiB"
                    .into(),
            );
        }
        let raw = entry.path().map_err(broken)?.into_owned();
        let inside = inside_path(&raw)?;
        let kind = entry.header().entry_type();
        if !kind.is_file() {
            continue;
        }
        // GitHub's top folder, then the theme's own.
        let Some(relative) = inside
            .components()
            .skip(1)
            .collect::<PathBuf>()
            .strip_prefix(folder)
            .ok()
            .map(Path::to_path_buf)
        else {
            continue;
        };
        if relative.as_os_str().is_empty() || !wanted(&relative) {
            continue;
        }
        let limit = if relative == Path::new(MANIFEST) {
            check::MAX_MANIFEST_BYTES
        } else {
            MAX_FONT_BYTES.max(MAX_IMAGE_BYTES).max(1 << 20)
        };
        if size > limit {
            too_large.push(relative);
            continue;
        }
        let mut bytes = Vec::with_capacity(size as usize);
        (&mut entry)
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(broken)?;
        if bytes.len() as u64 > limit {
            too_large.push(relative);
            continue;
        }
        kept.insert(relative, bytes);
    }
    Ok(())
}

/// A path in an archive, as the folders and name under its root; refused if
/// it's absolute or climbs out.
fn inside_path(raw: &Path) -> Result<PathBuf, String> {
    let mut inside = PathBuf::new();
    for part in raw.components() {
        match part {
            Component::Normal(name) => inside.push(name),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "The theme's archive names {}, outside itself, so nothing in it was kept",
                    raw.display()
                ));
            }
        }
    }
    Ok(inside)
}

/// The theme's files from what its folder holds, as [`Files::read`] keeps
/// them from a folder on disk.
fn files_of(mut kept: BTreeMap<PathBuf, Vec<u8>>, too_large: &[PathBuf]) -> Result<Files, String> {
    if too_large.iter().any(|path| path == Path::new(MANIFEST)) {
        return Err(format!("{MANIFEST} is at most 256 KiB"));
    }
    let manifest = kept
        .remove(Path::new(MANIFEST))
        .ok_or_else(|| format!("That repository has no {MANIFEST} where the theme should be"))?;
    let manifest = String::from_utf8(manifest).map_err(|_| format!("{MANIFEST} isn't text"))?;
    let mut files = BTreeMap::new();
    if let Ok(package) = super::Package::parse(&manifest) {
        let mut total = manifest.len() as u64;
        let named = package.files();
        if named.len() > MAX_THEME_FILES {
            return Err(format!(
                "The theme names {} files; a theme from GitHub holds at most {MAX_THEME_FILES}",
                named.len()
            ));
        }
        for relative in named {
            let relative: PathBuf = relative.components().collect();
            if too_large.contains(&relative) {
                return Err(format!(
                    "{} is too large: a theme holds at most 24 MiB, a font 4 MiB and a picture 2 MiB",
                    relative.display()
                ));
            }
            let Some(bytes) = kept.remove(&relative) else {
                continue;
            };
            total += bytes.len() as u64;
            if total > MAX_THEME_BYTES {
                return Err("The theme is too large: a theme holds at most 24 MiB".into());
            }
            files.insert(relative, bytes);
        }
    }
    let licence = LICENCE_FILES.iter().map(PathBuf::from).find_map(|name| {
        let bytes = kept.remove(&name)?;
        (bytes.len() <= 1 << 20).then_some((name, bytes))
    });
    Ok(Files {
        manifest,
        files,
        licence,
    })
}

/// Whether the files are the ones a listing pinned.
pub fn verify(files: &Files, sha256: &str) -> Result<(), String> {
    if digest(files) == sha256 {
        Ok(())
    } else {
        Err(
            "The theme's files aren't the ones the marketplace listed, so it wasn't installed"
                .into(),
        )
    }
}

/// The marketplace's catalog: what it lists, and what it left out and why.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub themes: Vec<Listing>,
    /// Entries left out, each as a sentence naming it.
    pub skipped: Vec<String>,
}

/// One theme the catalog lists, pinned to a commit and its files' digest.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Listing {
    /// The `id` in its `theme.json`, which an install checks.
    pub id: String,
    pub name: String,
    pub author: String,
    /// Its repository, `https://github.com/<owner>/<repository>`.
    pub repo: String,
    /// The folder `theme.json` is in; empty for the repository's root.
    #[serde(default)]
    pub path: String,
    /// The commit it's listed at.
    pub commit: String,
    /// [`super::install::digest`] of its files at that commit.
    pub sha256: String,
    #[serde(default)]
    pub description: String,
    /// A picture of it, on `raw.githubusercontent.com`; empty for none.
    #[serde(default)]
    pub preview: String,
    /// Whether it has a light variant and a dark one.
    #[serde(default)]
    pub light: bool,
    #[serde(default)]
    pub dark: bool,
}

impl Listing {
    /// Where to fetch it from.
    pub fn repository(&self) -> Repository {
        Repository::parse(&self.repo)
            .map(|mut repository| {
                repository.reference = Some(self.commit.clone());
                repository.path.clone_from(&self.path);
                repository
            })
            .expect("a listing's repository was checked when the catalog was read")
    }

    /// "Dark", "Light" or "Dark and light".
    pub fn modes(&self) -> &'static str {
        match (self.dark, self.light) {
            (true, true) => "Dark and light",
            (false, true) => "Light",
            _ => "Dark",
        }
    }

    fn problem(&self) -> Option<&'static str> {
        let repository = Repository::parse(&self.repo).ok();
        if self.id.is_empty() || self.name.trim().is_empty() {
            Some("it has no id or name")
        } else if !repository.is_some_and(|repository| repository.reference.is_none()) {
            Some("its repo isn't a GitHub repository's address")
        } else if !self
            .path
            .split('/')
            .filter(|part| !part.is_empty())
            .all(folder_part)
        {
            Some("its path isn't a folder in the repository")
        } else if !is_commit(&self.commit) {
            Some("its commit isn't a full commit hash")
        } else if !is_digest(&self.sha256) {
            Some("its sha256 isn't a SHA-256 digest")
        } else if !self.preview.is_empty()
            && !self
                .preview
                .starts_with("https://raw.githubusercontent.com/")
        {
            Some("its preview isn't on raw.githubusercontent.com")
        } else {
            None
        }
    }
}

#[derive(Deserialize)]
struct CatalogFile {
    version: u64,
    #[serde(default)]
    themes: Vec<serde_json::Value>,
}

/// Reads the catalog. One that isn't JSON, or is a version this Yapfast
/// doesn't read, is refused; an entry that's wrong is left out and named in
/// [`Catalog::skipped`], and the rest still list. A second entry with an id
/// already listed is left out too.
pub fn read_catalog(bytes: &[u8]) -> Result<Catalog, String> {
    if bytes.len() as u64 > MAX_CATALOG_BYTES {
        return Err("The theme catalog is too large to read".into());
    }
    let file: CatalogFile = serde_json::from_slice(bytes)
        .map_err(|error| format!("The theme catalog couldn't be read: {error}"))?;
    if file.version == 0 || file.version > CATALOG_VERSION {
        return Err(format!(
            "The theme catalog is version {}, and this Yapfast reads version {CATALOG_VERSION}; an update may read it",
            file.version
        ));
    }
    let mut catalog = Catalog::default();
    for (index, value) in file.themes.into_iter().enumerate() {
        let listing = match serde_json::from_value::<Listing>(value) {
            Ok(listing) => listing,
            Err(error) => {
                catalog
                    .skipped
                    .push(format!("entry {} isn't a listing: {error}", index + 1));
                continue;
            }
        };
        if let Some(problem) = listing.problem() {
            catalog
                .skipped
                .push(format!("{} was left out: {problem}", listing.id));
        } else if catalog.themes.iter().any(|listed| listed.id == listing.id) {
            catalog
                .skipped
                .push(format!("{} was left out: it's listed twice", listing.id));
        } else {
            catalog.themes.push(listing);
        }
    }
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::install::{Fetched, Themes};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn manifest(id: &str) -> String {
        format!(
            r##"{{ "theme_api": 1, "id": "{id}", "name": "Plum", "version": "1.0.0",
            "variants": {{ "dark": {{ "base": "yap", "colours": {{ "chat": "#2a1a3a" }} }} }} }}"##
        )
    }

    /// A `.tar.gz` holding each `(path, bytes)` as a file, the path written
    /// into the header as given, as a hostile archive might.
    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let encoder = flate2::write::GzEncoder::new(&mut out, flate2::Compression::fast());
            let mut builder = tar::Builder::new(encoder);
            for (path, bytes) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o644);
                header.set_entry_type(tar::EntryType::Regular);
                // Written raw, so `..` and absolute paths get through.
                let name = &mut header.as_old_mut().name;
                name[..path.len()].copy_from_slice(path.as_bytes());
                header.set_cksum();
                builder.append(&header, *bytes).unwrap();
            }
            builder.into_inner().unwrap().finish().unwrap();
        }
        out
    }

    fn listing() -> String {
        format!(
            r#"{{ "id": "com.example.plum", "name": "Plum", "author": "Ada",
            "repo": "https://github.com/ada/plum", "commit": "{COMMIT}", "sha256": "{}",
            "description": "Dusk.", "preview": "https://raw.githubusercontent.com/ada/plum/{COMMIT}/preview.png",
            "dark": true }}"#,
            "a".repeat(64)
        )
    }

    #[test]
    fn a_repository_address_reads_with_its_ref_and_folder() {
        let plain = Repository::parse("https://github.com/ada/plum.git/").unwrap();
        assert_eq!((plain.owner.as_str(), plain.name.as_str()), ("ada", "plum"));
        assert_eq!(
            (plain.reference.as_deref(), plain.path.as_str()),
            (None, "")
        );
        assert_eq!(
            plain.commit_url(),
            "https://api.github.com/repos/ada/plum/commits/HEAD"
        );
        let folder = Repository::parse("https://github.com/ada/themes/tree/v2/dusk/plum").unwrap();
        assert_eq!(folder.reference.as_deref(), Some("v2"));
        assert_eq!(folder.path, "dusk/plum");
        assert_eq!(folder.source(), "https://github.com/ada/themes");
        assert_eq!(
            folder.archive_url(COMMIT),
            format!("https://codeload.github.com/ada/themes/tar.gz/{COMMIT}")
        );
        for wrong in [
            "http://github.com/ada/plum",
            "https://gitlab.com/ada/plum",
            "https://github.com/ada",
            "https://github.com/ada/plum/blob/main/theme.json",
            "https://github.com/ada/plum/tree/main/../etc",
            "https://github.com/../plum",
        ] {
            assert!(Repository::parse(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn the_api_answers_with_a_commit_or_nothing_is_fetched() {
        assert_eq!(
            read_commit(format!("{COMMIT}\n").as_bytes()).unwrap(),
            COMMIT
        );
        assert!(read_commit(b"{\"message\": \"Not Found\"}").is_err());
    }

    #[test]
    fn an_archive_unpacks_to_the_theme_in_its_folder() {
        let manifest = manifest("com.example.plum");
        let bytes = archive(&[
            ("ada-plum-0123/README.md", b"not the theme's"),
            ("ada-plum-0123/dusk/theme.json", manifest.as_bytes()),
            ("ada-plum-0123/dusk/LICENSE", b"CC0"),
            ("ada-plum-0123/dusk/notes.txt", b"not named"),
        ]);
        let files = unpack(&bytes, "dusk").unwrap();
        assert_eq!(files.manifest, manifest);
        assert!(files.files.is_empty(), "only what theme.json names");
        assert_eq!(files.licence.unwrap().0, PathBuf::from("LICENSE"));
        let error = unpack(&bytes, "").unwrap_err();
        assert!(error.contains("no theme.json"), "{error}");
    }

    #[test]
    fn an_archive_that_climbs_out_or_is_absolute_is_refused() {
        let manifest = manifest("com.example.plum");
        for hostile in [
            "ada-plum-0123/../../evil",
            "/etc/evil",
            "ada-plum-0123/dusk/../../../x",
        ] {
            let bytes = archive(&[
                ("ada-plum-0123/theme.json", manifest.as_bytes()),
                (hostile, b"x"),
            ]);
            let error = unpack(&bytes, "").unwrap_err();
            assert!(error.contains("outside itself"), "{hostile}: {error}");
        }
    }

    #[test]
    fn too_many_files_or_too_much_is_refused() {
        let names: Vec<String> = (0..=MAX_ARCHIVE_ENTRIES)
            .map(|index| format!("ada-plum-0123/{index}"))
            .collect();
        let entries: Vec<(&str, &[u8])> =
            names.iter().map(|name| (name.as_str(), &b""[..])).collect();
        let error = unpack(&archive(&entries), "").unwrap_err();
        assert!(error.contains("more than 4096 files"), "{error}");

        // A font over the limit, named by theme.json.
        let manifest = r##"{ "theme_api": 1, "id": "com.example.plum", "name": "Plum", "version": "1.0.0",
            "type": { "body": { "font": "fonts/Big.ttf" } },
            "variants": { "dark": { "base": "yap" } } }"##;
        let big = vec![0u8; (MAX_FONT_BYTES + 1) as usize];
        let bytes = archive(&[
            ("ada-plum-0123/theme.json", manifest.as_bytes()),
            ("ada-plum-0123/fonts/Big.ttf", &big),
        ]);
        let error = unpack(&bytes, "").unwrap_err();
        assert!(error.contains("too large"), "{error}");

        // More files named than a theme may hold: every icon, and eight
        // sounds for every slot.
        use crate::themes::media::{ICONS, SoundSlot};
        let icons: Vec<String> = ICONS
            .iter()
            .map(|icon| format!(r#""{icon}": "icons/{icon}.svg""#))
            .collect();
        let sounds: Vec<String> = SoundSlot::ALL
            .iter()
            .map(|slot| {
                let files: Vec<String> = (0..8)
                    .map(|index| format!(r#""sounds/{}-{index}.wav""#, slot.name()))
                    .collect();
                format!(r#""{}": [{}]"#, slot.name(), files.join(", "))
            })
            .collect();
        let many = format!(
            r#"{{ "theme_api": 1, "id": "com.example.plum", "name": "Plum", "version": "1.0.0",
            "icons": {{ {} }}, "sounds": {{ {} }}, "variants": {{ "dark": {{ "base": "yap" }} }} }}"#,
            icons.join(", "),
            sounds.join(", ")
        );
        assert!(
            crate::themes::Package::parse(&many).unwrap().files().len() > MAX_THEME_FILES,
            "the fixture names enough files"
        );
        let bytes = archive(&[("ada-plum-0123/theme.json", many.as_bytes())]);
        let error = unpack(&bytes, "").unwrap_err();
        assert!(error.contains("at most 256"), "{error}");

        // Garbage isn't an archive.
        assert!(unpack(b"not gzip", "").is_err());
    }

    #[test]
    fn a_listed_theme_installs_only_with_the_files_it_was_listed_with() {
        let manifest = manifest("com.example.plum");
        let bytes = archive(&[("ada-plum-0123/theme.json", manifest.as_bytes())]);
        let files = unpack(&bytes, "").unwrap();
        let pinned = digest(&files);
        verify(&files, &pinned).unwrap();
        let error = verify(&files, &"0".repeat(64)).unwrap_err();
        assert!(error.contains("aren't the ones"), "{error}");

        let fetched = Fetched::new(
            "https://github.com/ada/plum".into(),
            String::new(),
            COMMIT.into(),
            COMMIT.into(),
            files,
        )
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let themes = Themes::open(directory.path());
        themes.install(&fetched).unwrap();
        let installed = themes.installed();
        assert_eq!(installed[0].entry.rev, COMMIT);
        assert_eq!(
            installed[0].entry.store, pinned,
            "the digest names its store"
        );
    }

    #[test]
    fn a_catalog_lists_its_good_entries_and_names_the_rest() {
        let text = format!(
            r#"{{ "version": 1, "themes": [
                {good},
                {good},
                {{ "id": "com.example.bad", "name": "Bad", "author": "", "repo": "https://gitlab.com/a/b",
                   "commit": "{COMMIT}", "sha256": "{digest}" }},
                {{ "id": "com.example.short", "name": "Short", "author": "", "repo": "https://github.com/a/b",
                   "commit": "abc", "sha256": "{digest}" }},
                {{ "id": "com.example.far", "name": "Far", "author": "", "repo": "https://github.com/a/b",
                   "commit": "{COMMIT}", "sha256": "{digest}", "preview": "https://example.com/p.png" }},
                {{ "name": 3 }}
            ] }}"#,
            good = listing(),
            digest = "b".repeat(64),
        );
        let catalog = read_catalog(text.as_bytes()).unwrap();
        assert_eq!(catalog.themes.len(), 1);
        let plum = &catalog.themes[0];
        assert_eq!((plum.name.as_str(), plum.modes()), ("Plum", "Dark"));
        let repository = plum.repository();
        assert_eq!(repository.reference.as_deref(), Some(COMMIT));
        assert_eq!(catalog.skipped.len(), 5, "{:?}", catalog.skipped);
        assert!(catalog.skipped[0].contains("twice"));
        assert!(catalog.skipped[1].contains("GitHub"));
        assert!(catalog.skipped[2].contains("commit"));
        assert!(catalog.skipped[3].contains("preview"));
        assert!(catalog.skipped[4].contains("entry 6"));
    }

    #[test]
    fn a_bad_or_later_catalog_is_refused_whole() {
        assert!(read_catalog(b"<html>offline</html>").is_err());
        let later = read_catalog(br#"{ "version": 2, "themes": [] }"#).unwrap_err();
        assert!(later.contains("version 2"), "{later}");
        assert!(read_catalog(br#"{ "themes": [] }"#).is_err());
        let huge = vec![b' '; (MAX_CATALOG_BYTES + 1) as usize];
        assert!(read_catalog(&huge).is_err());
        assert!(
            read_catalog(br#"{ "version": 1 }"#)
                .unwrap()
                .themes
                .is_empty()
        );
    }
}
