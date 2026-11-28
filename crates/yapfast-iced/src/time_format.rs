//! The interface clock's hour cycle and the system preference used by Auto.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum TimeFormat {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "12-hour")]
    Hour12,
    #[serde(rename = "24-hour")]
    Hour24,
}

impl TimeFormat {
    pub fn twelve_hour(self) -> bool {
        static SYSTEM_TWELVE_HOUR: OnceLock<bool> = OnceLock::new();
        match self {
            Self::Auto => *SYSTEM_TWELVE_HOUR.get_or_init(|| system_twelve_hour().unwrap_or(false)),
            Self::Hour12 => true,
            Self::Hour24 => false,
        }
    }
}

#[cfg(windows)]
fn system_twelve_hour() -> Option<bool> {
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SSHORTTIME};

    let mut pattern = [0u16; 80];
    // SAFETY: A null locale name requests the user's regional settings.
    // The output points to a writable buffer of the stated size.
    let count = unsafe {
        GetLocaleInfoEx(
            std::ptr::null(),
            LOCALE_SSHORTTIME,
            pattern.as_mut_ptr(),
            pattern.len() as i32,
        )
    };
    if count <= 1 {
        return None;
    }
    windows_hour_cycle(&String::from_utf16(&pattern[..count as usize - 1]).ok()?)
}

#[cfg(any(windows, test))]
fn windows_hour_cycle(pattern: &str) -> Option<bool> {
    let mut quoted = false;
    for part in pattern.bytes() {
        match part {
            b'\'' => quoted = !quoted,
            b'h' if !quoted => return Some(true),
            b'H' if !quoted => return Some(false),
            _ => {}
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn system_twelve_hour() -> Option<bool> {
    // SAFETY: The empty name selects the environment's LC_TIME. A null
    // base creates a private locale without changing the process locale.
    let locale = unsafe { libc::newlocale(libc::LC_TIME_MASK, c"".as_ptr(), std::ptr::null_mut()) };
    if locale.is_null() {
        return None;
    }
    // SAFETY: Read T_FMT while the private locale is alive, then free it.
    unsafe {
        let pattern = libc::nl_langinfo_l(libc::T_FMT, locale);
        let cycle = (!pattern.is_null())
            .then(|| posix_hour_cycle(std::ffi::CStr::from_ptr(pattern).to_bytes()))
            .flatten();
        libc::freelocale(locale);
        cycle
    }
}

#[cfg(any(target_os = "linux", test))]
fn posix_hour_cycle(pattern: &[u8]) -> Option<bool> {
    let mut parts = pattern.iter().copied();
    while let Some(part) = parts.next() {
        if part != b'%' {
            continue;
        }
        let mut specifier = parts.next()?;
        while matches!(
            specifier,
            b'E' | b'O' | b'-' | b'_' | b'0' | b'^' | b'#' | b'1'..=b'9'
        ) {
            specifier = parts.next()?;
        }
        match specifier {
            b'I' | b'l' | b'r' => return Some(true),
            b'H' | b'k' | b'R' | b'T' => return Some(false),
            _ => {}
        }
    }
    None
}

#[cfg(not(any(windows, target_os = "linux")))]
fn system_twelve_hour() -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_format_recognizes_system_hour_patterns() {
        assert_eq!(windows_hour_cycle("h:mm tt"), Some(true));
        assert_eq!(windows_hour_cycle("'h' HH:mm"), Some(false));
        assert_eq!(windows_hour_cycle("'hour' mm"), None);
        assert_eq!(posix_hour_cycle(b"%r"), Some(true));
        assert_eq!(posix_hour_cycle(b"%OI:%M %p"), Some(true));
        assert_eq!(posix_hour_cycle(b"%%I %T"), Some(false));
        assert_eq!(posix_hour_cycle(b"%H:%M:%S"), Some(false));
        assert_eq!(posix_hour_cycle(b"%M"), None);
    }

    #[test]
    fn time_format_preserves_legacy_explicit_hour_cycles() {
        use crate::layout::Layout;
        for (source, expected) in [
            ("{}", TimeFormat::Auto),
            (r#"{"twelve_hour_clock":true}"#, TimeFormat::Hour12),
            (r#"{"twelve_hour_clock":false}"#, TimeFormat::Hour24),
            (
                r#"{"twelve_hour_clock":true,"time_format":"auto"}"#,
                TimeFormat::Auto,
            ),
            (
                r#"{"twelve_hour_clock":true,"time_format":"24-hour"}"#,
                TimeFormat::Hour24,
            ),
        ] {
            let layout: Layout = serde_json::from_str(source).unwrap();
            assert_eq!(layout.prefs.time_format(), expected);
        }
    }
}
