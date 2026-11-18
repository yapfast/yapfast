use std::{io, path::Path};

use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
    System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, REG_VALUE_TYPE, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ,
        RRF_RT_REG_SZ, RegDeleteKeyValueW, RegDeleteKeyW, RegGetValueW, RegSetKeyValueW,
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
            if current.as_deref() != Some(ours.as_str()) && !current.as_deref().is_some_and(is_ours)
            {
                set_string(
                    BACKUP,
                    Some(HAD_COMMAND),
                    if current.is_some() { "1" } else { "0" },
                )?;
                // In its own type: a REG_EXPAND_SZ command comes back as one.
                let (previous, kind) = query_value(COMMAND, None).unwrap_or_default();
                set_value(BACKUP, Some(PREVIOUS_COMMAND), &previous, kind.max(REG_SZ))?;
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
                        let (previous, kind) =
                            query_value(BACKUP, Some(PREVIOUS_COMMAND)).unwrap_or_default();
                        set_value(COMMAND, None, &previous, kind.max(REG_SZ))?;
                    }
                    _ => delete_value(COMMAND, None)?,
                }
                if query_string(BACKUP, Some(HAD_URL_PROTOCOL)).as_deref() == Some("0") {
                    delete_value(PROTOCOL, Some("URL Protocol"))?;
                }
            }
            match unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide(BACKUP).as_ptr()) } {
                ERROR_FILE_NOT_FOUND => Ok(()),
                status => checked(status),
            }
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
/// Yapfast reads no option. The program comes first, quoted as Windows
/// names one.
fn command_line(command: &[String]) -> String {
    let mut line: Vec<String> = command
        .iter()
        .enumerate()
        .map(|(at, argument)| match at {
            0 => crate::autostart::windows_program(argument),
            _ => crate::autostart::windows_argument(argument),
        })
        .collect();
    line.extend([super::OPEN_LINK.to_owned(), "\"%1\"".to_owned()]);
    line.join(" ")
}

/// Whether a registered command runs a Yapfast, wherever it lives,
/// with or without `--open-link`, as earlier builds registered it.
fn is_ours(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    command.contains("yapfast.exe") && command.contains("%1")
}

pub(crate) fn set_string(key: &str, name: Option<&str>, value: &str) -> io::Result<()> {
    set_value(key, name, value, REG_SZ)
}

/// Writes a string value of `kind`, REG_SZ or REG_EXPAND_SZ.
fn set_value(key: &str, name: Option<&str>, value: &str, kind: REG_VALUE_TYPE) -> io::Result<()> {
    let value = wide(value);
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            wide(key).as_ptr(),
            name.map(wide)
                .as_ref()
                .map_or(std::ptr::null(), Vec::as_ptr),
            kind,
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
    query_value(key, name).map(|(value, _)| value)
}

/// [`query_string`], with the value's type.
fn query_value(key: &str, name: Option<&str>) -> Option<(String, REG_VALUE_TYPE)> {
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
    let mut kind = REG_SZ;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name,
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
            &mut kind,
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
    String::from_utf16(&text).ok().map(|text| (text, kind))
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
