//! Whether the desktop asks apps for less motion, for Accessibility's Sync
//! with computer setting: what Chromium reads for `prefers-reduced-motion`,
//! as Discord's client follows it. On Linux, the desktop portal's settings:
//! GNOME's (and GTK's) `enable-animations`, then KDE's animation speed, off
//! at 0; on Windows, the system's "Show animations in Windows". Elsewhere,
//! or where the desktop doesn't say, none.

/// Whether the desktop asks for reduced motion. It may wait on D-Bus.
pub fn reduced() -> Option<bool> {
    #[cfg(target_os = "linux")]
    return linux::reduced();
    #[cfg(windows)]
    return windows::reduced();
    #[cfg(not(any(target_os = "linux", windows)))]
    None
}

#[cfg(target_os = "linux")]
mod linux {
    use zbus::{
        blocking::Connection,
        zvariant::{OwnedValue, Value},
    };

    const PORTAL: &str = "org.freedesktop.portal.Desktop";
    const PATH: &str = "/org/freedesktop/portal/desktop";
    const SETTINGS: &str = "org.freedesktop.portal.Settings";

    pub fn reduced() -> Option<bool> {
        let connection = match Connection::session() {
            Ok(connection) => connection,
            Err(error) => {
                tracing::debug!(%error, "no session bus to read reduced motion from");
                return None;
            }
        };
        if let Some(on) = read(
            &connection,
            "org.gnome.desktop.interface",
            "enable-animations",
        )
        .and_then(|value| boolean(&value))
        {
            return Some(!on);
        }
        read(
            &connection,
            "org.kde.kdeglobals.KDE",
            "AnimationDurationFactor",
        )
        .and_then(|value| number(&value))
        .map(|factor| factor == 0.0)
    }

    /// One setting from the portal: `ReadOne`, or `Read` on portals older
    /// than its version 2, which wraps the value once more.
    fn read(connection: &Connection, namespace: &str, key: &str) -> Option<OwnedValue> {
        ["ReadOne", "Read"].into_iter().find_map(|method| {
            let reply = connection
                .call_method(
                    Some(PORTAL),
                    PATH,
                    Some(SETTINGS),
                    method,
                    &(namespace, key),
                )
                .ok()?;
            reply.body().deserialize().ok()
        })
    }

    fn boolean(value: &Value<'_>) -> Option<bool> {
        match value {
            Value::Bool(on) => Some(*on),
            Value::Value(inner) => boolean(inner),
            _ => None,
        }
    }

    /// KDE's portal gives its settings as they're written, in text.
    fn number(value: &Value<'_>) -> Option<f64> {
        match value {
            Value::F64(number) => Some(*number),
            Value::I32(number) => Some(f64::from(*number)),
            Value::Str(text) => text.as_str().trim().parse().ok(),
            Value::Value(inner) => number(inner),
            _ => None,
        }
    }
}

#[cfg(windows)]
mod windows {
    use windows_sys::{
        Win32::UI::WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW},
        core::BOOL,
    };

    pub fn reduced() -> Option<bool> {
        let mut on: BOOL = 0;
        // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the pointer,
        // which points at `on` for the call.
        let read = unsafe {
            SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut on).cast(), 0)
        };
        (read != 0).then_some(on == 0)
    }
}
