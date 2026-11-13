use super::*;

/// A plugin loaded from a folder, and when its files are looked at again.
pub(super) struct DevWatch {
    pub(super) folder: PathBuf,
    /// What the manifest and its module looked like when they were read.
    pub(super) stamp: String,
    /// When to look again.
    pub(super) due: Instant,
}
