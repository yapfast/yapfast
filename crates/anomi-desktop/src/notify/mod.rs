//! Desktop notifications. On Linux they go to the freedesktop notification service,
//! which GNOME, KDE and the other desktops run; on Windows they're toasts, which stay
//! in the notification center. Clicking one opens its conversation: the client hears
//! which, from the notifier's thread, and takes it onto its own event loop.

use std::path::Path;
use std::sync::Arc;

use yapfast_core::chat::Alert;

/// How a notification shows.
#[derive(Clone, Copy)]
pub struct Style {
    /// Say what the message says, rather than only who wrote.
    #[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
    pub text: bool,
}

/// A notification someone clicked: the conversation it's from, and on Wayland the
/// token that lets the window come forward, when the notification service gave one.
#[derive(Clone, Debug)]
pub struct Clicked {
    pub room: String,
    pub token: Option<String>,
}

/// What hears the clicks, on the notifier's threads.
pub type Opened = Arc<dyn Fn(Clicked) + Send + Sync>;

/// Shows notifications from a thread of its own, so a slow notification service never
/// holds up the window.
pub struct Notifier {
    #[cfg(any(target_os = "linux", windows))]
    requests: std::sync::mpsc::Sender<(Alert, Style)>,
}

impl Notifier {
    /// Starts showing notifications. `opened` hears, from a thread of the notifier's,
    /// the conversation of a notification someone clicked. Windows keeps the pictures
    /// a notification shows under `root`.
    pub fn start(opened: Opened, root: &Path) -> Self {
        #[cfg(any(target_os = "linux", windows))]
        {
            let (requests, alerts) = std::sync::mpsc::channel();
            #[cfg(target_os = "linux")]
            let _ = root;
            #[cfg(windows)]
            let root = root.to_owned();
            let started = std::thread::Builder::new()
                .name("yapfast-notify".into())
                .spawn(move || {
                    #[cfg(target_os = "linux")]
                    linux::run(alerts, opened);
                    #[cfg(windows)]
                    windows::run(alerts, opened, &root);
                });
            if let Err(error) = started {
                tracing::warn!(target: crate::LOG, %error, "could not start desktop notifications");
            }
            Self { requests }
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (opened, root);
            Self {}
        }
    }

    pub fn show(&self, alert: Alert, style: Style) {
        #[cfg(any(target_os = "linux", windows))]
        let _ = self.requests.send((alert, style));
        #[cfg(not(any(target_os = "linux", windows)))]
        let _ = (alert, style);
    }
}

/// The body of a notification: the message, or for privacy only that there is one.
/// A call's says only that it's ringing.
#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
fn body(alert: &Alert, style: Style) -> String {
    if style.text || alert.call {
        alert.body.clone()
    } else {
        yapfast_lang::tr!("New message").into()
    }
}

#[cfg(target_os = "linux")]
mod linux;

/// How many toasts a click still reaches Yapfast from: the notification center keeps
/// as many of an app's.
#[cfg(windows)]
const KEPT: usize = 20;

/// The 256-pixel picture in Yapfast's icon file, which is stored as a PNG.
#[cfg(any(windows, test))]
fn icon_png() -> Option<&'static [u8]> {
    const ICON: &[u8] = include_bytes!("../../../../packaging/windows/yapfast.ico");
    let count = u16::from_le_bytes(ICON.get(4..6)?.try_into().ok()?);
    (0..usize::from(count)).find_map(|index| {
        let entry = ICON.get(6 + index * 16..)?.get(..16)?;
        // A width of 0 is 256.
        if entry[0] != 0 {
            return None;
        }
        let size = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
        let start = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
        ICON.get(start..start.checked_add(size)?)
    })
}

/// A Windows path as a `file:` URI, which a toast takes its pictures by.
#[cfg(any(windows, test))]
fn file_uri(path: &str) -> String {
    let mut uri = String::from("file:///");
    for byte in path.bytes() {
        match byte {
            b'\\' => uri.push('/'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                uri.push(char::from(byte))
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

/// Text for a toast's XML, without the control characters XML can't hold.
#[cfg(any(windows, test))]
fn xml_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\t' | '\n' | '\r' => escaped.push(c),
            c if c < ' ' => {}
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(windows)]
mod windows;

#[cfg(test)]
mod tests {
    #[test]
    fn the_icon_has_a_png_picture() {
        let png = super::icon_png().unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn a_path_becomes_a_file_uri() {
        assert_eq!(
            super::file_uri(r"C:\Users\Zoë K\AppData\Local\0.png"),
            "file:///C:/Users/Zo%C3%AB%20K/AppData/Local/0.png"
        );
    }

    #[test]
    fn toast_text_is_escaped() {
        assert_eq!(
            super::xml_text("<b>\"Tom & Jerry\"\u{1}\n"),
            "&lt;b&gt;&quot;Tom &amp; Jerry&quot;\n"
        );
    }
}
