//! Where a saved attachment goes: the desktop's save dialog when it has
//! one, else straight into the Downloads folder, as a browser saves a
//! download.

use std::path::{Path, PathBuf};

/// The first of `name`, `name (1)`, `name (2)`… that isn't in `folder` yet,
/// the number before the extension as browsers put it: `notes (1).txt`. A
/// name with no extension, as `Home`, takes it at the end.
pub fn free_path(folder: &Path, name: &str) -> PathBuf {
    let path = folder.join(name);
    if !exists(&path) {
        return path;
    }
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    (1..)
        .map(|n| folder.join(format!("{stem} ({n}){extension}")))
        .find(|path| !exists(path))
        .unwrap_or(path)
}

/// Whether anything is at `path`, a broken link included.
fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// Where `name` goes when there's no save dialog to ask: a free name in the
/// Downloads folder, which is made if it isn't there.
pub fn fallback(name: &str) -> Option<PathBuf> {
    let (_, folder) = yapfast_core::home_and_downloads()?;
    std::fs::create_dir_all(&folder).ok()?;
    Some(free_path(&folder, name))
}

/// Whether the desktop's file chooser portal answers, which rfd's save
/// dialog goes through. Without it rfd shows nothing and answers as if the
/// save were cancelled, so the click seemed to do nothing; the file then
/// goes to Downloads instead.
#[cfg(target_os = "linux")]
pub async fn has_save_dialog() -> bool {
    let Ok(connection) = zbus::Connection::session().await else {
        return false;
    };
    connection
        .call_method(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.portal.FileChooser", "version"),
        )
        .await
        .is_ok()
}

/// Windows and macOS always have their own save dialog.
#[cfg(not(target_os = "linux"))]
pub async fn has_save_dialog() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_file_takes_a_name_not_used_yet() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path();
        assert_eq!(free_path(path, "Home"), path.join("Home"));
        std::fs::write(path.join("Home"), "").unwrap();
        assert_eq!(free_path(path, "Home"), path.join("Home (1)"));
        std::fs::write(path.join("Home (1)"), "").unwrap();
        assert_eq!(free_path(path, "Home"), path.join("Home (2)"));
        std::fs::write(path.join("notes.txt"), "").unwrap();
        assert_eq!(free_path(path, "notes.txt"), path.join("notes (1).txt"));
        std::fs::write(path.join(".env"), "").unwrap();
        assert_eq!(free_path(path, ".env"), path.join(".env (1)"));
    }
}
