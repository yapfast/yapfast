//! Desktop notifications. On Linux they go to the freedesktop notification service,
//! which GNOME, KDE and the other desktops run; on Windows they're toasts, which stay
//! in the notification center. Clicking one opens its conversation.

use std::path::Path;

use yapfast_core::chat::Alert;
use yapfast_core::i18n::tr;

/// How a notification shows.
#[derive(Clone, Copy)]
pub struct Style {
    /// Say what the message says, rather than only who wrote.
    #[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
    pub text: bool,
}

/// Shows notifications from a thread of its own, so a slow notification service never
/// holds up the window.
pub struct Notifier {
    #[cfg(any(target_os = "linux", windows))]
    requests: std::sync::mpsc::Sender<(Alert, Style)>,
}

impl Notifier {
    /// Starts showing notifications. `opened` runs on Slint's event loop with the
    /// conversation of a notification someone clicked. Windows keeps the pictures a
    /// notification shows under `root`.
    pub fn start(opened: fn(String), root: &Path) -> Self {
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
                tracing::warn!(%error, "could not start desktop notifications");
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
        tr("New message").into()
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex, mpsc::Receiver},
    };

    use yapfast_core::chat::Alert;
    use zbus::{
        blocking::{Connection, MessageIterator},
        zvariant::{StructureBuilder, Value},
    };

    use super::Style;

    const SERVICE: &str = "org.freedesktop.Notifications";
    const PATH: &str = "/org/freedesktop/Notifications";

    /// Shows each alert until the window closes, and opens the conversation of one
    /// that's clicked.
    pub fn run(alerts: Receiver<(Alert, Style)>, opened: fn(String)) {
        let connection = match Connection::session() {
            Ok(connection) => connection,
            Err(error) => {
                tracing::warn!(%error, "no session bus for desktop notifications");
                return;
            }
        };
        let capabilities: Vec<String> = connection
            .call_method(Some(SERVICE), PATH, Some(SERVICE), "GetCapabilities", &())
            .and_then(|reply| reply.body().deserialize())
            .unwrap_or_default();
        let markup = capabilities
            .iter()
            .any(|capability| capability == "body-markup");
        // Which conversation each notification shown is from, by its id.
        let shown = Arc::new(Mutex::new(HashMap::<u32, String>::new()));
        let clicks = connection.clone();
        let rooms = shown.clone();
        let listening = std::thread::Builder::new()
            .name("yapfast-notify-clicks".into())
            .spawn(move || listen(&clicks, &rooms, opened));
        if let Err(error) = listening {
            tracing::warn!(%error, "notifications can't open their conversations");
        }
        for (alert, style) in alerts {
            let mut body = super::body(&alert, style);
            if markup {
                body = escape(&body);
            }
            let mut hints = HashMap::<&str, Value>::new();
            hints.insert("category", Value::from("im.received"));
            // Yapfast plays its own sounds, and rings for a call itself.
            hints.insert("suppress-sound", Value::from(true));
            // The server finds Yapfast's name and icon through its desktop entry.
            hints.insert("desktop-entry", Value::from(crate::icon::APP_ID));
            if let Some(picture) = &alert.picture
                && let Some(image) = image_data(picture)
            {
                hints.insert("image-data", image);
            }
            let actions = ["default", "Open"];
            let request = (
                "Yapfast",
                0u32,
                "",
                alert.title.as_str(),
                body.as_str(),
                &actions[..],
                hints,
                -1i32,
            );
            let id = connection
                .call_method(Some(SERVICE), PATH, Some(SERVICE), "Notify", &request)
                .and_then(|reply| reply.body().deserialize::<u32>());
            match id {
                Ok(id) => {
                    shown
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .insert(id, alert.room);
                }
                Err(error) => tracing::warn!(%error, "could not show a notification"),
            }
        }
    }

    /// Opens the conversation of a notification that's clicked, and forgets those closed.
    fn listen(connection: &Connection, shown: &Mutex<HashMap<u32, String>>, opened: fn(String)) {
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(SERVICE)
            .and_then(|rule| rule.path(PATH))
            .map(|rule| rule.build());
        let messages =
            rule.and_then(|rule| MessageIterator::for_match_rule(rule, connection, Some(64)));
        let messages = match messages {
            Ok(messages) => messages,
            Err(error) => {
                tracing::warn!(%error, "notifications can't open their conversations");
                return;
            }
        };
        for message in messages.flatten() {
            let header = message.header();
            let shown = || {
                shown
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
            };
            match header.member().map(|member| member.as_str()) {
                Some("ActionInvoked") => {
                    let Ok((id, action)) = message.body().deserialize::<(u32, String)>() else {
                        continue;
                    };
                    let room = shown().get(&id).cloned();
                    if let (Some(room), "default") = (room, action.as_str()) {
                        let _ = slint::invoke_from_event_loop(move || opened(room));
                    }
                }
                Some("NotificationClosed") => {
                    if let Ok((id, _reason)) = message.body().deserialize::<(u32, u32)>() {
                        shown().remove(&id);
                    }
                }
                _ => {}
            }
        }
    }

    /// The author's picture, as the specification's `(iiibiiay)` image.
    fn image_data(picture: &yapfast_core::chat::Picture) -> Option<Value<'static>> {
        let width = i32::try_from(picture.width).ok()?;
        let height = i32::try_from(picture.height).ok()?;
        let structure = StructureBuilder::new()
            .add_field(width)
            .add_field(height)
            .add_field(width.checked_mul(4)?)
            .add_field(true)
            .add_field(8i32)
            .add_field(4i32)
            .add_field(picture.rgba.to_vec())
            .build()
            .ok()?;
        Some(Value::from(structure))
    }

    /// Text for a service that reads the body as markup.
    fn escape(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
}

/// How many toasts a click still reaches Yapfast from: the notification center keeps
/// as many of an app's.
#[cfg(windows)]
const KEPT: usize = 20;

/// The 256-pixel picture in Yapfast's icon file, which is stored as a PNG.
#[cfg(any(windows, test))]
fn icon_png() -> Option<&'static [u8]> {
    const ICON: &[u8] = include_bytes!("../packaging/windows/yapfast.ico");
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
mod windows {
    use std::{collections::VecDeque, path::Path, sync::mpsc::Receiver};

    use ::windows::{
        Data::Xml::Dom::XmlDocument,
        Foundation::TypedEventHandler,
        UI::Notifications::{ToastNotification, ToastNotificationManager, ToastNotifier},
        core::HSTRING,
    };
    use anyhow::Context;
    use image::ImageEncoder;
    use yapfast_core::chat::{Alert, Picture};

    use super::{KEPT, Style};

    /// Shows each alert as a toast until the window closes, and opens the conversation
    /// of one that's clicked while Yapfast runs.
    pub fn run(alerts: Receiver<(Alert, Style)>, opened: fn(String), root: &Path) {
        let folder = root.join("notifications");
        // Toasts take their pictures by absolute paths.
        let folder = std::path::absolute(&folder).unwrap_or(folder);
        let notifier = match register(&folder) {
            Ok(notifier) => notifier,
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), "no desktop notifications");
                return;
            }
        };
        // The toasts that can still be clicked. Each keeps its author's picture in a
        // file of its own, which the next toast in its place writes over.
        let mut shown = VecDeque::with_capacity(KEPT);
        let mut count = 0usize;
        for (alert, style) in alerts {
            let picture = alert.picture.as_deref().and_then(|picture| {
                let path = folder.join(format!("{}.png", count % KEPT));
                match save(picture, &path) {
                    Ok(()) => Some(path),
                    Err(error) => {
                        tracing::warn!(%error, "could not keep a notification's picture");
                        None
                    }
                }
            });
            match show(&notifier, &alert, style, picture.as_deref(), opened) {
                Ok(toast) => {
                    if shown.len() == KEPT {
                        shown.pop_front();
                    }
                    shown.push_back(toast);
                    count += 1;
                }
                Err(error) => tracing::warn!(%error, "could not show a notification"),
            }
        }
    }

    /// Names Yapfast and its icon to Windows by its app ID, which a toast needs from
    /// an app without a package. The installer's Start menu shortcut can't do it, as
    /// a portable copy hasn't got one.
    fn register(folder: &Path) -> anyhow::Result<ToastNotifier> {
        std::fs::create_dir_all(folder).context("making the notifications folder")?;
        let icon = folder.join("icon.png");
        let png = super::icon_png().context("Yapfast's icon has no 256-pixel picture")?;
        if std::fs::read(&icon).ok().as_deref() != Some(png) {
            std::fs::write(&icon, png).context("writing the notification icon")?;
        }
        let key = format!(r"Software\Classes\AppUserModelId\{}", crate::icon::APP_ID);
        crate::protocol::set_string(&key, Some("DisplayName"), "Yapfast")
            .context("registering Yapfast's name")?;
        let icon = icon
            .to_str()
            .context("the data folder's path isn't Unicode")?;
        crate::protocol::set_string(&key, Some("IconUri"), icon)
            .context("registering Yapfast's icon")?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(crate::icon::APP_ID))
            .context("starting toasts")
    }

    fn show(
        notifier: &ToastNotifier,
        alert: &Alert,
        style: Style,
        picture: Option<&Path>,
        opened: fn(String),
    ) -> ::windows::core::Result<ToastNotification> {
        let picture = picture
            .and_then(Path::to_str)
            .map(|path| {
                format!(
                    r#"<image placement="appLogoOverride" hint-crop="circle" src="{}"/>"#,
                    super::xml_text(&super::file_uri(path))
                )
            })
            .unwrap_or_default();
        // Yapfast plays its own sounds, and rings for a call itself.
        let xml = format!(
            r#"<toast><visual><binding template="ToastGeneric">{picture}<text>{}</text><text>{}</text></binding></visual><audio silent="true"/></toast>"#,
            super::xml_text(&alert.title),
            super::xml_text(&super::body(alert, style)),
        );
        let document = XmlDocument::new()?;
        document.LoadXml(&HSTRING::from(xml))?;
        let toast = ToastNotification::CreateToastNotification(&document)?;
        let room = alert.room.clone();
        toast.Activated(&TypedEventHandler::new(move |_, _| {
            let room = room.clone();
            let _ = slint::invoke_from_event_loop(move || opened(room));
            Ok(())
        }))?;
        notifier.Show(&toast)?;
        Ok(toast)
    }

    /// The author's picture as a PNG at `path`, for a toast to show.
    fn save(picture: &Picture, path: &Path) -> image::ImageResult<()> {
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        image::codecs::png::PngEncoder::new(file).write_image(
            &picture.rgba,
            picture.width,
            picture.height,
            image::ExtendedColorType::Rgba8,
        )
    }
}

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
