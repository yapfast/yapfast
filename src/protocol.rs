//! Opening Discord's own protocol links in Yapfast: a current-user
//! protocol registration on Windows, and the person's default
//! applications on Linux. Yapfast's own `yapfast:` scheme is registered by
//! its installer or desktop entry; this claims `discord:` only when
//! Settings says to, and gives the previous handler back when turned off.

use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

/// Whether this platform can claim Discord's protocol links.
pub const SUPPORTED: bool = cfg!(any(windows, target_os = "linux"));

/// Whether Discord protocol links open in this Yapfast.
pub fn discord_enabled(root: &Path) -> bool {
    platform::enabled(root)
}

/// Claims Discord protocol links for this Yapfast, or gives them back.
pub fn set_discord_enabled(root: &Path, enabled: bool) -> std::io::Result<()> {
    platform::set(root, enabled)
}

/// Points a claimed registration at this Yapfast after its folder moved
/// or an update replaced it. A handler someone else chose since is left.
pub fn refresh(root: &Path) {
    if let Err(error) = platform::refresh(root) {
        tracing::warn!(%error, "could not update Discord link handling");
    }
}

/// This Yapfast as a command: itself, its data folder when it isn't the
/// usual one, then the link the desktop hands it.
#[cfg(windows)]
fn command(root: &Path) -> std::io::Result<Vec<String>> {
    let program = std::env::current_exe()?;
    #[cfg(target_os = "linux")]
    let program = crate::desktop_entry::appimage(&program).unwrap_or(program);
    let mut command = vec![path_text(program)?];
    let usual = yapfast_core::default_directory().ok();
    if usual.as_deref() != Some(root) {
        command.extend([
            "--data-dir".to_owned(),
            path_text(std::path::absolute(root)?)?,
        ]);
    }
    Ok(command)
}

#[cfg(windows)]
fn path_text(path: PathBuf) -> std::io::Result<String> {
    path.into_os_string()
        .into_string()
        .map_err(|path| std::io::Error::other(format!("{} isn't valid Unicode", path.display())))
}

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(any(windows, target_os = "linux")))]
use none as platform;
#[cfg(windows)]
use windows as platform;

#[cfg(not(any(windows, target_os = "linux")))]
mod none {
    use std::path::Path;

    pub fn enabled(_: &Path) -> bool {
        false
    }

    pub fn set(_: &Path, _: bool) -> std::io::Result<()> {
        Err(std::io::Error::other(
            "opening Discord links here isn't supported on this platform",
        ))
    }

    pub fn refresh(_: &Path) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use std::{io, path::Path};

    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
        System::Registry::{
            HKEY_CURRENT_USER, REG_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
            RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
        },
    };

    const PROTOCOL: &str = r"Software\Classes\discord";
    const YAPFAST_PROTOCOL: &str = r"Software\Classes\yapfast";
    const YAPFAST_COMMAND: &str = r"Software\Classes\yapfast\shell\open\command";
    const YAPFAST_ICON: &str = r"Software\Classes\yapfast\DefaultIcon";
    const COMMAND: &str = r"Software\Classes\discord\shell\open\command";
    const BACKUP: &str = r"Software\Yapfast\DiscordProtocol";
    const PREVIOUS_COMMAND: &str = "PreviousCommand";
    const HAD_COMMAND: &str = "HadCommand";
    const HAD_URL_PROTOCOL: &str = "HadUrlProtocol";

    pub fn enabled(root: &Path) -> bool {
        let Some(current) = query_string(COMMAND, None) else {
            return false;
        };
        super::command(root).is_ok_and(|command| command_line(&command) == current)
    }

    pub fn set(root: &Path, enabled: bool) -> io::Result<()> {
        let ours = command_line(&super::command(root)?);
        let current = query_string(COMMAND, None);
        match enabled {
            true => {
                if current.as_deref() != Some(ours.as_str())
                    && !current.as_deref().is_some_and(is_ours)
                {
                    set_string(
                        BACKUP,
                        Some(HAD_COMMAND),
                        if current.is_some() { "1" } else { "0" },
                    )?;
                    set_string(
                        BACKUP,
                        Some(PREVIOUS_COMMAND),
                        current.as_deref().unwrap_or_default(),
                    )?;
                    set_string(
                        BACKUP,
                        Some(HAD_URL_PROTOCOL),
                        if query_string(PROTOCOL, Some("URL Protocol")).is_some() {
                            "1"
                        } else {
                            "0"
                        },
                    )?;
                }
                register_yapfast(root, &ours)?;
                if query_string(PROTOCOL, Some("URL Protocol")).is_none() {
                    set_string(PROTOCOL, Some("URL Protocol"), "")?;
                }
                set_string(COMMAND, None, &ours)
            }
            false => {
                if current.as_deref().is_some_and(is_ours) {
                    match query_string(BACKUP, Some(HAD_COMMAND)).as_deref() {
                        Some("1") => {
                            let previous =
                                query_string(BACKUP, Some(PREVIOUS_COMMAND)).unwrap_or_default();
                            set_string(COMMAND, None, &previous)?;
                        }
                        _ => delete_value(COMMAND, None)?,
                    }
                    if query_string(BACKUP, Some(HAD_URL_PROTOCOL)).as_deref() == Some("0") {
                        delete_value(PROTOCOL, Some("URL Protocol"))?;
                    }
                }
                delete_value(BACKUP, Some(PREVIOUS_COMMAND))?;
                delete_value(BACKUP, Some(HAD_COMMAND))?;
                delete_value(BACKUP, Some(HAD_URL_PROTOCOL))
            }
        }
    }

    pub fn refresh(root: &Path) -> io::Result<()> {
        if query_string(BACKUP, Some(HAD_COMMAND)).is_some()
            && query_string(COMMAND, None).as_deref().is_some_and(is_ours)
        {
            let ours = command_line(&super::command(root)?);
            register_yapfast(root, &ours)?;
            set_string(COMMAND, None, &ours)?;
        }
        Ok(())
    }

    /// Registers Yapfast's own scheme for this copy, as the installer
    /// does, so a portable copy can be handed `yapfast:` links too.
    fn register_yapfast(root: &Path, ours: &str) -> io::Result<()> {
        set_string(YAPFAST_PROTOCOL, None, "URL:Yapfast Protocol")?;
        set_string(YAPFAST_PROTOCOL, Some("URL Protocol"), "")?;
        let program = super::command(root)?
            .into_iter()
            .next()
            .ok_or_else(|| io::Error::other("Yapfast's own path is unavailable"))?;
        set_string(
            YAPFAST_ICON,
            None,
            &format!("{},0", crate::autostart::windows_argument(&program)),
        )?;
        set_string(YAPFAST_COMMAND, None, ours)
    }

    /// The registered command line, with the link last, after
    /// `--open-link`. Windows puts the link in as it comes, quotes
    /// included, so whatever it splits into follows the marker, where
    /// Yapfast reads no option.
    fn command_line(command: &[String]) -> String {
        let mut line: Vec<String> = command
            .iter()
            .map(|argument| crate::autostart::windows_argument(argument))
            .collect();
        line.extend([crate::OPEN_LINK.to_owned(), "\"%1\"".to_owned()]);
        line.join(" ")
    }

    /// Whether a registered command runs a Yapfast, wherever it lives,
    /// with or without `--open-link`, as earlier builds registered it.
    fn is_ours(command: &str) -> bool {
        let command = command.to_ascii_lowercase();
        command.contains("yapfast.exe") && command.contains("%1")
    }

    fn set_string(key: &str, name: Option<&str>, value: &str) -> io::Result<()> {
        let value = wide(value);
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                wide(key).as_ptr(),
                name.map(wide)
                    .as_ref()
                    .map_or(std::ptr::null(), Vec::as_ptr),
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        };
        checked(status)
    }

    fn delete_value(key: &str, name: Option<&str>) -> io::Result<()> {
        let status = unsafe {
            RegDeleteKeyValueW(
                HKEY_CURRENT_USER,
                wide(key).as_ptr(),
                name.map(wide)
                    .as_ref()
                    .map_or(std::ptr::null(), Vec::as_ptr),
            )
        };
        match status {
            ERROR_FILE_NOT_FOUND => Ok(()),
            status => checked(status),
        }
    }

    /// A string value under `key`, unexpanded, by its name or the key's
    /// default value when it has none.
    fn query_string(key: &str, name: Option<&str>) -> Option<String> {
        let (key, name) = (wide(key), name.map(wide));
        let name = name.as_ref().map_or(std::ptr::null(), Vec::as_ptr);
        let mut size = 0u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name,
                RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS || size < 2 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name,
                RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
                std::ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        data.truncate(size as usize);
        let text: Vec<u16> = data
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|&unit| unit != 0)
            .collect();
        String::from_utf16(&text).ok()
    }

    fn checked(status: WIN32_ERROR) -> io::Result<()> {
        match status {
            ERROR_SUCCESS => Ok(()),
            status => Err(io::Error::from_raw_os_error(status as i32)),
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn recognises_its_own_registered_command() {
            assert!(is_ours(r#""C:\Apps\Yapfast\yapfast.exe" --open-link "%1""#));
            assert!(is_ours(r#""C:\Apps\Yapfast\yapfast.exe" "%1""#));
            assert!(!is_ours(r#""C:\Apps\Discord\Discord.exe" "%1""#));
            assert_eq!(
                command_line(&[r"C:\Apps\Yapfast\yapfast.exe".into()]),
                r#""C:\Apps\Yapfast\yapfast.exe" --open-link "%1""#
            );
        }

        /// Writes the current user's registry: run it under Wine with a
        /// throwaway WINEPREFIX, not on a Windows account.
        #[test]
        #[ignore = "writes the current user's Classes key"]
        fn claims_and_restores_the_discord_protocol() {
            let previous = query_string(COMMAND, None);
            set(Path::new("."), true).unwrap();
            assert!(enabled(Path::new(".")));
            set(Path::new("."), false).unwrap();
            assert!(!enabled(Path::new(".")));
            assert_eq!(query_string(COMMAND, None), previous);
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    //! The person's own `mimeapps.list` is the setting: it's on while its
    //! default applications name Yapfast first for `discord:`. Yapfast's
    //! desktop entry declares only `yapfast:`, so without a Discord app
    //! installed the desktop doesn't fall back to Yapfast for Discord's
    //! links; gio and xdg-open follow a default all the same.

    use std::{
        io::{self, Write},
        path::{Path, PathBuf},
    };

    use crate::desktop_entry;

    /// What the switch claims: Discord's links, and Yapfast's own, which
    /// another app may have been chosen for.
    const SCHEMES: [&str; 2] = ["x-scheme-handler/discord", "x-scheme-handler/yapfast"];
    const DEFAULTS: &str = "[Default Applications]";
    /// In the data folder: each scheme's line in `mimeapps.list` before
    /// Yapfast claimed it, as it was, or nothing when there was none.
    const PREVIOUS: &str = "discord-protocol-previous";

    fn desktop() -> String {
        format!("{}.desktop", crate::icon::APP_ID)
    }

    pub fn enabled(_: &Path) -> bool {
        list().is_some_and(|list| enabled_in(&list))
    }

    pub fn set(root: &Path, enabled: bool) -> io::Result<()> {
        let list = list().ok_or_else(|| io::Error::other("no home folder"))?;
        if enabled {
            desktop_entry::install();
            if !desktop_exists() {
                return Err(io::Error::other("Yapfast's desktop entry isn't installed"));
            }
        }
        apply(&list, &root.join(PREVIOUS), enabled)
    }

    pub fn refresh(_: &Path) -> io::Result<()> {
        // A package or the AppImage entry itself names the current
        // Yapfast; the default names only that entry.
        Ok(())
    }

    fn enabled_in(list: &Path) -> bool {
        std::fs::read_to_string(list)
            .is_ok_and(|text| default_line(&text, SCHEMES[0]).is_some_and(names_yapfast))
    }

    /// Claims the schemes in `list`, keeping their lines in `previous`, or
    /// puts those lines back where Yapfast still has them.
    fn apply(list: &Path, previous: &Path, enabled: bool) -> io::Result<()> {
        let current = match std::fs::read_to_string(list) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            text => text?,
        };
        let mut text = current.clone();
        if enabled {
            // Kept from the first claim on: a line naming Yapfast is no
            // one else's to give back.
            if !previous.exists() {
                let lines = SCHEMES.map(|scheme| {
                    default_line(&current, scheme).filter(|line| !names_yapfast(line))
                });
                let kept: String = lines
                    .into_iter()
                    .flatten()
                    .map(|line| format!("{line}\n"))
                    .collect();
                desktop_entry::write(previous, kept.as_bytes())?;
            }
            for scheme in SCHEMES {
                text = with_line(&text, scheme, Some(&format!("{scheme}={};", desktop())));
            }
        } else {
            let kept = std::fs::read_to_string(previous).unwrap_or_default();
            for scheme in SCHEMES {
                if default_line(&text, scheme).is_some_and(names_yapfast) {
                    let line = kept.lines().find(|line| key(line) == Some(scheme));
                    text = with_line(&text, scheme, line);
                }
            }
        }
        if text != current {
            write_list(list, &text)?;
        }
        if !enabled {
            match std::fs::remove_file(previous) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                result => result?,
            }
        }
        Ok(())
    }

    /// Whether a default line names Yapfast first.
    fn names_yapfast(line: &str) -> bool {
        line.split_once('=')
            .and_then(|(_, value)| value.split(';').next())
            .is_some_and(|first| first.trim() == desktop())
    }

    /// The key a `key=value` line sets.
    fn key(line: &str) -> Option<&str> {
        line.split_once('=').map(|(key, _)| key.trim())
    }

    /// Where the `[Default Applications]` section's lines start and end.
    fn defaults(lines: &[&str]) -> Option<(usize, usize)> {
        let start = lines.iter().position(|line| line.trim() == DEFAULTS)? + 1;
        let end = lines[start..]
            .iter()
            .position(|line| line.trim_start().starts_with('['))
            .map_or(lines.len(), |next| start + next);
        Some((start, end))
    }

    /// `scheme`'s line among the default applications, as it's written.
    fn default_line<'a>(text: &'a str, scheme: &str) -> Option<&'a str> {
        let lines: Vec<&str> = text.lines().collect();
        let (start, end) = defaults(&lines)?;
        lines[start..end]
            .iter()
            .find(|line| key(line) == Some(scheme))
            .copied()
    }

    /// A mimeapps.list with `scheme`'s default line set to `line` or taken
    /// out, its other sections and lines as they were.
    fn with_line(text: &str, scheme: &str, line: Option<&str>) -> String {
        let mut lines: Vec<&str> = text.lines().collect();
        match defaults(&lines) {
            None => {
                if let Some(line) = line {
                    if !lines.is_empty() {
                        lines.push("");
                    }
                    lines.extend([DEFAULTS, line]);
                }
            }
            Some((start, end)) => {
                let index = lines[start..end]
                    .iter()
                    .position(|line| key(line) == Some(scheme))
                    .map(|index| start + index);
                match (index, line) {
                    (Some(index), Some(line)) => lines[index] = line,
                    (Some(index), None) => {
                        lines.remove(index);
                    }
                    (None, Some(line)) => {
                        // After the section's last line, before a blank one.
                        let after = lines[start..end]
                            .iter()
                            .rposition(|line| !line.trim().is_empty())
                            .map_or(start, |last| start + last + 1);
                        lines.insert(after, line);
                    }
                    (None, None) => {}
                }
            }
        }
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        text
    }

    /// Writes the list where it is: through a link, as home-manager or a
    /// dotfiles folder links it from elsewhere, which a new file would cut
    /// off, and keeping its permissions. One that can't be written there
    /// is reported rather than replaced.
    fn write_list(list: &Path, text: &str) -> io::Result<()> {
        let target = match std::fs::symlink_metadata(list) {
            Ok(metadata) if metadata.file_type().is_symlink() => std::fs::canonicalize(list)?,
            _ => list.to_owned(),
        };
        let permissions = match std::fs::metadata(&target) {
            Ok(metadata) => {
                // A rename would replace a file that can't be written.
                std::fs::OpenOptions::new().write(true).open(&target)?;
                metadata.permissions()
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                std::os::unix::fs::PermissionsExt::from_mode(0o644)
            }
            Err(error) => return Err(error),
        };
        let folder = target.parent().expect("the list is in a folder");
        std::fs::create_dir_all(folder)?;
        let mut file = tempfile::NamedTempFile::new_in(folder)?;
        file.write_all(text.as_bytes())?;
        file.as_file().set_permissions(permissions)?;
        file.persist(&target).map(drop).map_err(|error| error.error)
    }

    fn desktop_exists() -> bool {
        let mut folders = Vec::new();
        if let Some(data) = desktop_entry::xdg_home("XDG_DATA_HOME", ".local/share") {
            folders.push(data);
        }
        let dirs = std::env::var_os("XDG_DATA_DIRS")
            .filter(|dirs| !dirs.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        folders.extend(std::env::split_paths(&dirs).filter(|path| path.is_absolute()));
        folders
            .into_iter()
            .any(|folder| folder.join("applications").join(desktop()).exists())
    }

    /// The person's own default applications.
    fn list() -> Option<PathBuf> {
        desktop_entry::xdg_home("XDG_CONFIG_HOME", ".config")
            .map(|config| config.join("mimeapps.list"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const DISCORD: &str = SCHEMES[0];

        struct Folders {
            _folder: tempfile::TempDir,
            list: PathBuf,
            previous: PathBuf,
        }

        fn folders() -> Folders {
            let folder = tempfile::tempdir().unwrap();
            Folders {
                list: folder.path().join("config/mimeapps.list"),
                previous: folder.path().join(PREVIOUS),
                _folder: folder,
            }
        }

        fn read(path: &Path) -> String {
            std::fs::read_to_string(path).unwrap()
        }

        #[test]
        fn claims_and_gives_back_the_lines_as_they_were() {
            let folders = folders();
            let current = "[Added Associations]\n\
                x-scheme-handler/discord=other.desktop;\n\
                \n\
                [Default Applications]\n\
                text/html=browser.desktop;\n\
                x-scheme-handler/discord = discord.desktop;vesktop.desktop;\n\
                \n\
                [Removed Associations]\n";
            desktop_entry::write(&folders.list, current.as_bytes()).unwrap();
            assert!(!enabled_in(&folders.list));
            apply(&folders.list, &folders.previous, true).unwrap();
            assert!(enabled_in(&folders.list));
            let claimed = read(&folders.list);
            let ours = format!("x-scheme-handler/discord={};", desktop());
            assert!(claimed.contains(&format!("text/html=browser.desktop;\n{ours}\n")));
            assert!(
                claimed.contains("[Added Associations]\nx-scheme-handler/discord=other.desktop;")
            );
            assert!(claimed.contains(&format!(
                "x-scheme-handler/yapfast={};\n\n[Removed",
                desktop()
            )));
            // A second claim keeps what the first kept.
            apply(&folders.list, &folders.previous, true).unwrap();
            apply(&folders.list, &folders.previous, false).unwrap();
            assert_eq!(read(&folders.list), current);
            assert!(!enabled_in(&folders.list));
            assert!(!folders.previous.exists());
        }

        #[test]
        fn takes_out_a_line_there_was_none_of() {
            let folders = folders();
            apply(&folders.list, &folders.previous, true).unwrap();
            assert_eq!(
                read(&folders.list),
                format!(
                    "[Default Applications]\nx-scheme-handler/discord={0};\nx-scheme-handler/yapfast={0};\n",
                    desktop()
                )
            );
            apply(&folders.list, &folders.previous, false).unwrap();
            assert_eq!(read(&folders.list), "[Default Applications]\n");
            assert!(!enabled_in(&folders.list));
        }

        #[test]
        fn leaves_a_default_chosen_since() {
            let folders = folders();
            apply(&folders.list, &folders.previous, true).unwrap();
            let chosen = with_line(
                &read(&folders.list),
                DISCORD,
                Some("x-scheme-handler/discord=vesktop.desktop;"),
            );
            desktop_entry::write(&folders.list, chosen.as_bytes()).unwrap();
            apply(&folders.list, &folders.previous, false).unwrap();
            assert_eq!(
                default_line(&read(&folders.list), DISCORD),
                Some("x-scheme-handler/discord=vesktop.desktop;")
            );
        }

        #[test]
        fn writes_through_a_linked_list_and_never_replaces_one() {
            use std::os::unix::fs::PermissionsExt;
            let folders = folders();
            let dotfiles = folders.previous.with_file_name("dotfiles");
            std::fs::create_dir_all(&dotfiles).unwrap();
            std::fs::create_dir_all(folders.list.parent().unwrap()).unwrap();
            let target = dotfiles.join("mimeapps.list");
            std::fs::write(&target, "[Default Applications]\n").unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
            std::os::unix::fs::symlink(&target, &folders.list).unwrap();
            apply(&folders.list, &folders.previous, true).unwrap();
            assert!(
                std::fs::symlink_metadata(&folders.list)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert!(enabled_in(&target));
            assert_eq!(
                std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                0o600
            );

            // As in a read-only store, the setting fails and the list stays.
            let before = read(&target);
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o400)).unwrap();
            assert!(apply(&folders.list, &folders.previous, false).is_err());
            assert_eq!(read(&target), before);
            assert!(
                std::fs::symlink_metadata(&folders.list)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        }
    }
}
