//! Opening Discord's own protocol links in Yapfast: a current-user
//! protocol registration on Windows, and the person's default
//! applications on Linux. Yapfast's own `yapfast:` scheme is registered by
//! its installer or desktop entry; this claims `discord:` only when
//! Settings says to, and gives the previous handler back when turned off.

use std::path::Path;
#[cfg(any(windows, target_os = "linux"))]
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

    /// The registered command line, with the link as its last argument.
    fn command_line(command: &[String]) -> String {
        let mut line: Vec<String> = command
            .iter()
            .map(|argument| crate::autostart::windows_argument(argument))
            .collect();
        line.push("\"%1\"".to_owned());
        line.join(" ")
    }

    /// Whether a registered command runs a Yapfast, wherever it lives.
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
            assert!(is_ours(r#""C:\Apps\Yapfast\yapfast.exe" "%1""#));
            assert!(!is_ours(r#""C:\Apps\Discord\Discord.exe" "%1""#));
            assert_eq!(
                command_line(&[r"C:\Apps\Yapfast\yapfast.exe".into()]),
                r#""C:\Apps\Yapfast\yapfast.exe" "%1""#
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
    use std::{
        io,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };

    use crate::desktop_entry;

    const DISCORD: &str = "x-scheme-handler/discord";
    const YAPFAST: &str = "x-scheme-handler/yapfast";
    const PREVIOUS: &str = "discord-protocol-previous";

    fn desktop() -> String {
        format!("{}.desktop", crate::icon::APP_ID)
    }

    pub fn enabled(_: &Path) -> bool {
        query_default(DISCORD).as_deref() == Some(desktop().as_str())
    }

    pub fn set(root: &Path, enabled: bool) -> io::Result<()> {
        let config = config().ok_or_else(|| io::Error::other("no home folder"))?;
        match enabled {
            true => {
                desktop_entry::install();
                if !desktop_exists() {
                    return Err(io::Error::other("Yapfast's desktop entry isn't installed"));
                }
                let previous = root.join(PREVIOUS);
                if !previous.exists() {
                    let current = [query_default(DISCORD), query_default(YAPFAST)];
                    if current
                        .iter()
                        .any(|current| current.as_deref() != Some(&desktop()))
                    {
                        write_previous(&previous, &current)?;
                    }
                }
                set_default(&config, DISCORD, Some(&desktop()))?;
                set_default(&config, YAPFAST, Some(&desktop()))
            }
            false => {
                let previous = read_previous(&root.join(PREVIOUS));
                if query_default(DISCORD).as_deref() == Some(desktop().as_str()) {
                    set_default(&config, DISCORD, previous[0].as_deref())?;
                }
                if query_default(YAPFAST).as_deref() == Some(desktop().as_str()) {
                    set_default(&config, YAPFAST, previous[1].as_deref())?;
                }
                match std::fs::remove_file(root.join(PREVIOUS)) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    result => result,
                }
            }
        }
    }

    pub fn refresh(_: &Path) -> io::Result<()> {
        // A package or the AppImage entry itself names the current
        // Yapfast; the default names only that entry.
        Ok(())
    }

    /// The desktop file a scheme opens, as xdg-mime reads every
    /// mimeapps.list in turn.
    fn query_default(scheme: &str) -> Option<String> {
        let output = Command::new("xdg-mime")
            .args(["query", "default", scheme])
            .env_remove("LD_LIBRARY_PATH")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        let value = text.trim().split(';').next()?.trim();
        (!value.is_empty()).then(|| value.to_owned())
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

    fn write_previous(path: &Path, previous: &[Option<String>; 2]) -> io::Result<()> {
        let text = format!(
            "{}\n{}\n",
            previous[0].as_deref().unwrap_or_default(),
            previous[1].as_deref().unwrap_or_default()
        );
        desktop_entry::write(path, text.as_bytes())
    }

    fn read_previous(path: &Path) -> [Option<String>; 2] {
        let Ok(text) = std::fs::read_to_string(path) else {
            return [None, None];
        };
        let mut lines = text.lines();
        let value = |lines: &mut std::str::Lines<'_>| {
            lines
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        [value(&mut lines), value(&mut lines)]
    }

    fn set_default(config: &Path, scheme: &str, desktop: Option<&str>) -> io::Result<()> {
        let path = config.join("mimeapps.list");
        let current = match std::fs::read_to_string(&path) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let text = with_default(current.as_deref(), scheme, desktop);
        desktop_entry::write(&path, text.as_bytes())
    }

    /// A mimeapps.list with `scheme`'s default set or taken out, its other
    /// sections and lines as they were.
    fn with_default(current: Option<&str>, scheme: &str, desktop: Option<&str>) -> String {
        let key = format!("{scheme}=");
        let mut lines: Vec<String> = current
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect();
        let start = lines
            .iter()
            .position(|line| line.trim() == "[Default Applications]");
        let Some(start) = start else {
            if let Some(desktop) = desktop {
                if !lines.is_empty() {
                    lines.push(String::new());
                }
                lines.push("[Default Applications]".to_owned());
                lines.push(format!("{key}{desktop};"));
            }
            return finish(lines);
        };
        let end = lines[start + 1..]
            .iter()
            .position(|line| line.trim_start().starts_with('['))
            .map_or(lines.len(), |next| start + 1 + next);
        if let Some(index) = lines[start + 1..end]
            .iter()
            .position(|line| line.trim_start().starts_with(&key))
        {
            let index = start + 1 + index;
            match desktop {
                Some(desktop) => lines[index] = format!("{key}{desktop};"),
                None => {
                    lines.remove(index);
                }
            }
        } else if let Some(desktop) = desktop {
            lines.insert(end, format!("{key}{desktop};"));
        }
        finish(lines)
    }

    fn finish(mut lines: Vec<String>) -> String {
        if lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
        lines.join("\n")
    }

    fn config() -> Option<PathBuf> {
        desktop_entry::xdg_home("XDG_CONFIG_HOME", ".config")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn sets_and_restores_a_default_without_touching_the_rest() {
            let current = "[Added Associations]\n\
                x-scheme-handler/discord=other.desktop;\n\
                [Default Applications]\n\
                text/html=browser.desktop;\n\
                x-scheme-handler/discord=discord.desktop;\n";
            let claimed = with_default(Some(current), DISCORD, Some("yapfast.desktop"));
            assert!(
                claimed.contains("[Added Associations]\nx-scheme-handler/discord=other.desktop;")
            );
            assert!(claimed.contains("text/html=browser.desktop;"));
            assert!(claimed.contains("x-scheme-handler/discord=yapfast.desktop;"));
            let restored = with_default(Some(&claimed), DISCORD, Some("discord.desktop"));
            assert_eq!(restored, current);
            let removed = with_default(Some(current), DISCORD, None);
            assert!(!removed.contains(
                "[Default Applications]\ntext/html=browser.desktop;\nx-scheme-handler/discord="
            ));
        }

        #[test]
        fn makes_a_default_applications_section_when_there_is_none() {
            assert_eq!(
                with_default(None, DISCORD, Some("yapfast.desktop")),
                "[Default Applications]\nx-scheme-handler/discord=yapfast.desktop;\n"
            );
            assert_eq!(with_default(None, DISCORD, None), "");
        }

        #[test]
        fn previous_handlers_round_trip() {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join(PREVIOUS);
            let previous = [Some("discord.desktop".to_owned()), None];
            write_previous(&path, &previous).unwrap();
            assert_eq!(read_previous(&path), previous);
        }
    }
}
