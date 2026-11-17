//! The data folder's `layout.json`, where the window keeps its
//! arrangement and the settings that are the window's own.

use std::{io, path::Path};

/// The file, in the data folder.
pub const LAYOUT: &str = "layout.json";

/// Writes `bytes` as `root`'s `layout.json`: to a file beside it, then over
/// it, so a crash or a power cut mid-write leaves the old file whole
/// rather than a short one that reads as empty.
pub fn write(root: &Path, bytes: &[u8]) -> io::Result<()> {
    std::fs::create_dir_all(root)?;
    let beside = root.join("layout.json.tmp");
    std::fs::write(&beside, bytes)?;
    std::fs::rename(&beside, root.join(LAYOUT))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_layout_is_replaced_whole() {
        let folder = tempfile::tempdir().expect("a folder");
        super::write(folder.path(), b"{\"a\": 1}").expect("written");
        super::write(folder.path(), b"{\"a\": 2}").expect("written again");
        let kept = std::fs::read(folder.path().join(super::LAYOUT)).expect("kept");
        assert_eq!(kept, b"{\"a\": 2}");
        assert!(!folder.path().join("layout.json.tmp").exists());
    }
}
