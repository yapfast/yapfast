//! Starting Yapfast when the person signs in to their computer: a value in the
//! registry's Run key on Windows, an XDG autostart entry on Linux. The entry is
//! the setting, so one turned off in Task Manager, or a deleted file, shows as
//! off in Settings too. The tray mode adds the `--background` argument.

use std::path::PathBuf;

use yapfast_iced::Autostart;

/// Whether this platform can start Yapfast at sign-in.
pub const SUPPORTED: bool = cfg!(any(target_os = "linux", windows));

/// The name the entry goes by: the Run key's value, and the Inno Setup
/// installer's, which writes the same one.
#[cfg(windows)]
const NAME: &str = "Yapfast";

/// How Yapfast starts when the person signs in.
pub fn mode() -> Autostart {
    platform::mode()
}

/// Changes how Yapfast starts at sign-in.
pub fn set(mode: Autostart) -> std::io::Result<()> {
    match mode {
        Autostart::Off => platform::remove(),
        Autostart::Window | Autostart::Tray => platform::write(&command(mode)?),
    }
}

/// Points an entry at this Yapfast, after its folder moved or an update.
pub fn refresh() {
    let mode = mode();
    if mode != Autostart::Off
        && let Err(error) = set(mode)
    {
        tracing::warn!(%error, "could not update starting at sign-in");
    }
}

/// What starts Yapfast with the data folder this one uses.
fn command(mode: Autostart) -> std::io::Result<Vec<String>> {
    let program = std::env::current_exe()?;
    #[cfg(target_os = "linux")]
    let program = crate::desktop_entry::appimage(&program).unwrap_or(program);
    let mut command = vec![path_text(program)?];
    if mode == Autostart::Tray {
        command.push("--background".to_owned());
    }
    let mut arguments = std::env::args_os().skip(1);
    if arguments.any(|argument| argument == "--data-dir")
        && let Some(folder) = arguments.next()
    {
        let folder = std::path::absolute(folder)?;
        command.extend(["--data-dir".to_owned(), path_text(folder)?]);
    }
    Ok(command)
}

/// The startup entries we write put the optional tray flag immediately
/// after the executable and before any data-folder argument.
#[cfg(any(target_os = "linux", windows))]
fn command_mode(command: &str) -> Autostart {
    let command = command.trim_start();
    let arguments = if let Some(quoted) = command.strip_prefix('"') {
        let mut escaped = false;
        let end = quoted.find(|character| {
            if escaped {
                escaped = false;
                return false;
            }
            escaped = character == '\\';
            character == '"'
        });
        end.map_or("", |end| &quoted[end + 1..])
    } else {
        command
            .find(char::is_whitespace)
            .map_or("", |end| &command[end..])
    };
    match arguments.split_whitespace().next() {
        Some("--background" | "\"--background\"") => Autostart::Tray,
        _ => Autostart::Window,
    }
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
    windows_quoted(argument)
}

/// The program a command line runs, always quoted as Windows registers one:
/// a protocol handler's, and the `Run` key's, name it that way whatever the
/// path holds, so a path that later gains a space still runs that program
/// rather than a longer name of its own.
#[cfg(any(windows, test))]
pub(crate) fn windows_program(program: &str) -> String {
    windows_quoted(program)
}

/// `argument` quoted and escaped as Windows splits a command line.
#[cfg(any(windows, test))]
fn windows_quoted(argument: &str) -> String {
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

    use super::{Autostart, command_mode};
    use crate::desktop_entry;

    pub fn mode() -> Autostart {
        config().map_or(Autostart::Off, |config| entry_mode(&config))
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
    fn entry_mode(config: &Path) -> Autostart {
        let Ok(entry) = std::fs::read_to_string(entry(config)) else {
            return Autostart::Off;
        };
        if entry.lines().any(|line| {
            matches!(
                line.replace(' ', "").as_str(),
                "Hidden=true" | "X-GNOME-Autostart-enabled=false"
            )
        }) {
            return Autostart::Off;
        }
        entry
            .lines()
            .find_map(|line| {
                let (key, command) = line.split_once('=')?;
                (key.trim() == "Exec").then(|| command_mode(command))
            })
            .unwrap_or(Autostart::Off)
    }

    pub(super) fn write_entry(config: &Path, command: &[String]) -> std::io::Result<()> {
        let exec: Vec<String> = command
            .iter()
            .map(|argument| desktop_entry::quoted(argument))
            .collect();
        let text = format!(
            "[Desktop Entry]\nType=Application\nName=Yapfast\n\
             Comment=Starts Yapfast when you sign in\n\
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
            assert_eq!(entry_mode(config.path()), Autostart::Off);
            let command = [
                "/opt/Yapfast Builds/yapfast".to_owned(),
                "--background".to_owned(),
                "--data-dir".to_owned(),
                "/home/a/100% $data".to_owned(),
            ];
            write_entry(config.path(), &command).unwrap();
            assert_eq!(entry_mode(config.path()), Autostart::Tray);
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
            assert_eq!(entry_mode(config.path()), Autostart::Window);
            let path = entry(config.path());
            let text = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, text.replace("enabled=true", "enabled=false")).unwrap();
            assert_eq!(entry_mode(config.path()), Autostart::Off);
            std::fs::write(&path, "[Desktop Entry]\nHidden=true\n").unwrap();
            assert_eq!(entry_mode(config.path()), Autostart::Off);
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::io;

    use super::{Autostart, command_mode};
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

    pub fn mode() -> Autostart {
        let Some(run) = query(RUN, RRF_RT_REG_SZ) else {
            return Autostart::Off;
        };
        // Its first byte is odd while Task Manager has the entry turned off.
        let off = query(APPROVED, RRF_RT_REG_BINARY)
            .is_some_and(|approved| approved.first().is_some_and(|byte| byte & 1 == 1));
        if off {
            return Autostart::Off;
        }
        let command: Vec<u16> = run
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .take_while(|unit| *unit != 0)
            .collect();
        command_mode(&String::from_utf16_lossy(&command))
    }

    pub fn write(command: &[String]) -> io::Result<()> {
        let line: Vec<String> = command
            .iter()
            .enumerate()
            .map(|(at, argument)| match at {
                0 => super::windows_program(argument),
                _ => super::windows_argument(argument),
            })
            .collect();
        let value = wide(&line.join(" "));
        // SAFETY: the key and name are NUL-terminated, and value holds the stated byte count.
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
        // SAFETY: both NUL-terminated strings live through the registry call.
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
        // SAFETY: the strings are NUL-terminated and size is writable. Null data
        // requests the required buffer size.
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
        // SAFETY: the strings are NUL-terminated and data is writable for size bytes.
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
            assert_eq!(mode(), Autostart::Off);
            write(&[
                r"C:\Program Files\Yapfast\yapfast.exe".into(),
                "--background".into(),
            ])
            .unwrap();
            assert_eq!(mode(), Autostart::Tray);
            let value = query(RUN, RRF_RT_REG_SZ).unwrap();
            let text: Vec<u16> = value
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|&unit| unit != 0)
                .collect();
            assert_eq!(
                String::from_utf16(&text).unwrap(),
                r#""C:\Program Files\Yapfast\yapfast.exe" --background"#
            );
            // Task Manager turns it off with an odd first byte.
            let disabled = [3u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            // SAFETY: the key and name are NUL-terminated, and disabled holds the stated byte count.
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
            assert_eq!(mode(), Autostart::Off);
            write(&["yapfast.exe".into()]).unwrap();
            assert_eq!(mode(), Autostart::Window);
            remove().unwrap();
            assert_eq!(mode(), Autostart::Off);
            assert!(query(APPROVED, RRF_RT_REG_BINARY).is_none());
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod none {
    use super::Autostart;

    pub fn mode() -> Autostart {
        Autostart::Off
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
    use super::{windows_argument, windows_program};

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn reads_the_startup_mode_from_the_flag_after_the_program() {
        use super::{Autostart, command_mode};

        assert_eq!(
            command_mode(r#""C:\Program Files\Yapfast\yapfast.exe" --background"#),
            Autostart::Tray,
        );
        assert_eq!(
            command_mode(r#""/opt/Yapfast \"local\"/yapfast" --background --data-dir /tmp/data"#),
            Autostart::Tray,
        );
        assert_eq!(
            command_mode(r#"yapfast --data-dir "/tmp/ --background files""#),
            Autostart::Window,
        );
    }

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

    #[test]
    fn quotes_a_program_whether_or_not_it_needs_it() {
        assert_eq!(
            windows_program(r"C:\Apps\Yapfast\yapfast.exe"),
            r#""C:\Apps\Yapfast\yapfast.exe""#
        );
        assert_eq!(
            windows_program(r"C:\A B\yapfast.exe"),
            r#""C:\A B\yapfast.exe""#
        );
    }
}
