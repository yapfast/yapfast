//! Copying the files the person chooses into the data folder, and deleting
//! the copies no longer chosen.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{Format, MOST_BYTES, Problem, Stored, read_problem, woff};

/// What became of a file chosen: the copy it's drawn from, or its name and
/// why there's none.
pub type Imported = Result<Stored, (String, Problem)>;

/// Copies the font files at `paths` into `folder`, unwrapping WOFF. It
/// reads and writes files, so it runs off the window's thread.
pub fn import(folder: &Path, paths: &[PathBuf]) -> Vec<Imported> {
    paths
        .iter()
        .map(|path| {
            let name = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned();
            copy(folder, path)
                .map(|file| Stored {
                    name: name.clone(),
                    file,
                })
                .map_err(|problem| (name, problem))
        })
        .collect()
}

/// The extension a copy takes from its first bytes, for whoever looks in the
/// folder; Anomi itself reads the bytes.
fn extension(head: &[u8]) -> &'static str {
    match head.get(..4) {
        Some(b"OTTO") => "otf",
        Some(b"ttcf") => "ttc",
        _ => "ttf",
    }
}

/// Copies the font file at `path` into `folder`, as a plain font, and gives
/// the copy's name. The same font chosen again finds its copy there.
fn copy(folder: &Path, path: &Path) -> Result<String, Problem> {
    let mut file = fs::File::open(path).map_err(read_problem)?;
    let metadata = file.metadata().map_err(read_problem)?;
    if !metadata.is_file() {
        return Err(Problem::NotAFont);
    }
    if metadata.len() > MOST_BYTES as u64 {
        return Err(Problem::TooLarge);
    }
    let mut head = [0u8; 4];
    file.read_exact(&mut head).map_err(|_| Problem::NotAFont)?;
    let format = Format::of(&head).ok_or(Problem::NotAFont)?;
    if format == Format::Woff2 {
        return Err(Problem::Woff2);
    }
    fs::create_dir_all(folder).map_err(|_| Problem::NotCopied)?;
    let mut temporary = tempfile::NamedTempFile::new_in(folder).map_err(|_| Problem::NotCopied)?;
    let mut hash = Sha256::new();
    let extension = if format == Format::Woff {
        let mut packed = Vec::with_capacity(metadata.len() as usize);
        packed.extend_from_slice(&head);
        (&mut file)
            .take(MOST_BYTES as u64)
            .read_to_end(&mut packed)
            .map_err(|_| Problem::Unreadable)?;
        let font = woff::to_sfnt(&packed).map_err(|error| match error {
            woff::Error::TooLarge => Problem::TooLarge,
            _ => Problem::Damaged,
        })?;
        // The compressed file isn't needed once its tables are out.
        drop(packed);
        hash.update(&font);
        temporary.write_all(&font).map_err(|_| Problem::NotCopied)?;
        extension(&font)
    } else {
        // Streamed: a font is never whole in memory.
        hash.update(head);
        temporary.write_all(&head).map_err(|_| Problem::NotCopied)?;
        let mut buffer = [0u8; 16 << 10];
        let mut total = head.len();
        loop {
            let read = match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Problem::Unreadable),
            };
            total += read;
            if total > MOST_BYTES {
                return Err(Problem::TooLarge);
            }
            hash.update(&buffer[..read]);
            temporary
                .write_all(&buffer[..read])
                .map_err(|_| Problem::NotCopied)?;
        }
        extension(&head)
    };
    // On the disk before it has its name, so a crash never leaves a name
    // that a later choice of the same font would take for a whole copy.
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| Problem::NotCopied)?;
    let digest = hash.finalize();
    let mut name = String::with_capacity(36);
    for byte in &digest[..16] {
        let _ = write!(name, "{byte:02x}");
    }
    name.push('.');
    name.push_str(extension);
    // A copy is never written over, even with the same bytes: it may be
    // mapped.
    match temporary.persist_noclobber(folder.join(&name)) {
        Ok(_) => Ok(name),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(name),
        Err(_) => Err(Problem::NotCopied),
    }
}

/// Deletes the copies in `folder` that none of `kept` names, and leftovers
/// of a copy cut short, and the folder once nothing is kept. A copy the
/// system won't delete while it's mapped, as Windows, stays until a later
/// sweep.
pub fn sweep(folder: &Path, kept: &[Stored]) {
    let Ok(listing) = fs::read_dir(folder) else {
        return;
    };
    for entry in listing.flatten() {
        let name = entry.file_name();
        let wanted = name
            .to_str()
            .is_some_and(|name| kept.iter().any(|stored| stored.file == name));
        if !wanted {
            let _ = fs::remove_file(entry.path());
        }
    }
    if kept.is_empty() {
        let _ = fs::remove_dir(folder);
    }
}
