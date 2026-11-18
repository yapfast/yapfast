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
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

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
        .enumerate()
        .map(|(at, argument)| match at {
            0 => super::windows_program(argument),
            _ => super::windows_argument(argument),
        })
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
