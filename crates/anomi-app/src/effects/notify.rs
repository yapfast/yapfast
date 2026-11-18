//! A message or a call for this account: `Effect::Notify`, which the core
//! sends only where the person's settings, the place's mutes and their
//! status let it (Do Not Disturb sends none). As the Slint app's `Notify`
//! (`src/ui.rs`): the message sounds unless its conversation is being read
//! in front, as a call rings instead; behind other windows the taskbar
//! flashes, until the window comes in front again, and a desktop
//! notification shows, as the settings say. A click on it brings the window
//! forward on its conversation (`platform::notified`).

use anomi_desktop::notify::Style;
use iced::{Task, window};
use yapfast_core::application::Effect;
use yapfast_core::chat::Alert;
use yapfast_core::sounds::Sound;

use crate::app::App;
use crate::message::Message;

pub(super) fn run(app: &mut App, effect: Effect) -> Task<Message> {
    match effect {
        Effect::Notify(alert) => alert_of(app, alert),
        effect => super::waiting(&effect),
    }
}

fn alert_of(app: &mut App, alert: Alert) -> Task<Message> {
    // In front, the window itself shows what's new.
    let front = app.motion.front;
    let reading = front && app.core.state.selected.as_ref() == Some(&alert.room);
    if !alert.call && !reading {
        let sound = if alert.mention {
            Sound::Mention
        } else {
            Sound::Message
        };
        super::sound::play(app, sound);
    }
    let mut task = Task::none();
    if !front
        && !alert.call
        && app.layout.flash_taskbar.unwrap_or(true)
        && let Some(id) = app.window
    {
        app.desktop.flashing = true;
        task = window::request_user_attention(id, Some(window::UserAttention::Informational));
    }
    if !front && app.layout.notifications.unwrap_or(true) {
        let style = Style {
            text: app.layout.notification_text.unwrap_or(true),
        };
        show(app, alert, style);
    }
    task
}

/// Shows a notification, starting the notifier with the first: its thread
/// hands a click to the window's loop as `Heard::Notification`.
#[cfg(not(test))]
fn show(app: &mut App, alert: Alert, style: Style) {
    let desktop = &mut app.desktop;
    let Some(tell) = &desktop.tell else {
        return;
    };
    let notifier = desktop.notifier.get_or_insert_with(|| {
        let tell = tell.clone();
        let opened = std::sync::Arc::new(move |clicked: anomi_desktop::notify::Clicked| {
            let _ = tell.send(crate::platform::Heard::Notification(clicked));
        });
        anomi_desktop::notify::Notifier::start(opened, &app.root)
    });
    notifier.show(alert, style);
}

/// A test reaches no notification service: it keeps the conversation and
/// the text a notification would show.
#[cfg(test)]
fn show(app: &mut App, alert: Alert, style: Style) {
    let text = if style.text || alert.call {
        alert.body
    } else {
        String::new()
    };
    app.asked.notified.push((alert.room, text));
}

/// The window came in front: the taskbar stops flashing for it, as the
/// Slint window's own focus did (`src/ui.rs:545`).
pub(crate) fn front(app: &mut App) -> Task<Message> {
    match app.window {
        Some(id) if std::mem::take(&mut app.desktop.flashing) => {
            window::request_user_attention(id, None)
        }
        _ => Task::none(),
    }
}
