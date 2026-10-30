//! Desktop entries on Linux: the one that starts Yapfast at sign-in
//! (`crate::autostart`), and the one an AppImage installs for itself.
//!
//! Desktops find a window's icon through the desktop entry its app ID names.
//! On Wayland that's the only place they look, so without an entry the
//! taskbar and title bar show a generic icon. A package installs the entry and
//! the icon; an AppImage installs nothing, so Yapfast run from one writes both
//! to the person's data folder before it opens its window, which also puts it
//! in the app menu. The entry runs the AppImage where it is now, and its
//! TryExec hides the entry once the AppImage is deleted.

use std::{
    io,
    path::{Path, PathBuf},
};

use crate::icon::APP_ID;

/// The entry a package installs, whose Exec line names the AppImage instead.
const ENTRY: &str = include_str!("../packaging/linux/io.github.yapfast.Yapfast.desktop");
const ICON: &[u8] = include_bytes!("../packaging/icon.svg");

/// The AppImage this Yapfast runs from. An AppImage runs from a temporary
/// mount, APPDIR, and names its file in APPIMAGE. Programs it starts inherit
/// both, so they're Yapfast's only when Yapfast runs from inside that mount.
pub fn appimage(exe: &Path) -> Option<PathBuf> {
    let mount = std::env::var_os("APPDIR")?;
    let image = std::env::var_os("APPIMAGE")?;
    exe.starts_with(mount).then(|| PathBuf::from(image))
}

/// Puts Yapfast's entry and icon in the person's data folder when it runs
/// from an AppImage, unless a package or an AppImage manager already did.
pub fn install() {
    let Some(image) = std::env::current_exe().ok().and_then(|exe| appimage(&exe)) else {
        return;
    };
    let Some(data) = xdg_home("XDG_DATA_HOME", ".local/share") else {
        return;
    };
    match install_in(&data, &data_dirs(), &image) {
        Ok(true) => tracing::info!(appimage = %image.display(), "added Yapfast to the app menu"),
        Ok(false) => {}
        Err(error) => tracing::warn!(%error, "could not add Yapfast to the app menu"),
    }
}

/// Writes the entry and icon into `data`, or leaves them: whether it wrote.
pub(crate) fn install_in(data: &Path, system: &[PathBuf], image: &Path) -> io::Result<bool> {
    let image = image
        .to_str()
        .filter(|image| !image.contains(['\n', '\r']))
        .ok_or_else(|| io::Error::other("the AppImage's path can't go in a desktop entry"))?;
    let name = format!("{APP_ID}.desktop");
    let applications = data.join("applications");
    let path = applications.join(&name);
    let icon = data
        .join("icons/hicolor/scalable/apps")
        .join(format!("{APP_ID}.svg"));
    let text = entry(image);
    // The usual start, with both in place: two small reads.
    let icon_current = std::fs::read(&icon).is_ok_and(|bytes| bytes == ICON);
    if icon_current && std::fs::read_to_string(&path).is_ok_and(|current| current == text) {
        return Ok(false);
    }
    // A package's entry, which one of the person's own would hide.
    if system
        .iter()
        .any(|folder| folder.join("applications").join(&name).exists())
    {
        return Ok(false);
    }
    if managed(&applications, &name, image)? {
        return Ok(false);
    }
    if !icon_current {
        write(&icon, ICON)?;
    }
    // After the icon, so a desktop that sees the entry finds its icon too.
    write(&path, text.as_bytes())?;
    Ok(true)
}

/// The packaged entry, run from `image`.
fn entry(image: &str) -> String {
    let tried = image.replace('\\', "\\\\");
    ENTRY.replace(
        "\nExec=yapfast %u\n",
        &format!("\nTryExec={tried}\nExec={} %u\n", quoted(image)),
    )
}

/// Whether an AppImage manager, such as Gear Lever or AppImageLauncher, gave
/// this AppImage an entry of its own, which Yapfast's would show twice.
fn managed(applications: &Path, ours: &str, image: &str) -> io::Result<bool> {
    let entries = match std::fs::read_dir(applications) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        entries => entries?,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name == ours || !name.ends_with(".desktop") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        if text
            .lines()
            .any(|line| line.starts_with("Exec=") && line.contains(image))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `$XDG_DATA_DIRS`, where packages install their entries.
fn data_dirs() -> Vec<PathBuf> {
    let dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    std::env::split_paths(&dirs)
        .filter(|path| path.is_absolute())
        .collect()
}

/// An XDG base folder from its variable, or the one in the home folder.
pub(crate) fn xdg_home(variable: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(fallback)))
}

/// Writes a file whole or not at all: a desktop reading it never sees half.
/// Readable by all, as a package installs it, not the temporary file's 0600.
pub(crate) fn write(path: &Path, contents: &[u8]) -> io::Result<()> {
    use std::{io::Write, os::unix::fs::PermissionsExt};
    let folder = path.parent().expect("the file is in a folder");
    std::fs::create_dir_all(folder)?;
    let mut file = tempfile::NamedTempFile::new_in(folder)?;
    file.write_all(contents)?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o644))?;
    file.persist(path).map(drop).map_err(|error| error.error)
}

/// One argument of an entry's Exec line, quoted as the Desktop Entry
/// specification asks when it holds anything but plain characters.
pub(crate) fn quoted(argument: &str) -> String {
    let argument = argument.replace('%', "%%");
    let plain = !argument.is_empty()
        && argument
            .chars()
            .all(|character| character.is_alphanumeric() || "-_./=:+,@%".contains(character));
    if plain {
        return argument;
    }
    let mut quoted = String::from('"');
    for character in argument.chars() {
        if matches!(character, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(character);
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMAGE: &str = "/home/a/Apps/Yapfast 0.4.AppImage";

    fn entry_in(data: &Path) -> String {
        std::fs::read_to_string(data.join(format!("applications/{APP_ID}.desktop"))).unwrap()
    }

    #[test]
    fn an_appimage_adds_its_entry_and_icon() {
        let data = tempfile::tempdir().unwrap();
        assert!(install_in(data.path(), &[], Path::new(IMAGE)).unwrap());
        let text = entry_in(data.path());
        assert!(text.contains(&format!("\nTryExec={IMAGE}\nExec=\"{IMAGE}\" %u\n")));
        assert!(text.contains("MimeType=x-scheme-handler/yapfast;x-scheme-handler/discord;"));
        assert!(text.contains(&format!("\nIcon={APP_ID}\n")));
        let icon = data
            .path()
            .join(format!("icons/hicolor/scalable/apps/{APP_ID}.svg"));
        assert_eq!(std::fs::read(&icon).unwrap(), ICON);
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(icon).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o644);
        // The next start finds both in place.
        assert!(!install_in(data.path(), &[], Path::new(IMAGE)).unwrap());
    }

    #[test]
    fn a_moved_appimage_updates_its_entry() {
        let data = tempfile::tempdir().unwrap();
        install_in(data.path(), &[], Path::new("/tmp/Yapfast.AppImage")).unwrap();
        assert!(install_in(data.path(), &[], Path::new(IMAGE)).unwrap());
        assert!(entry_in(data.path()).contains(&format!("\nExec=\"{IMAGE}\" %u\n")));
    }

    #[test]
    fn a_package_or_a_manager_keeps_its_own_entry() {
        let data = tempfile::tempdir().unwrap();
        let system = tempfile::tempdir().unwrap();
        let package = system.path().join(format!("applications/{APP_ID}.desktop"));
        write(&package, ENTRY.as_bytes()).unwrap();
        let system = [system.path().to_owned()];
        assert!(!install_in(data.path(), &system, Path::new(IMAGE)).unwrap());

        let gear_lever = data.path().join("applications/yapfast.desktop");
        write(
            &gear_lever,
            format!("[Desktop Entry]\nExec=\"{IMAGE}\"\n").as_bytes(),
        )
        .unwrap();
        assert!(!install_in(data.path(), &[], Path::new(IMAGE)).unwrap());
        assert!(
            !data
                .path()
                .join(format!("applications/{APP_ID}.desktop"))
                .exists()
        );
    }
}
