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

use super::{Clicked, KEPT, Opened, Style};

/// Shows each alert as a toast until the window closes, and opens the conversation
/// of one that's clicked while Yapfast runs.
pub fn run(alerts: Receiver<(Alert, Style)>, opened: Opened, root: &Path) {
    let folder = root.join("notifications");
    // Toasts take their pictures by absolute paths.
    let folder = std::path::absolute(&folder).unwrap_or(folder);
    let notifier = match register(&folder) {
        Ok(notifier) => notifier,
        Err(error) => {
            tracing::warn!(target: crate::LOG, error = format!("{error:#}"), "no desktop notifications");
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
                    tracing::warn!(target: crate::LOG, %error, "could not keep a notification's picture");
                    None
                }
            }
        });
        match show(&notifier, &alert, style, picture.as_deref(), &opened) {
            Ok(toast) => {
                if shown.len() == KEPT {
                    shown.pop_front();
                }
                shown.push_back(toast);
                count += 1;
            }
            Err(error) => {
                tracing::warn!(target: crate::LOG, %error, "could not show a notification")
            }
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
    crate::protocol::set_string(&key, Some("DisplayName"), yapfast_lang::PRODUCT)
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
    opened: &Opened,
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
    let opened = std::sync::Arc::clone(opened);
    toast.Activated(&TypedEventHandler::new(move |_, _| {
        opened(Clicked {
            room: room.clone(),
            token: None,
        });
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
