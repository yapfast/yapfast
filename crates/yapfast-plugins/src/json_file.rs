//! Writing one of the plugin system's JSON files, whole.
//!
//! The files here are small and written rarely, so each is written beside its
//! destination and renamed over it: a crash mid-write leaves the previous file
//! rather than half of one. One helper, because both `grants.json` and
//! `budget.json` want exactly this and nothing else.

use std::path::Path;

use anyhow::{Context, Result};

/// Writes `bytes` to `path` so that a crash leaves either file whole.
///
/// The temporary file is in the same directory, so the rename is one `rename`
/// within one filesystem, and a reader never sees a half-written file.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(directory)
        .with_context(|| format!("could not make {}", directory.display()))?;
    let mut file = tempfile::NamedTempFile::new_in(directory)
        .with_context(|| format!("could not write beside {}", path.display()))?;
    std::io::Write::write_all(&mut file, bytes)
        .with_context(|| format!("could not write {}", path.display()))?;
    file.as_file()
        .sync_all()
        .with_context(|| format!("could not flush {}", path.display()))?;
    file.persist(path)
        .map(|_| ())
        .with_context(|| format!("could not replace {}", path.display()))
}
