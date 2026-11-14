//! A Discord channel's settings, as the Slint client's tests check them
//! (`src/ui/tests.rs`): opened from the channel's menu over the window,
//! Overview holding its draft until Reset, Permissions' @everyone three
//! ways each, locked where this account can't, and Delete Channel closing
//! them to ask.

use iced::{
    Point,
    keyboard::{self, key},
    widget::Id,
};
use yapfast_core::{
    application::{ChannelInput, ChannelSection, OverwriteState},
    chat::{ChannelSettings, Event},
    yapfast_discord::ready::permission,
};

use super::{interact, server::discord_server, shows};
use crate::app::{App, Message};

/// #general's room id.
const GENERAL: &str = "discord:10:11";

/// The channel's menu, as a right click on its row opens it, and Edit
/// channel chosen from it.
fn open(app: &mut App) {
    let _ = app.open_place_menu(GENERAL, Point::new(120.0, 100.0), false);
    interact(app, |ui| {
        ui.click("Edit channel").expect("Edit channel");
    });
}

fn input(app: &mut App, input: ChannelInput) {
    let _ = app.update(Message::ChannelSettings(
        crate::channel_settings::Event::Input(input),
    ));
}

#[test]
fn channel_settings_cover_the_window_with_overview_and_permissions() {
    let (mut app, _folder) = discord_server(
        permission::MANAGE_CHANNELS | permission::MANAGE_ROLES,
        |_, _| {},
    );
    open(&mut app);
    let core = app.client.state().channel_settings.as_ref().expect("open");
    assert_eq!(core.section, ChannelSection::Overview);
    for row in [
        "#GENERAL",
        "Overview",
        "Permissions",
        "Delete Channel",
        "Loading...",
    ] {
        assert!(shows(&app, row), "{row}");
    }

    app.client.apply(Event::ChannelLoaded {
        room: GENERAL.into(),
        result: Ok(ChannelSettings {
            name: "general".into(),
            topic: "Say hi".into(),
            slowmode: 300,
            nsfw: true,
        }),
    });
    let _ = app.sync();
    assert!(!shows(&app, "Loading..."));
    for words in ["CHANNEL TOPIC", "Say hi", "5m", "Age-Restricted Channel"] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(!shows(&app, "Save Changes"));

    // Typing shows the bar, and another page waits for it.
    interact(&mut app, |ui| {
        ui.click(Id::new("channel-topic"))
            .expect("the topic's field");
        ui.typewrite("!");
    });
    assert!(shows(&app, "Save Changes"));
    interact(&mut app, |ui| {
        ui.click("Permissions").expect("Permissions");
    });
    let core = app.client.state().channel_settings.as_ref().unwrap();
    assert_eq!((core.section, core.nudge), (ChannelSection::Overview, 1));
    interact(&mut app, |ui| {
        ui.click("Reset").expect("Reset");
    });
    assert!(shows(&app, "Say hi") && !shows(&app, "Save Changes"));

    // Permissions: @everyone's, three ways each, locked where she can't.
    interact(&mut app, |ui| {
        ui.click("Permissions").expect("Permissions");
    });
    for words in [
        "Channel Permissions",
        "ROLES/MEMBERS",
        "@everyone",
        "GENERAL CHANNEL PERMISSIONS",
        "View Channel",
        "Permissions synced with category: Talk",
        "Private Channel",
    ] {
        assert!(shows(&app, words), "{words}");
    }
    assert!(!shows(&app, "Remove @everyone"), "@everyone stays");
    let rules = app.client.app.overwrite_rules(GENERAL);
    assert!(!rules.locked(permission::VIEW_CHANNEL.trailing_zeros()));
    assert!(rules.locked(permission::MANAGE_WEBHOOKS.trailing_zeros()));
    let send = permission::SEND_MESSAGES.trailing_zeros();
    input(&mut app, ChannelInput::Set(send, OverwriteState::Allow));
    assert!(shows(&app, "Save Changes"));
    interact(&mut app, |ui| {
        ui.click("Reset").expect("Reset");
    });
    let _ = app.update(Message::Key(
        keyboard::Key::Named(key::Named::Escape),
        Default::default(),
    ));
    assert!(app.client.state().channel_settings.is_none());
    assert!(app.channel_screen.is_none(), "let go of with the core's");

    // Delete Channel, last in the sidebar, closes them and asks.
    open(&mut app);
    interact(&mut app, |ui| {
        ui.click("Delete Channel").expect("Delete Channel");
    });
    assert!(app.client.state().channel_settings.is_none());
    assert!(app.client.state().channel_dialog.is_some(), "asks");
}
