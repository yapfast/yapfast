//! `fs.folder` and `fs.full`: a plugin's reach into one folder, answered by the
//! parent and never by the helper.
//!
//! The helper keeps no filesystem at all — its OS sandbox refuses every path —
//! so a plugin's files are read and written here, on its behalf, through
//! [`Root::resolve`]. A path a plugin names is a relative path of plain parts
//! joined by `/`; `..`, a root, a drive and an empty part are refused before
//! anything is opened, and what the path names is then resolved by the
//! operating system and must still be inside the folder, so a symbolic link
//! that points out of it is refused like a `..` would be. A file written is
//! checked the same way through the folder it goes in, and an existing link at
//! its name is refused rather than followed.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use yapfast_plugin_api::call::{FsBytes, FsEntry, FsRead, FsThumbnail, Thumbnail};

/// The most one `fs.write` carries, which is what one frame holds as base64.
pub const MAX_WRITE_BYTES: usize = 768 << 10;

/// The most entries one `fs.list` answers with.
pub const MAX_LIST: usize = 1000;

/// The longest path a plugin may name, in bytes.
pub const MAX_PATH_BYTES: usize = 4096;

/// A folder a plugin may reach, already resolved by the operating system.
#[derive(Clone, Debug)]
pub struct Root {
    real: PathBuf,
    write: bool,
}

impl Root {
    /// The folder at `path`, which must exist.
    pub fn new(path: &Path, write: bool) -> Result<Self, String> {
        let real = std::fs::canonicalize(path)
            .map_err(|error| format!("the folder {} can't be opened: {error}", path.display()))?;
        if !real.is_dir() {
            return Err(format!("{} isn't a folder", path.display()));
        }
        Ok(Root { real, write })
    }

    /// Whether this root takes writes.
    pub fn writes(&self) -> bool {
        self.write
    }

    /// The plain parts of a path a plugin named, or why it isn't one.
    fn parts(named: &str) -> Result<PathBuf, String> {
        if named.len() > MAX_PATH_BYTES {
            return Err(format!("a path is at most {MAX_PATH_BYTES} bytes"));
        }
        if named.contains('\0') || named.contains('\\') {
            return Err(format!("`{named}` isn't a path inside the folder"));
        }
        let mut out = PathBuf::new();
        for part in named.split('/') {
            if part.is_empty() || part == "." {
                continue;
            }
            let component = Path::new(part).components().next();
            match component {
                Some(Component::Normal(normal)) if Path::new(part).components().count() == 1 => {
                    out.push(normal);
                }
                _ => {
                    return Err(format!(
                        "`{named}` leaves the folder: a path names what is inside it, with no \
                         `..`, drive or root"
                    ));
                }
            }
        }
        Ok(out)
    }

    /// Something that exists inside the folder, resolved: a link that leads
    /// out of the folder is refused.
    pub fn resolve(&self, named: &str) -> Result<PathBuf, String> {
        let relative = Self::parts(named)?;
        let joined = self.real.join(&relative);
        let real = std::fs::canonicalize(&joined)
            .map_err(|_| format!("there is nothing at `{named}` in the folder"))?;
        if !real.starts_with(&self.real) {
            return Err(format!(
                "`{named}` is a link to somewhere outside the folder, which a plugin can't follow"
            ));
        }
        Ok(real)
    }

    /// Where a file to write goes: its folder resolved and inside, and its
    /// own name not a link.
    fn resolve_new(&self, named: &str) -> Result<PathBuf, String> {
        if !self.write {
            return Err("this plugin may read its folder, not write to it".to_owned());
        }
        let relative = Self::parts(named)?;
        let name = relative
            .file_name()
            .ok_or_else(|| "`fs.write` needs a file's name".to_owned())?
            .to_owned();
        let parent = match relative.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                self.resolve(&parent.to_string_lossy().replace('\\', "/"))?
            }
            _ => self.real.clone(),
        };
        let target = parent.join(name);
        if let Ok(metadata) = std::fs::symlink_metadata(&target)
            && (metadata.file_type().is_symlink() || metadata.is_dir())
        {
            return Err(format!(
                "`{named}` is a link or a folder, which `fs.write` doesn't write through"
            ));
        }
        Ok(target)
    }

    /// The path inside the folder of something already resolved.
    fn shown(&self, real: &Path) -> String {
        real.strip_prefix(&self.real)
            .map(|inside| inside.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default()
    }

    fn entry(&self, real: &Path) -> Result<FsEntry, String> {
        let metadata =
            std::fs::metadata(real).map_err(|error| format!("it can't be read: {error}"))?;
        Ok(FsEntry {
            path: self.shown(real),
            dir: metadata.is_dir(),
            bytes: if metadata.is_dir() { 0 } else { metadata.len() },
            modified: metadata
                .modified()
                .ok()
                .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|since| since.as_millis() as u64),
        })
    }

    /// `fs.stat`.
    pub fn stat(&self, named: &str) -> Result<FsEntry, String> {
        self.entry(&self.resolve(named)?)
    }

    /// `fs.list`: what one folder holds, sorted, at most [`MAX_LIST`]. An
    /// entry that is a link out of the folder is left out.
    pub fn list(&self, named: &str) -> Result<Vec<FsEntry>, String> {
        let folder = self.resolve(named)?;
        let mut entries: Vec<FsEntry> = std::fs::read_dir(&folder)
            .map_err(|error| format!("`{named}` can't be listed: {error}"))?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let real = std::fs::canonicalize(entry.path()).ok()?;
                real.starts_with(&self.real)
                    .then(|| self.entry(&real).ok())
                    .flatten()
            })
            .take(MAX_LIST)
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    /// `fs.read`: at most [`FsRead::MAX_BYTES`] of one file.
    pub fn read(&self, request: &FsRead) -> Result<FsBytes, String> {
        let real = self.resolve(&request.path)?;
        let mut file = std::fs::File::open(&real)
            .map_err(|error| format!("`{}` can't be read: {error}", request.path))?;
        let size = file.metadata().map_err(|error| error.to_string())?.len();
        let length = request
            .length
            .unwrap_or(FsRead::MAX_BYTES)
            .min(FsRead::MAX_BYTES);
        file.seek(SeekFrom::Start(request.offset))
            .map_err(|error| error.to_string())?;
        let mut bytes = Vec::new();
        file.take(length)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        Ok(FsBytes {
            size,
            offset: request.offset,
            bytes_base64: yapfast_plugin_api::call::base64::encode(&bytes),
        })
    }

    /// `fs.write`.
    pub fn write(&self, named: &str, bytes: &[u8], append: bool) -> Result<u64, String> {
        if bytes.len() > MAX_WRITE_BYTES {
            return Err(format!(
                "`fs.write` writes at most {MAX_WRITE_BYTES} bytes at a time"
            ));
        }
        let target = self.resolve_new(named)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&target)
            .map_err(|error| format!("`{named}` can't be written: {error}"))?;
        file.write_all(bytes)
            .map_err(|error| format!("`{named}` can't be written: {error}"))?;
        Ok(bytes.len() as u64)
    }

    /// A file inside the folder, for `messages.attach`: resolved and a file.
    pub fn file(&self, named: &str) -> Result<PathBuf, String> {
        let real = self.resolve(named)?;
        if !real.is_file() {
            return Err(format!("`{named}` isn't a file"));
        }
        Ok(real)
    }
}

/// `fs.thumbnail`, run off every thread a person waits on: the picture is
/// measured before it is read, decoded under a size limit, and scaled to at
/// most [`FsThumbnail::MAX_SIDE`] pixels on its longer side.
pub fn thumbnail(root: &Root, request: &FsThumbnail) -> Result<Thumbnail, String> {
    let real = root.file(&request.path)?;
    let bytes = std::fs::metadata(&real)
        .map_err(|error| error.to_string())?
        .len();
    if bytes > FsThumbnail::MAX_SOURCE_BYTES {
        return Err(format!(
            "`{}` is {bytes} bytes, and a thumbnail is made from a picture of at most {} MiB",
            request.path,
            FsThumbnail::MAX_SOURCE_BYTES >> 20
        ));
    }
    let side = request.size.clamp(16, FsThumbnail::MAX_SIDE);
    let mut reader = image::ImageReader::open(&real)
        .map_err(|error| error.to_string())?
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 << 20);
    reader.limits(limits);
    let picture = reader
        .decode()
        .map_err(|error| format!("`{}` isn't a picture Yapfast reads: {error}", request.path))?;
    let small = picture.thumbnail(side, side);
    let mut png = std::io::Cursor::new(Vec::new());
    small
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(Thumbnail {
        width: small.width(),
        height: small.height(),
        png_base64: yapfast_plugin_api::call::base64::encode(png.get_ref()),
    })
}

/// The home folder, for `fs.full` with `home`.
pub fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// The disk's root, for `fs.full` with `disk`.
pub fn disk() -> PathBuf {
    if cfg!(windows) {
        {
            // `C:` alone is the drive's current folder, not its root.
            let mut root = std::env::var_os("SystemDrive").unwrap_or_else(|| "C:".into());
            root.push("\\");
            PathBuf::from(root)
        }
    } else {
        PathBuf::from("/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> (tempfile::TempDir, Root) {
        let directory = tempfile::tempdir().expect("a directory");
        let inside = directory.path().join("inside");
        std::fs::create_dir_all(inside.join("sub")).expect("folders");
        std::fs::write(inside.join("a.txt"), b"hello").expect("a file");
        std::fs::write(directory.path().join("secret.txt"), b"not yours").expect("a file");
        let root = Root::new(&inside, true).expect("a root");
        (directory, root)
    }

    #[test]
    fn a_path_names_what_is_inside_and_dot_dot_never_climbs_out() {
        let (_directory, root) = folder();
        assert!(root.stat("a.txt").is_ok());
        assert!(root.stat("sub/../a.txt").is_err(), "no `..` at all");
        for escape in [
            "../secret.txt",
            "sub/../../secret.txt",
            "/etc/passwd",
            "..",
            "a\\..\\..\\secret.txt",
            "a.txt\0",
        ] {
            assert!(root.stat(escape).is_err(), "`{escape}` escaped");
            assert!(
                root.read(&FsRead {
                    path: escape.into(),
                    ..FsRead::default()
                })
                .is_err(),
                "`{escape}` was read"
            );
            assert!(
                root.write(escape, b"x", false).is_err(),
                "`{escape}` was written"
            );
        }
        assert_eq!(
            std::fs::read(root.real.parent().unwrap().join("secret.txt")).unwrap(),
            b"not yours"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_folder_is_not_followed_for_reading_listing_or_writing() {
        let (directory, root) = folder();
        let outside = directory.path().join("secret.txt");
        std::os::unix::fs::symlink(&outside, root.real.join("link.txt")).expect("a link");
        std::os::unix::fs::symlink(directory.path(), root.real.join("up")).expect("a link");
        assert!(root.stat("link.txt").is_err());
        assert!(
            root.read(&FsRead {
                path: "up/secret.txt".into(),
                ..FsRead::default()
            })
            .is_err(),
            "a linked folder is no way out"
        );
        assert!(root.write("link.txt", b"overwritten", false).is_err());
        assert!(root.write("up/secret.txt", b"overwritten", false).is_err());
        let listed: Vec<String> = root
            .list("")
            .expect("a list")
            .into_iter()
            .map(|entry| entry.path)
            .collect();
        assert_eq!(listed, ["a.txt", "sub"], "the links out are left out");
        assert_eq!(std::fs::read(&outside).unwrap(), b"not yours");
        // A link that stays inside is fine.
        std::os::unix::fs::symlink(root.real.join("a.txt"), root.real.join("sub/same.txt"))
            .expect("a link");
        assert!(root.stat("sub/same.txt").is_ok());
    }

    #[test]
    fn reads_are_capped_and_writes_need_write_access() {
        let (_directory, root) = folder();
        let piece = root
            .read(&FsRead {
                path: "a.txt".into(),
                offset: 1,
                length: Some(3),
            })
            .expect("a read");
        assert_eq!(piece.size, 5);
        assert_eq!(
            yapfast_plugin_api::call::base64::decode(&piece.bytes_base64).unwrap(),
            b"ell"
        );
        assert_eq!(root.write("sub/new.txt", b"made", false), Ok(4));
        assert_eq!(root.write("sub/new.txt", b"!", true), Ok(1));
        assert_eq!(
            std::fs::read(root.real.join("sub/new.txt")).unwrap(),
            b"made!"
        );
        assert!(
            root.write("big.bin", &vec![0; MAX_WRITE_BYTES + 1], false)
                .is_err()
        );
        let reading = Root::new(&root.real, false).expect("a root");
        assert!(reading.write("x.txt", b"x", false).is_err());
    }

    #[test]
    fn a_thumbnail_is_small_and_a_file_that_isnt_a_picture_is_refused() {
        let (_directory, root) = folder();
        let picture = image::RgbImage::from_pixel(600, 300, image::Rgb([200, 30, 30]));
        picture.save(root.real.join("big.png")).expect("a picture");
        let made = thumbnail(
            &root,
            &FsThumbnail {
                path: "big.png".into(),
                size: 9999,
            },
        )
        .expect("a thumbnail");
        assert_eq!((made.width, made.height), (256, 128), "capped at 256");
        assert!(
            thumbnail(
                &root,
                &FsThumbnail {
                    path: "a.txt".into(),
                    size: 64
                }
            )
            .is_err()
        );
    }
}
