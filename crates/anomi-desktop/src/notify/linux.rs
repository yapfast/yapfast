use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc::Receiver},
};

use yapfast_core::chat::Alert;
use zbus::{
    blocking::{Connection, MessageIterator},
    zvariant::{StructureBuilder, Value},
};

use super::{Clicked, Opened, Style};

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

/// Shows each alert until the window closes, and opens the conversation of one
/// that's clicked.
pub fn run(alerts: Receiver<(Alert, Style)>, opened: Opened) {
    let connection = match Connection::session() {
        Ok(connection) => connection,
        Err(error) => {
            tracing::warn!(target: crate::LOG, %error, "no session bus for desktop notifications");
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
        .spawn(move || listen(&clicks, &rooms, &opened));
    if let Err(error) = listening {
        tracing::warn!(target: crate::LOG, %error, "notifications can't open their conversations");
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
        let actions = ["default", yapfast_lang::tr!("Open")];
        let request = (
            yapfast_lang::PRODUCT,
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
            Err(error) => {
                tracing::warn!(target: crate::LOG, %error, "could not show a notification")
            }
        }
    }
}

/// Opens the conversation of a notification that's clicked, and forgets those closed.
fn listen(connection: &Connection, shown: &Mutex<HashMap<u32, String>>, opened: &Opened) {
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
            tracing::warn!(target: crate::LOG, %error, "notifications can't open their conversations");
            return;
        }
    };
    // The activation token a click brought, which the service sends just
    // before the click itself (version 1.2 of the specification).
    let mut token: Option<(u32, String)> = None;
    for message in messages.flatten() {
        let header = message.header();
        let shown = || {
            shown
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        };
        match header.member().map(|member| member.as_str()) {
            Some("ActivationToken") => {
                token = message.body().deserialize::<(u32, String)>().ok();
            }
            Some("ActionInvoked") => {
                let Ok((id, action)) = message.body().deserialize::<(u32, String)>() else {
                    continue;
                };
                let room = shown().get(&id).cloned();
                tracing::debug!(target: crate::LOG, id, %action, known = room.is_some(), "a notification was clicked");
                let token = token
                    .take()
                    .filter(|(of, _)| *of == id)
                    .map(|(_, token)| token);
                if let (Some(room), "default") = (room, action.as_str()) {
                    opened(Clicked { room, token });
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
