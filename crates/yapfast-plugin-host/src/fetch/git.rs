//! Reading one plugin out of one commit of a repository.
//!
//! A fetch is shallow: `git`'s objects for the commit a ref points at, and
//! nothing behind it, so a plugin's history is never downloaded. There is no
//! checkout either — the files are read straight out of the tree and written to
//! a scratch directory, which means no `core.*` setting, no filter driver and no
//! file name from the repository can reach the filesystem as anything but the
//! bytes it holds.
//!
//! The caps are enforced while the fetch runs: a watchdog stops a clone that
//! takes too long or grows past its size, and every file is measured from its
//! object header before its bytes are decoded, so an oversized module is refused
//! without ever being held.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use gix::bstr::ByteSlice;
use gix::objs::tree::EntryKind;
use yapfast_plugin_api::limits;

use super::url::Source;

/// How often the watchdog looks at the clock and the size on disk.
const WATCH_INTERVAL: Duration = Duration::from_millis(250);
/// The depth of directories a plugin may nest.
const MAX_DEPTH: usize = 32;
/// The files one plugin directory may hold: a plugin is code and a little data.
const MAX_FILES: usize = 512;

/// A plugin's files, read out of one commit into a scratch directory.
pub struct Staged {
    /// The full id of the commit that was read.
    pub commit: String,
    /// The plugin directory's files, at their paths relative to it.
    pub dir: PathBuf,
    /// The text of its `yapfast-plugin.json`.
    pub manifest: String,
    /// The directory inside the repository, if the URL named one.
    pub path: Option<String>,
}

/// Fetches the commit `source` names and writes its plugin directory under
/// `work`.
///
/// `work` is the fetch's own: the bare repository goes in `work/repo` and the
/// plugin's files in `work/plugin`, so both go away with the directory that
/// holds them.
pub fn fetch(source: &Source, work: &Path) -> anyhow::Result<Staged> {
    let repository = work.join("repo");
    clone(source, &repository)?;
    let repo = gix::open(repository.clone())
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
        })?;
    let commit = commit.id().to_string();

    let dir = work.join("plugin");
    std::fs::create_dir(&dir)
        .with_context(|| format!("`{}` can't hold the plugin", dir.display()))?;
    let mut budget = Budget::default();
    let tree = repo.find_object(commit_id)?.peel_to_tree()?;
    let tree = match &source.path {
        Some(path) => descend(&repo, tree, path, source)?,
        None => tree,
    };
    let prefix = source
        .path
        .as_ref()
        .map(|path| format!("{path}/"))
        .unwrap_or_default();
    stage(&repo, tree, &dir, &prefix, &mut budget, 0)?;

    let manifest_path = dir.join(super::MANIFEST_FILE);
    if !manifest_path.is_file() {
        bail!(
            "{} has no `{}`; a plugin is a directory that holds one, and a URL can name that \
             directory with `#path=plugins/foo`",
            match &source.path {
                Some(path) => format!("`{path}` in `{}`", source.repo),
                None => format!("`{}`", source.repo),
            },
            super::MANIFEST_FILE
        );
    }
    let manifest = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("`{}` isn't UTF-8 text", super::MANIFEST_FILE))?;

    Ok(Staged {
        commit,
        dir,
        manifest,
        path: source.path.clone(),
    })
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

/// Walks to the directory a source names, refusing anything on the way that
/// isn't a real directory.
fn descend<'repo>(
    repo: &'repo gix::Repository,
    mut tree: gix::Tree<'repo>,
    path: &str,
    source: &Source,
) -> anyhow::Result<gix::Tree<'repo>> {
    let mut prefix = String::new();
    for component in path.split('/').filter(|part| !part.is_empty()) {
        let Some((oid, kind)) = find(&tree, component) else {
            bail!(
                "`{}` has no `{prefix}{component}` at {}",
                source.repo,
                source.reference_name()
            );
        };
        match kind {
            EntryKind::Tree => tree = repo.find_tree(oid)?,
            EntryKind::Link => bail!(
                "`{prefix}{component}` is a symbolic link; a plugin's files and directories are \
                 real ones, so nothing is followed out of the repository"
            ),
            EntryKind::Commit => {
                bail!("`{prefix}{component}` is a submodule, and a fetch never downloads one")
            }
            _ => bail!(
                "`{prefix}{component}` is a file, not a directory; a source names the directory \
                 that holds `{}`",
                super::MANIFEST_FILE
            ),
        }
        prefix.push_str(component);
        prefix.push('/');
    }
    Ok(tree)
}

/// Writes a tree's files into `out`, at their paths relative to it.
fn stage(
    repo: &gix::Repository,
    tree: gix::Tree<'_>,
    out: &Path,
    prefix: &str,
    budget: &mut Budget,
    depth: usize,
) -> anyhow::Result<()> {
    for entry in tree.iter() {
        let entry = entry.context("a directory of the plugin couldn't be read")?;
        let name = entry.filename().to_str().map_err(|_| {
            anyhow::anyhow!(
                "`{prefix}` holds a file name that isn't UTF-8, which a plugin can't carry"
            )
        })?;
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
            bail!("`{prefix}{name}` isn't a name a file can have");
        }
        let relative = format!("{prefix}{name}");
        match entry.mode().kind() {
            EntryKind::Blob | EntryKind::BlobExecutable => {
                stage_blob(repo, entry.object_id(), &out.join(name), &relative, budget)?;
            }
            EntryKind::Tree => {
                if depth >= MAX_DEPTH {
                    bail!("`{relative}` is nested deeper than {MAX_DEPTH} directories");
                }
                let dir = out.join(name);
                std::fs::create_dir(&dir)
                    .with_context(|| format!("`{relative}` couldn't be written"))?;
                let sub = repo.find_tree(entry.object_id())?;
                stage(repo, sub, &dir, &format!("{relative}/"), budget, depth + 1)?;
            }
            EntryKind::Link => bail!(
                "`{relative}` is a symbolic link; a plugin's files are real files, so nothing is \
                 followed out of the repository"
            ),
            EntryKind::Commit => {
                bail!("`{relative}` is a submodule, and a fetch never downloads one")
            }
        }
    }
    Ok(())
}

/// Writes one blob, measured and capped before its bytes are decoded.
fn stage_blob(
    repo: &gix::Repository,
    oid: gix::ObjectId,
    destination: &Path,
    relative: &str,
    budget: &mut Budget,
) -> anyhow::Result<()> {
    let header = repo
        .find_header(oid)
        .with_context(|| format!("`{relative}` couldn't be read"))?;
    if header.kind() != gix::objs::Kind::Blob {
        bail!("`{relative}` isn't a file");
    }
    let size = header.size();
    if size > limits::MAX_WASM_BYTES {
        bail!(
            "`{relative}` is {size} bytes; a plugin's files are at most {} MiB each",
            limits::MAX_WASM_BYTES >> 20
        );
    }
    budget.take(size, relative)?;
    // gix decodes an object in one step — there is no streaming decode — so the
    // header above is what bounds this: at most one capped file's bytes are in
    // memory at a time, and they are written out and dropped before the next
    // entry is read.
    let blob = repo.find_blob(oid)?;
    if let Some(pointer) = lfs_pointer(&blob.data) {
        bail!(
            "`{relative}` is a Git LFS pointer ({pointer}), not the file: Git LFS keeps the bytes \
             on another server, so nothing here could run. Commit the built file to the \
             repository itself, without LFS"
        );
    }
    std::fs::write(destination, &blob.data)
        .with_context(|| format!("`{relative}` couldn't be written"))?;
    Ok(())
}

/// The one name in a tree that matches, with its id and kind.
fn find(tree: &gix::Tree<'_>, name: &str) -> Option<(gix::ObjectId, EntryKind)> {
    let entry = tree.find_entry(name.as_bytes())?;
    Some((entry.object_id(), entry.mode().kind()))
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

/// The bytes and files written so far, against the caps on what a fetch may
/// read.
#[derive(Default)]
struct Budget {
    written: u64,
    files: usize,
}

impl Budget {
    fn take(&mut self, bytes: u64, relative: &str) -> anyhow::Result<()> {
        self.written += bytes;
        self.files += 1;
        if self.files > MAX_FILES {
            bail!(
                "the plugin holds more than {MAX_FILES} files, at `{relative}`; a plugin is code \
                 and a little data"
            );
        }
        if self.written > limits::MAX_FETCH_BYTES {
            bail!(
                "the plugin's files come to more than the {} MiB a plugin may carry, at \
                 `{relative}`",
                limits::MAX_FETCH_BYTES >> 20
            );
        }
        Ok(())
    }
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
    fn a_budget_stops_at_the_cap() {
        let mut budget = Budget::default();
        assert!(budget.take(limits::MAX_FETCH_BYTES, "a").is_ok());
        assert!(budget.take(1, "b").is_err());
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
}
