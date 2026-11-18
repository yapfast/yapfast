//! Notifications on the preview's real core: a message behind other windows
//! notifies, saying what it says as the settings let it, and flashes the
//! taskbar until the window comes in front; in front none shows; a click on
//! one brings the window to its conversation, over the settings.

use yapfast_core::application::{Action, Effect};
use yapfast_core::chat::Alert;

use anomi_desktop::notify::Clicked;
use anomi_vm::shell::Layer;

use super::start;
use crate::app::App;
use crate::message::Message;
use crate::platform::Heard;

fn alert(room: &str, call: bool) -> Alert {
    Alert {
        room: room.into(),
        title: "Sam".into(),
        body: "Anyone around tonight?".into(),
        picture: None,
        call,
        mention: false,
    }
}

fn notify(app: &mut App, alert: Alert) {
    drop(crate::effects::run(app, vec![Effect::Notify(alert)]));
}

#[test]
fn a_message_behind_other_windows_notifies_and_flashes() {
    let mut window = start("notify-behind", true);
    let app = &mut window.app;
    app.window = Some(iced::window::Id::unique());
    // In front the window itself shows what's new.
    notify(app, alert("general", false));
    assert!(app.asked.notified.is_empty());
    assert!(!app.desktop.flashing);
    drop(app.update(Message::Focused(false)));
    notify(app, alert("general", false));
    assert_eq!(
        app.asked.notified,
        [("general".to_owned(), "Anyone around tonight?".to_owned())]
    );
    assert!(app.desktop.flashing);
    // Back in front, the taskbar stops.
    drop(app.update(Message::Focused(true)));
    assert!(!app.desktop.flashing);
}

#[test]
fn the_settings_choose_what_a_notification_says_and_whether_it_shows() {
    let mut window = start("notify-settings", true);
    let app = &mut window.app;
    drop(app.update(Message::Focused(false)));
    app.layout
        .edit(|saved| saved.notification_text = Some(false));
    notify(app, alert("general", false));
    // A call's notification says it rings, whatever the setting.
    notify(app, alert("general", true));
    let said: Vec<_> = app
        .asked
        .notified
        .iter()
        .map(|(_, text)| text.as_str())
        .collect();
    assert_eq!(said, ["", "Anyone around tonight?"]);
    app.layout.edit(|saved| {
        saved.notifications = Some(false);
        saved.flash_taskbar = Some(false);
    });
    app.window = Some(iced::window::Id::unique());
    notify(app, alert("general", false));
    assert_eq!(app.asked.notified.len(), 2);
    assert!(!app.desktop.flashing);
}

#[test]
fn a_click_on_a_notification_opens_its_conversation_over_the_settings() {
    let mut window = start("notify-click", true);
    let app = &mut window.app;
    drop(app.dispatch(Action::Open("general".into())));
    app.vm.shell.open(Layer::Settings);
    let room = (app.core.state.rooms.iter())
        .find(|room| room.direct)
        .expect("a DM")
        .id
        .clone();
    let clicked = Clicked {
        room: room.clone(),
        token: None,
    };
    drop(app.update(Message::Desktop(Heard::Notification(clicked))));
    assert_eq!(app.core.state.selected.as_deref(), Some(room.as_str()));
    assert!(!app.vm.shell.is_open(Layer::Settings));
}
