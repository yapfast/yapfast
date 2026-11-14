//! Reading one plugin out of one commit of a repository.
//!
//! A fetch is shallow: `git`'s objects for the commit a ref points at, and
//! nothing behind it, so a plugin's history is never downloaded. There is no
//! checkout either: a file's bytes are read straight out of the tree, so no
//! `core.*` setting, no filter driver and no file name from the repository can
//! reach the filesystem as anything but the bytes it holds.
//!
//! The caps are enforced while the fetch runs — a watchdog stops a clone that
//! takes too long or grows past its size — and every file is measured from its
//! object header before its bytes are decoded, so an oversized one is refused
//! without ever being held whole.

use std::num::NonZeroU32;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use gix::objs::Find as _;
use gix::objs::tree::EntryKind;
use yapfast_plugin_api::limits;

use super::url::Source;

/// How often the watchdog looks at the clock and the size on disk.
const WATCH_INTERVAL: Duration = Duration::from_millis(250);
/// The depth a file may nest inside a plugin.
const MAX_DEPTH: usize = 32;

/// How much may be read for one file, and how a larger one is refused.
#[derive(Clone, Copy)]
pub struct Cap {
    /// The largest file read for it, measured from its object header.
    pub bytes: u64,
    /// The rest of the sentence a larger one is refused with, as in `a module
    /// is at most 8 MiB`.
    pub label: &'static str,
}

/// One commit of one repository, open for reading the plugin's files.
pub struct Snapshot {
    repo: gix::Repository,
    /// The tree of the folder the source names: what the plugin's paths are
    /// relative to.
    root: gix::ObjectId,
    /// The full id of the commit that was read.
    pub commit: String,
}

impl Snapshot {
    /// Clones `source` into `work` and opens the commit and folder it names.
    ///
    /// `work` is the fetch's own: the bare repository goes in `work/repo`, so it
    /// goes away with the directory that holds it.
    pub fn open(source: &Source, work: &Path) -> anyhow::Result<Snapshot> {
        let repository = work.join("repo");
        clone(source, &repository)?;
        let repo = gix::open(repository)
            .with_context(|| format!("`{}` couldn't be reopened after the fetch", source.repo))?;
        let commit_id = resolve(&repo, source)?;
        let commit = repo
            .find_object(commit_id)
            .with_context(|| format!("commit {commit_id} is missing from the fetch"))?
            .peel_to_commit()
            .map_err(|_| {
                anyhow::anyhow!(
                    "`{}` points at {commit_id}, which isn't a commit; name a branch or tag",
                    source.reference_name()
                )
            })
            .map(|commit| commit.id().to_string())?;
        let root = {
            let tree = repo.find_object(commit_id)?.peel_to_tree()?;
            match &source.path {
                Some(path) => directory(&repo, tree, path, source)?,
                None => tree.id,
            }
        };
        Ok(Snapshot { repo, root, commit })
    }

    /// The bytes of the file at `path` inside the plugin's folder, or `None`
    /// when the folder has no such file.
    ///
    /// `path` is checked before anything is read: it must stay inside the
    /// folder, and every part of it must be a real file or directory there. A
    /// symbolic link is refused rather than followed, and a submodule is refused
    /// because a fetch never downloads one.
    pub fn file(&self, path: &Path, cap: Cap) -> anyhow::Result<Option<Vec<u8>>> {
        let shown = path.display().to_string();
        let mut parts = checked_parts(path, &shown)?;
        let Some(name) = parts.pop() else {
            bail!("`{shown}` isn't a path inside the plugin's directory");
        };
        let mut tree = self.repo.find_tree(self.root)?;
        let mut prefix = String::new();
        for (depth, part) in parts.iter().enumerate() {
            let relative = format!("{prefix}{part}");
            let Some(entry) = tree.find_entry(part.as_bytes()) else {
                return Ok(None);
            };
            match entry.mode().kind() {
                EntryKind::Tree => {
                    if depth >= MAX_DEPTH {
                        bail!("`{relative}` is nested deeper than {MAX_DEPTH} directories");
                    }
                    tree = self.repo.find_tree(entry.object_id())?;
                    prefix = format!("{relative}/");
                }
                kind => return Err(not_a_directory(&relative, kind)),
            }
        }
        let relative = format!("{prefix}{name}");
        let Some(entry) = tree.find_entry(name.as_bytes()) else {
            return Ok(None);
        };
        match entry.mode().kind() {
            EntryKind::Blob | EntryKind::BlobExecutable => {
                read_blob(&self.repo, entry.object_id(), &relative, cap).map(Some)
            }
            kind => Err(not_a_file(&relative, kind)),
        }
    }
}

/// Splits a path into the parts a tree can be looked up by, refusing any that
/// would read outside the plugin's folder.
fn checked_parts(path: &Path, shown: &str) -> anyhow::Result<Vec<String>> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let name = part.to_str().ok_or_else(|| {
                    anyhow::anyhow!("`{shown}` has a name that isn't UTF-8, so it can't be read")
                })?;
                if name.is_empty() || name.contains(['\\', '\0']) {
                    bail!("`{shown}` isn't a name a file in a plugin can have");
                }
                parts.push(name.to_owned());
            }
            // `./plugin.wasm` reads the same file.
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("`{shown}` isn't a path inside the plugin's directory")
            }
        }
    }
    if parts.is_empty() {
        bail!("`{shown}` isn't a path inside the plugin's directory");
    }
    Ok(parts)
}

/// Reads one blob, measured and capped before its bytes are decoded.
fn read_blob(
    repo: &gix::Repository,
    oid: gix::ObjectId,
    relative: &str,
    cap: Cap,
) -> anyhow::Result<Vec<u8>> {
    let header = repo
        .find_header(oid)
        .with_context(|| format!("`{relative}` couldn't be read"))?;
    if header.kind() != gix::objs::Kind::Blob {
        bail!("`{relative}` isn't a file");
    }
    let size = header.size();
    if size > cap.bytes {
        bail!("`{relative}` is {size} bytes; {}", cap.label);
    }
    // Decoded straight into a buffer of our own: gix hands an object back with
    // its bytes borrowed from a buffer it keeps, so this is the one place a
    // fetched file is ever held, and it is held once.
    let mut bytes = Vec::with_capacity(size as usize);
    {
        let found = repo
            .objects
            .try_find(&oid, &mut bytes)
            .with_context(|| format!("`{relative}` couldn't be read"))?;
        let Some(data) = found else {
            bail!("`{relative}` is missing from the fetch");
        };
        if let Some(pointer) = lfs_pointer(data.data) {
            bail!(
                "`{relative}` is a Git LFS pointer ({pointer}), not the file: Git LFS keeps the \
                 bytes on another server, so nothing here could run. Check in the file itself, \
                 committed to the repository without LFS"
            );
        }
    }
    Ok(bytes)
}

/// Why an entry that isn't a real directory is refused, by its name.
fn not_a_directory(relative: &str, kind: EntryKind) -> anyhow::Error {
    match kind {
        EntryKind::Link => anyhow::anyhow!(
            "`{relative}` is a symbolic link; a plugin's files and directories are real ones, so \
             nothing is followed out of the repository"
        ),
        EntryKind::Commit => {
            anyhow::anyhow!("`{relative}` is a submodule, and a fetch never downloads one")
        }
        _ => anyhow::anyhow!("`{relative}` is a file, not a directory"),
    }
}

/// Why an entry that isn't a real file is refused, by its name.
fn not_a_file(relative: &str, kind: EntryKind) -> anyhow::Error {
    match kind {
        EntryKind::Tree => anyhow::anyhow!("`{relative}` is a directory, not a file"),
        kind => not_a_directory(relative, kind),
    }
}

/// Clones the ref shallowly into a bare repository at `destination`.
fn clone(source: &Source, destination: &Path) -> anyhow::Result<()> {
    let mut prepare =
        gix::prepare_clone_bare(source.repo.as_str(), destination).map_err(|error| {
            anyhow::anyhow!(
                "`{}` couldn't be opened as a repository: {error}{}",
                source.repo,
                file_scheme_hint(source)
            )
        })?;
    // One commit, not one history: a plugin's past is never part of an install.
    prepare = prepare.with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(NonZeroU32::MIN));
    prepare = match &source.reference {
        Some(reference) if is_object_id(reference) => {
            // A commit id is asked for on its own, which reaches a pin even when
            // the branch it came from has moved on.
            prepare.with_revision(Some(reference.as_str()))?
        }
        Some(reference) => prepare.with_ref_name(Some(gix::bstr::BStr::new(reference)))?,
        None => prepare,
    };

    let interrupt = AtomicBool::new(false);
    let tripped = AtomicU8::new(Trip::None as u8);
    let stop = AtomicBool::new(false);
    let outcome = thread::scope(|scope| {
        let watch = scope.spawn(|| watch(destination, &interrupt, &tripped, &stop));
        let outcome = prepare.fetch_only(gix::progress::Discard, &interrupt);
        stop.store(true, Ordering::Relaxed);
        let _ = watch.join();
        outcome
    });
    match Trip::of(tripped.load(Ordering::Relaxed)) {
        Trip::Time => bail!(
            "fetching `{}` took longer than {} seconds, so it was stopped; a plugin is a small \
             repository, and a source that takes longer is probably not one",
            source.repo,
            limits::MAX_FETCH_SECONDS
        ),
        Trip::Size => bail!(
            "`{}` is larger than the {} MiB a fetch may use, so it was stopped; Yapfast installs \
             one plugin, not a history{}",
            source.repo,
            limits::MAX_FETCH_BYTES >> 20,
            file_scheme_hint(source)
        ),
        Trip::None => {}
    }
    if let Err(error) = outcome {
        bail!(
            "`{}` couldn't be fetched: {error}{}",
            source.repo,
            file_scheme_hint(source)
        );
    }
    // The watchdog looks a few times a second, so a last burst can land between
    // two looks; the size is checked once more before anything is read.
    let size = directory_size(destination);
    if size > limits::MAX_FETCH_BYTES {
        bail!(
            "`{}` fetched {} bytes, over the {} MiB a fetch may use",
            source.repo,
            size,
            limits::MAX_FETCH_BYTES >> 20
        );
    }
    Ok(())
}

/// The commit a fetched repository's ref resolves to.
fn resolve(repo: &gix::Repository, source: &Source) -> anyhow::Result<gix::ObjectId> {
    let id = match &source.reference {
        None => repo.head_id().map_err(|error| {
            anyhow::anyhow!("`{}` has no default branch to read: {error}", source.repo)
        })?,
        Some(reference) => repo.rev_parse_single(reference.as_str()).map_err(|_| {
            anyhow::anyhow!(
                "`{}` has no branch, tag or commit `{reference}`",
                source.repo
            )
        })?,
    };
    Ok(id.detach())
}

/// Walks to the folder a source names, refusing anything on the way that isn't
/// a real directory.
fn directory(
    repo: &gix::Repository,
    tree: gix::Tree<'_>,
    path: &str,
    source: &Source,
) -> anyhow::Result<gix::ObjectId> {
    let mut current = tree.id;
    let mut prefix = String::new();
    for component in path.split('/').filter(|part| !part.is_empty()) {
        let tree = repo.find_tree(current)?;
        let Some(entry) = tree.find_entry(component.as_bytes()) else {
            bail!(
                "`{}` has no `{prefix}{component}` at {}",
                source.repo,
                source.reference_name()
            );
        };
        match entry.mode().kind() {
            EntryKind::Tree => current = entry.object_id(),
            kind => return Err(not_a_directory(&format!("{prefix}{component}"), kind)),
        }
        prefix.push_str(component);
        prefix.push('/');
    }
    Ok(current)
}

/// The first line of a Git LFS pointer, which stands in for a file git doesn't
/// hold: the bytes live on an LFS server and never reach a plugin.
const LFS_PREFIX: &str = "version https://git-lfs.github.com/spec/";

/// The pointer's object id, if these bytes are a Git LFS pointer rather than
/// the file they stand for.
fn lfs_pointer(data: &[u8]) -> Option<String> {
    let head = std::str::from_utf8(data.get(..256).unwrap_or(data)).ok()?;
    if !head.starts_with(LFS_PREFIX) {
        return None;
    }
    let oid = head
        .lines()
        .find_map(|line| line.strip_prefix("oid sha256:"))
        .unwrap_or("no object id");
    Some(format!("oid sha256:{oid}"))
}

/// Why a fetch was stopped early.
#[repr(u8)]
enum Trip {
    /// It wasn't: the clone ended on its own.
    None = 0,
    /// It ran past the time a fetch may take.
    Time = 1,
    /// It grew past the space a fetch may use.
    Size = 2,
}

impl Trip {
    fn of(value: u8) -> Trip {
        match value {
            1 => Trip::Time,
            2 => Trip::Size,
            _ => Trip::None,
        }
    }
}

/// Stops a clone that runs too long or grows too large, while it runs.
fn watch(directory: &Path, interrupt: &AtomicBool, tripped: &AtomicU8, stop: &AtomicBool) {
    let started = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(WATCH_INTERVAL);
        let reason = if started.elapsed() > Duration::from_secs(limits::MAX_FETCH_SECONDS) {
            Trip::Time
        } else if directory_size(directory) > limits::MAX_FETCH_BYTES {
            Trip::Size
        } else {
            continue;
        };
        tripped.store(reason as u8, Ordering::Relaxed);
        // gix stops at the next place it checks, and deletes the repository it
        // was filling, so nothing over the cap survives the interruption.
        interrupt.store(true, Ordering::Relaxed);
        return;
    }
}

/// How many bytes a directory tree holds, counting what a running fetch has
/// written so far.
fn directory_size(directory: &Path) -> u64 {
    let mut total = 0;
    let mut pending = vec![directory.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.metadata() {
                // A fetch writes files as it goes, so anything that is already
                // gone is not counted; the cap only has to be a bound.
                Ok(metadata) if metadata.is_dir() => pending.push(entry.path()),
                Ok(metadata) => total += metadata.len(),
                Err(_) => {}
            }
        }
    }
    total
}

/// Whether a ref names a commit by its id rather than a branch or tag.
fn is_object_id(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Why a fetch of a `file://` source may have failed: reading one runs git's
/// own upload-pack, so git has to be installed.
fn file_scheme_hint(source: &Source) -> &'static str {
    if source.repo.starts_with("file://") {
        "; a `file://` source is read with `git-upload-pack`, so git has to be on PATH"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_pointer_as_a_pointer() {
        let pointer = b"version https://git-lfs.github.com/spec/v1\noid sha256:4d7a2146\nsize 17\n";
        assert_eq!(lfs_pointer(pointer).as_deref(), Some("oid sha256:4d7a2146"));
    }

    #[test]
    fn a_module_with_the_same_first_bytes_is_not_a_pointer() {
        let module = b"\0asm\x01\0\0\0version https://git-lfs.github.com/spec/v1\n";
        assert_eq!(lfs_pointer(module), None);
    }

    #[test]
    fn names_a_commit_id_and_not_a_branch() {
        assert!(is_object_id("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_object_id("main"));
        assert!(!is_object_id("0123456789abcdef0123456789abcdef0123456"));
    }

    #[test]
    fn a_size_the_watchdog_can_read() {
        assert_eq!(Trip::of(0) as u8, Trip::None as u8);
        assert!(matches!(Trip::of(2), Trip::Size));
    }

    #[test]
    fn refuses_a_path_that_climbs_out_of_the_plugin() {
        assert!(checked_parts(Path::new("../outside.wasm"), "../outside.wasm").is_err());
        assert!(
            checked_parts(Path::new("/etc/passwd"), "/etc/passwd").is_err(),
            "an absolute path leaves the folder too"
        );
        assert!(checked_parts(Path::new(""), "").is_err());
        assert_eq!(
            checked_parts(Path::new("./plugin.wasm"), "./plugin.wasm")
                .expect("a path that stays inside"),
            vec!["plugin.wasm".to_owned()]
        );
    }
}
