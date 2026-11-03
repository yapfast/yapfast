//! Starting Yapfast when the person signs in to their computer: a value in the
//! registry's Run key on Windows, an XDG autostart entry on Linux. The entry is
//! the setting, so one turned off in Task Manager, or a deleted file, shows as
//! off in Settings too. It starts Yapfast with `--background`, in the tray.

use std::path::PathBuf;

/// Whether this platform can start Yapfast at sign-in.
pub const SUPPORTED: bool = cfg!(any(target_os = "linux", windows));

/// The name the entry goes by: the Run key's value, and the Inno Setup
/// installer's, which writes the same one.
#[cfg(windows)]
const NAME: &str = "Yapfast";

/// Whether Yapfast starts when the person signs in.
pub fn enabled() -> bool {
    platform::enabled()
}

/// Starts Yapfast at sign-in, or stops it.
pub fn set(on: bool) -> std::io::Result<()> {
    match on {
        true => platform::write(&command()?),
        false => platform::remove(),
    }
}

/// Points an entry at this Yapfast, after its folder moved or an update.
pub fn refresh() {
    if enabled()
        && let Err(error) = set(true)
    {
        tracing::warn!(%error, "could not update starting at sign-in");
    }
}

/// What starts Yapfast in the tray, with the data folder this one uses.
fn command() -> std::io::Result<Vec<String>> {
    let program = std::env::current_exe()?;
    #[cfg(target_os = "linux")]
    let program = crate::desktop_entry::appimage(&program).unwrap_or(program);
    let mut command = vec![path_text(program)?, "--background".to_owned()];
    let arguments: Vec<String> = std::env::args().collect();
    if let Some(index) = arguments
        .iter()
        .position(|argument| argument == "--data-dir")
        && let Some(folder) = arguments.get(index + 1)
    {
        let folder = std::path::absolute(folder)?;
        command.extend(["--data-dir".to_owned(), path_text(folder)?]);
    }
    Ok(command)
}

pub(crate) fn path_text(path: PathBuf) -> std::io::Result<String> {
    path.into_os_string()
        .into_string()
        .map_err(|path| std::io::Error::other(format!("{} isn't valid Unicode", path.display())))
}

/// One argument of the command line, quoted when it holds a space, as
/// Windows splits a command line into arguments.
#[cfg(any(windows, test))]
pub(crate) fn windows_argument(argument: &str) -> String {
    if !argument.is_empty() && !argument.contains([' ', '\t', '"']) {
        return argument.to_owned();
    }
    // Backslashes count only before a quote, where each is doubled.
    let mut quoted = String::from('"');
    let mut backslashes = 0;
    for character in argument.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.extend(std::iter::repeat_n('\\', backslashes));
                quoted.push(character);
                backslashes = 0;
            }
        }
    }
    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(any(target_os = "linux", windows)))]
use none as platform;
#[cfg(windows)]
use windows as platform;

#[cfg(target_os = "linux")]
mod linux {
    use std::path::{Path, PathBuf};

    use crate::desktop_entry;

    pub fn enabled() -> bool {
        config().is_some_and(|config| entry_enabled(&config))
    }

    pub fn write(command: &[String]) -> std::io::Result<()> {
        let config = config().ok_or_else(|| std::io::Error::other("no home folder"))?;
        write_entry(&config, command)
    }

    pub fn remove() -> std::io::Result<()> {
        match config() {
            Some(config) => match std::fs::remove_file(entry(&config)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                result => result,
            },
            None => Ok(()),
        }
    }

    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    fn config() -> Option<PathBuf> {
        desktop_entry::xdg_home("XDG_CONFIG_HOME", ".config")
    }

    fn entry(config: &Path) -> PathBuf {
        config
            .join("autostart")
            .join(format!("{}.desktop", crate::icon::APP_ID))
    }

    /// The entry is there and not turned off, as desktops turn one off
    /// without deleting it.
    pub(super) fn entry_enabled(config: &Path) -> bool {
        std::fs::read_to_string(entry(config)).is_ok_and(|entry| {
            !entry.lines().any(|line| {
                matches!(
                    line.replace(' ', "").as_str(),
                    "Hidden=true" | "X-GNOME-Autostart-enabled=false"
                )
            })
        })
    }

    pub(super) fn write_entry(config: &Path, command: &[String]) -> std::io::Result<()> {
        let exec: Vec<String> = command
            .iter()
            .map(|argument| desktop_entry::quoted(argument))
            .collect();
        let text = format!(
            "[Desktop Entry]\nType=Application\nName=Yapfast\n\
             Comment=Starts Yapfast in the system tray when you sign in\n\
             Exec={}\nIcon={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            exec.join(" "),
            crate::icon::APP_ID,
        );
        desktop_entry::write(&entry(config), text.as_bytes())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn writes_an_entry_that_starts_in_the_tray() {
            let config = tempfile::tempdir().unwrap();
            assert!(!entry_enabled(config.path()));
            let command = [
                "/opt/Yapfast Builds/yapfast".to_owned(),
                "--background".to_owned(),
                "--data-dir".to_owned(),
                "/home/a/100% $data".to_owned(),
            ];
            write_entry(config.path(), &command).unwrap();
            assert!(entry_enabled(config.path()));
            let text = std::fs::read_to_string(entry(config.path())).unwrap();
            assert!(text.contains(
                r#"Exec="/opt/Yapfast Builds/yapfast" --background --data-dir "/home/a/100%% \$data""#
            ));
            assert!(text.contains("Name=Yapfast\n"));
        }

        #[test]
        fn an_entry_a_desktop_turned_off_is_off() {
            let config = tempfile::tempdir().unwrap();
            write_entry(config.path(), &["yapfast".to_owned()]).unwrap();
            let path = entry(config.path());
            let text = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, text.replace("enabled=true", "enabled=false")).unwrap();
            assert!(!entry_enabled(config.path()));
            std::fs::write(&path, "[Desktop Entry]\nHidden=true\n").unwrap();
            assert!(!entry_enabled(config.path()));
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::io;

    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
        System::Registry::{
            HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegDeleteKeyValueW,
            RegGetValueW, RegSetKeyValueW,
        },
    };

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Where Task Manager's Startup apps page keeps an entry turned off.
    const APPROVED: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    pub fn enabled() -> bool {
        let run = query(RUN, RRF_RT_REG_SZ);
        // Its first byte is odd while Task Manager has the entry turned off.
        let off = query(APPROVED, RRF_RT_REG_BINARY)
            .is_some_and(|approved| approved.first().is_some_and(|byte| byte & 1 == 1));
        run.is_some() && !off
    }

    pub fn write(command: &[String]) -> io::Result<()> {
        let line: Vec<String> = command
            .iter()
            .map(|argument| super::windows_argument(argument))
            .collect();
        let value = wide(&line.join(" "));
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                wide(RUN).as_ptr(),
                wide(super::NAME).as_ptr(),
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        };
        checked(status)?;
        // Turning it on here turns it on in Task Manager too.
        delete(APPROVED)
    }

    pub fn remove() -> io::Result<()> {
        delete(RUN)?;
        delete(APPROVED)
    }

    fn delete(key: &str) -> io::Result<()> {
        let status = unsafe {
            RegDeleteKeyValueW(
                HKEY_CURRENT_USER,
                wide(key).as_ptr(),
                wide(super::NAME).as_ptr(),
            )
        };
        match status {
            ERROR_FILE_NOT_FOUND => Ok(()),
            status => checked(status),
        }
    }

    /// The entry's value under `key`, if it has one of the `kind` asked for.
    fn query(key: &str, kind: u32) -> Option<Vec<u8>> {
        let (key, name) = (wide(key), wide(super::NAME));
        let mut size = 0u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                kind,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                kind,
                std::ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        (status == ERROR_SUCCESS).then(|| {
            data.truncate(size as usize);
            data
        })
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

    /// Writes the current user's registry: run it under Wine with a
    /// throwaway WINEPREFIX, not on a Windows account.
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        #[ignore = "writes the current user's Run key"]
        fn turns_on_and_off_as_task_manager_sees_it() {
            remove().unwrap();
            assert!(!enabled());
            write(&[
                r"C:\Program Files\Yapfast\yapfast.exe".into(),
                "--background".into(),
            ])
            .unwrap();
            assert!(enabled());
            let value = query(RUN, RRF_RT_REG_SZ).unwrap();
            let text: Vec<u16> = value
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|&unit| unit != 0)
                .collect();
            assert_eq!(
                String::from_utf16(&text).unwrap(),
                r#""C:\Program Files\Yapfast\yapfast.exe" --background"#
            );
            // Task Manager turns it off with an odd first byte.
            let disabled = [3u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let status = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    wide(APPROVED).as_ptr(),
                    wide(super::super::NAME).as_ptr(),
                    windows_sys::Win32::System::Registry::REG_BINARY,
                    disabled.as_ptr().cast(),
                    disabled.len() as u32,
                )
            };
            checked(status).unwrap();
            assert!(!enabled());
            write(&["yapfast.exe".into()]).unwrap();
            assert!(enabled(), "turning it on here turns it on in Task Manager");
            remove().unwrap();
            assert!(!enabled());
            assert!(query(APPROVED, RRF_RT_REG_BINARY).is_none());
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod none {
    pub fn enabled() -> bool {
        false
    }

    pub fn write(_: &[String]) -> std::io::Result<()> {
        Err(std::io::Error::other(
            "starting at sign-in isn't supported here",
        ))
    }

    pub fn remove() -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::windows_argument;

    #[test]
    fn quotes_windows_arguments_as_windows_splits_them() {
        assert_eq!(windows_argument("--background"), "--background");
        assert_eq!(
            windows_argument(r"C:\Users\A B\Yapfast\yapfast.exe"),
            r#""C:\Users\A B\Yapfast\yapfast.exe""#
        );
        assert_eq!(windows_argument(r"C:\A B\"), r#""C:\A B\\""#);
        assert_eq!(windows_argument(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(windows_argument(""), r#""""#);
    }
}
