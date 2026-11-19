//! Settings: a theme chosen in Appearance shows at once and is kept, a
//! custom theme made, edited and deleted there, preferences kept across a
//! restart, a device chosen in Voice & video, and push to talk's key
//! recorded and kept.

use iced::keyboard::{Key, Location, key};
use yapfast_core::voice;

use super::{change, interact, local, preview, shows, start};
use crate::{
    app::{
        App, Message,
        layers::{Layer, Popup},
    },
    settings::{Event, Section, voice::Event as VoiceEvent},
};

/// Settings on a section, in a window tall enough to show its whole page.
fn open(app: &mut App, section: Section) {
    app.shell.frame.height = 2400.0;
    crate::window_bar::fit_window(app);
    let _ = app.update(Message::Settings(Event::Open(section)));
}

fn stored(folder: &tempfile::TempDir) -> serde_json::Value {
    serde_json::from_slice(&{
        crate::layout::flush(folder.path());
        std::fs::read(folder.path().join("layout.json")).expect("the layout")
    })
    .expect("JSON")
}

/// Settings open moving, once: their motion starts as they open, stays
/// through what's done in them, and turns to closing as they close, the
/// Settings closed kept, taking nothing, until they've been drawn going.
#[test]
fn settings_open_and_close_as_a_popup() {
    let (mut app, _folder) = preview();
    assert!(app.popups_moving().is_empty());
    open(&mut app, Section::Account);
    let opening = app.popups_moving();
    assert_eq!(opening.len(), 1, "Settings' motion");
    assert!(!opening[0].1, "opening");
    open(&mut app, Section::Appearance);
    assert_eq!(
        app.popups_moving(),
        opening,
        "the same motion, not a new one"
    );
    let _ = app.update(Message::Settings(Event::Close));
    assert!(app.win.settings.get().is_none(), "closed");
    assert!(app.win.settings.leaving().is_some(), "drawn going");
    assert_eq!(app.popups_moving(), [(opening[0].0, true)], "closing");
    // Escape is the window's again while they go.
    assert!(app.layers().all(|layer| layer != Layer::Settings));
    let _ = app.update(Message::PopupGone(Popup::Layer(Layer::Settings)));
    assert!(app.win.settings.leaving().is_none(), "gone");
    assert!(app.popups_moving().is_empty());
}

/// Settings opened again as they close are the new ones, opening: the
/// ones going go at once.
#[test]
fn settings_opened_again_as_they_close_open_anew() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Account);
    let _ = app.update(Message::Settings(Event::Close));
    open(&mut app, Section::Appearance);
    assert!(app.win.settings.leaving().is_none());
    let moving = app.popups_moving();
    assert_eq!(moving.len(), 1);
    assert!(!moving[0].1, "opening");
}

#[test]
fn a_theme_chosen_in_appearance_shows_at_once_and_is_kept() {
    let (mut app, folder) = preview();
    open(&mut app, Section::Account);
    assert!(shows(&app, "My account"));
    interact(&mut app, |ui| {
        ui.click("Appearance").expect("the section");
    });
    assert!(shows(&app, "Catppuccin Latte"), "the themes' tiles");
    interact(&mut app, |ui| {
        ui.click("Nord").expect("its tile");
    });
    assert_eq!(app.theme.id, "nord", "at once");
    assert_eq!(stored(&folder)["theme"], "nord");
    // Escape closes Settings and lets go of what it held.
    let _ = app.update(Message::Key(
        Key::Named(key::Named::Escape),
        Default::default(),
    ));
    assert!(!app.win.settings.is_open());
    assert_eq!(start(&folder, true).theme.id, "nord", "the next start too");
}

#[test]
fn a_custom_theme_is_made_edited_and_deleted_in_appearance() {
    use crate::settings::appearance_event as theme;
    let (mut app, folder) = preview();
    open(&mut app, Section::Appearance);
    interact(&mut app, |ui| {
        ui.click("New theme").expect("New theme");
    });
    assert!(shows(&app, "NEW THEME") && shows(&app, "START FROM"));
    // A colour typed shows at once; one that isn't a colour says so.
    let chat = yapfast_core::theme::Theme::NAMES
        .iter()
        .position(|name| *name == "chat")
        .expect("chat");
    let _ = app.update(theme(crate::settings::ThemeEvent::Colour(
        chat,
        "#102030".into(),
    )));
    assert_eq!(app.theme.chat, crate::style::hex(0x102030));
    let _ = app.update(theme(crate::settings::ThemeEvent::Colour(
        chat,
        "#1020".into(),
    )));
    assert!(shows(&app, "Not a colour"));
    assert_eq!(
        app.theme.chat,
        crate::style::hex(0x102030),
        "the last good one"
    );
    let _ = app.update(theme(crate::settings::ThemeEvent::Colour(
        chat,
        "#203040".into(),
    )));
    let _ = app.update(theme(crate::settings::ThemeEvent::Name("Harbour".into())));
    interact(&mut app, |ui| {
        ui.click("Save").expect("Save");
    });
    assert_eq!(app.theme.id, "custom:1", "chosen once saved");
    let layout = stored(&folder);
    assert_eq!(layout["theme"], "custom:1");
    assert_eq!(layout["custom_themes"][0]["name"], "Harbour");
    assert_eq!(layout["custom_themes"][0]["colours"]["chat"], "#203040");
    assert!(shows(&app, "Harbour"), "among the tiles");
    // Edited, then cancelled: the theme from before comes back.
    let _ = app.update(theme(crate::settings::ThemeEvent::Edit("custom:1".into())));
    assert!(shows(&app, "EDIT THEME"));
    let _ = app.update(theme(crate::settings::ThemeEvent::Colour(
        chat,
        "#ffffff".into(),
    )));
    assert_eq!(app.theme.chat, crate::style::hex(0xffffff));
    interact(&mut app, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert_eq!(app.theme.chat, crate::style::hex(0x203040));
    // Deleted, asked once more: the chosen theme goes, and Yap shows.
    let _ = app.update(theme(crate::settings::ThemeEvent::Edit("custom:1".into())));
    interact(&mut app, |ui| {
        ui.click("Delete theme").expect("Delete theme");
    });
    assert!(shows(&app, "Delete it for good"));
    interact(&mut app, |ui| {
        ui.click("Delete it for good").expect("Delete it for good");
    });
    assert_eq!(app.theme.id, "yap");
    assert_eq!(stored(&folder)["custom_themes"], serde_json::json!([]));
}

#[test]
fn preferences_are_kept_across_a_restart() {
    let (mut app, folder) = preview();
    // The newest message sent at a time of its own, as Discord's are.
    change(&mut app, |state| {
        let mut messages = (*state.messages).clone();
        let last = messages.last_mut().expect("a message");
        let mut message = (**last).clone();
        message.timestamp = 1_790_000_000;
        *last = std::sync::Arc::new(message);
        state.messages = std::sync::Arc::new(messages);
    });
    open(&mut app, Section::Messages);
    interact(&mut app, |ui| {
        ui.click("12-hour").expect("the clock");
    });
    interact(&mut app, |ui| {
        ui.click("Show seconds").expect("the switch");
    });
    interact(&mut app, |ui| {
        ui.click("Collapse long messages").expect("the switch");
    });
    interact(&mut app, |ui| {
        ui.click("Large").expect("the text size");
    });
    assert!(app.layout.prefs.twelve_hour_clock && app.layout.prefs.clock_seconds);
    // The timeline's times follow at once: 9:41:05 PM, say.
    let time = &app.timeline.looks.last().expect("a message").time;
    assert!(
        time.matches(':').count() == 2 && (time.ends_with("AM") || time.ends_with("PM")),
        "{time}"
    );
    open(&mut app, Section::Notifications);
    interact(&mut app, |ui| {
        ui.click("Flash the taskbar").expect("the switch");
    });
    open(&mut app, Section::Advanced);
    interact(&mut app, |ui| {
        ui.click("Developer Mode").expect("the switch");
    });
    let again = start(&folder, true);
    let prefs = &again.layout.prefs;
    assert!(prefs.twelve_hour_clock && prefs.clock_seconds);
    assert!(again.layout.collapse_long && again.layout.developer_mode);
    assert_eq!(prefs.text_size, Some(2));
    assert_eq!(prefs.flash_taskbar, Some(false));
    // `layout.json` keeps them under the keys older versions wrote.
    let layout = stored(&folder);
    assert_eq!(layout["twelve_hour_clock"], true);
    assert_eq!(layout["flash_taskbar"], false);
}

#[test]
fn a_device_chosen_in_voice_settings_is_the_one_used() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| {
        let device = |id: &str, name: &str| voice::Device {
            id: id.into(),
            name: name.into(),
        };
        state.voice.microphones = vec![device("mic-a", "Desk"), device("mic-b", "Headset")];
        state.voice.microphone = Some("mic-a".into());
    });
    open(&mut app, Section::Voice);
    assert!(shows(&app, "Desk"), "the one chosen");
    assert!(!shows(&app, "Headset"), "the rest only in the list");
    interact(&mut app, |ui| {
        ui.click("Desk").expect("the dropdown");
    });
    interact(&mut app, |ui| {
        ui.click("Headset").expect("its option");
    });
    assert_eq!(
        app.client.state().voice.microphone.as_deref(),
        Some("mic-b")
    );
    assert!(
        app.win.settings.get().is_some_and(|s| s.open.is_none()),
        "closed"
    );
}

#[test]
fn the_soundboard_volume_moves_with_its_drag() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| state.discord_linked = true);
    open(&mut app, Section::Voice);
    assert!(shows(&app, "Soundboard volume"));
    assert!(!shows(&app, "37%"));
    let voice = |event| Message::Settings(Event::Voice(event));
    let _ = app.update(voice(VoiceEvent::SoundboardVolume(37, false)));
    assert!(shows(&app, "37%"), "the slider follows the drag");
    // Let go, what the drag left is sent, not what the slider was built
    // with, and the slider shows the core's volume again.
    let _ = app.update(voice(VoiceEvent::SoundboardVolume(80, true)));
    assert!(
        app.win
            .settings
            .get()
            .is_some_and(|settings| settings.voice.soundboard.is_none())
    );
    assert!(!shows(&app, "37%"));
}

#[test]
fn a_recorded_push_to_talk_key_is_named_and_kept() {
    use crate::settings::keybinds::recorded;
    use iced::keyboard::key::{Code, Physical};
    let v = recorded(
        &Key::Character("v".into()),
        Physical::Code(Code::KeyV),
        Location::Standard,
    )
    .expect("a key");
    assert_eq!(v.name, "V");
    if cfg!(target_os = "linux") {
        assert_eq!(v.scancode, 47, "evdev's, as winit gives it");
        assert_eq!(v.trigger.as_deref(), Some("v"));
    }
    let ctrl = recorded(
        &Key::Named(key::Named::Control),
        Physical::Code(Code::ControlRight),
        Location::Right,
    )
    .expect("a key");
    assert_eq!(ctrl.name, "Right Ctrl");
    if cfg!(target_os = "linux") {
        assert_eq!(ctrl.trigger.as_deref(), Some("Control_R"));
    }

    let (mut app, folder) = preview();
    open(&mut app, Section::Voice);
    interact(&mut app, |ui| {
        ui.click("Push to talk").expect("the input mode");
    });
    interact(&mut app, |ui| {
        ui.click("Record keybind").expect("the button");
    });
    assert!(app.recording_keybind());
    assert!(shows(&app, "Press a key..."));
    // The keys pressed together are the keybind once they're all up, and
    // show as they're pressed.
    let heard = |key: &yapfast_core::keybinds::Key, pressed| {
        Message::Settings(Event::Heard(crate::settings::keybinds::Heard::Key {
            key: key.clone(),
            pressed,
            escape: false,
        }))
    };
    let _ = app.update(heard(&ctrl, true));
    let _ = app.update(heard(&v, true));
    assert!(shows(&app, "Right Ctrl + V") || shows(&app, "Ctrl + V"));
    let _ = app.update(heard(&v, false));
    assert!(app.recording_keybind(), "a key is still down");
    let _ = app.update(heard(&ctrl, false));
    assert!(!app.recording_keybind());
    assert!(shows(&app, "Edit keybind"));
    let layout = stored(&folder);
    assert_eq!(layout["input_mode"], "push_to_talk");
    assert_eq!(layout["push_to_talk_key"]["name"], "V");
    assert_eq!(layout["push_to_talk_key"]["with"][0]["name"], "Right Ctrl");
    // Escape, pressed first, stops without changing it.
    interact(&mut app, |ui| {
        ui.click("Edit keybind").expect("the button");
    });
    let escape = yapfast_core::keybinds::Key {
        scancode: 1,
        name: "Escape".into(),
        trigger: None,
    };
    let _ = app.update(Message::Settings(Event::Heard(
        crate::settings::keybinds::Heard::Key {
            key: escape,
            pressed: true,
            escape: true,
        },
    )));
    assert!(!app.recording_keybind());
    assert_eq!(stored(&folder)["push_to_talk_key"]["name"], "V");
    // Where the key works, as its watcher tells from its own thread,
    // shows without anything else happening; a word from a watcher gone
    // since doesn't.
    let told = app.keybinds.count;
    let status = |reach| {
        crate::keybinds::Told::Status(yapfast_core::keybinds::Status {
            reach,
            given: Default::default(),
        })
    };
    let everywhere = "Hold the key in any app to talk.";
    let _ = app.update(Message::Keybinds(
        told,
        status(yapfast_core::keybinds::Reach::Everywhere),
    ));
    assert!(shows(&app, everywhere));
    let focused = yapfast_core::keybinds::Reach::Focused("Your desktop has no global keys");
    let _ = app.update(Message::Keybinds(told - 1, status(focused.clone())));
    assert!(shows(&app, everywhere));
    // Kept while Settings are closed, for when they open.
    let _ = app.close_settings();
    let _ = app.update(Message::Keybinds(told, status(focused)));
    open(&mut app, Section::Voice);
    assert!(shows(
        &app,
        "Your desktop has no global keys, so push to talk works only while Yapfast's window is in front."
    ));
    let again = start(&folder, true);
    assert_eq!(
        again
            .layout
            .voice
            .push_to_talk_key
            .as_ref()
            .map(yapfast_core::keybinds::Shortcut::name),
        Some("Ctrl + V".to_owned())
    );
}

#[test]
fn keybinds_are_added_chosen_recorded_and_removed() {
    let (mut app, folder) = preview();
    open(&mut app, Section::Keybinds);
    assert!(shows(&app, "Add a Keybind"));
    interact(&mut app, |ui| {
        ui.click("Add a Keybind").expect("the button");
    });
    assert!(shows(
        &app,
        "This action doesn't do anything! Please choose one from the drop down."
    ));
    let keybind = |event| Message::Settings(Event::Keybind(event));
    use crate::settings::keybinds::{Event as KeybindEvent, Heard, Target};
    let _ = app.update(keybind(KeybindEvent::Action(0, 4)));
    assert!(shows(&app, "Toggle Deafen"));
    let _ = app.update(Message::Settings(Event::Record(Target::Row(0), true)));
    assert!(app.recording_keybind());
    let d = yapfast_core::keybinds::Key {
        scancode: 32,
        name: "D".into(),
        trigger: Some("d".into()),
    };
    for pressed in [true, false] {
        let _ = app.update(Message::Settings(Event::Heard(Heard::Key {
            key: d.clone(),
            pressed,
            escape: false,
        })));
    }
    assert!(!app.recording_keybind());
    let layout = stored(&folder);
    assert_eq!(layout["keybinds"][0]["action"], "toggle_deafen");
    assert_eq!(layout["keybinds"][0]["keys"]["name"], "D");
    assert_eq!(layout["keybinds"][0]["id"], 1);
    // The watcher watches it: a toggle from it deafens.
    assert!(
        app.keybinds
            .watched
            .iter()
            .any(|watched| watched.id == "keybind-1")
    );
    // As the account card's button: out of a call, the next one starts
    // deafened.
    let deafened =
        |app: &crate::app::App| (app.client.state().voice.deafened, app.layout.deafen_on_join);
    let before = deafened(&app);
    let _ = app.update(Message::Keybinds(
        app.keybinds.count,
        crate::keybinds::Told::Toggled(yapfast_core::keybinds::Action::ToggleDeafen),
    ));
    assert_ne!(deafened(&app), before);
    let _ = app.update(keybind(KeybindEvent::Remove(0)));
    assert!(!shows(&app, "Toggle Deafen"));
    assert_eq!(stored(&folder)["keybinds"], serde_json::json!([]));
    assert!(app.keybinds.watched.is_empty());
}

#[test]
fn closing_settings_lets_go_of_an_edited_theme() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Appearance);
    interact(&mut app, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let _ = app.update(crate::settings::appearance_event(
        crate::settings::ThemeEvent::Dark(false),
    ));
    assert!(!app.theme.dark, "shown while edited");
    let _ = app.update(Message::Settings(Event::Close));
    assert!(!app.win.settings.is_open());
    assert_eq!(app.theme.id, "bonfire", "the theme from before");
}

#[test]
fn a_new_setting_is_tagged_until_its_page_is_left() {
    use crate::settings::{Event as Settings, Section};
    // A layout from before the tags: everything tagged since is new.
    let folder = tempfile::tempdir_in(super::local()).expect("test folder");
    std::fs::write(folder.path().join("layout.json"), b"{}").expect("layout");
    let mut app = start(&folder, true);
    let _ = app.update(Message::Settings(Settings::Open(Section::Account)));
    assert!(shows(&app, "NEW"), "About's row says so");
    let _ = app.update(Message::Settings(Settings::Open(Section::About)));
    assert!(shows(&app, "Check for updates") && shows(&app, "NEW"));
    // Leaving About sees them, and the layout keeps it.
    let _ = app.update(Message::Settings(Settings::Open(Section::Account)));
    assert!(!shows(&app, "NEW"));
    let layout: serde_json::Value = serde_json::from_slice(&{
        crate::layout::flush(folder.path());
        std::fs::read(folder.path().join("layout.json")).expect("the layout")
    })
    .expect("JSON");
    let seen: Vec<&str> = layout["seen_new"]
        .as_array()
        .expect("seen")
        .iter()
        .filter_map(|key| key.as_str())
        .collect();
    assert_eq!(seen, ["check-updates", "update-nightlies"]);
    // A fresh install has met them all.
    let (mut fresh, _folder) = preview();
    let _ = fresh.update(Message::Settings(Settings::Open(Section::About)));
    assert!(!shows(&fresh, "NEW"));
}

/// Every key the core tags has its tag in Settings' pages.
#[test]
fn every_tagged_setting_has_its_tag() {
    let pages = [
        include_str!("../settings/pages/mod.rs"),
        include_str!("../settings/pages/about.rs"),
        include_str!("../settings/pages/account.rs"),
        include_str!("../settings/pages/advanced.rs"),
        include_str!("../settings/pages/discord.rs"),
        include_str!("../settings/pages/games.rs"),
        include_str!("../settings/pages/messages.rs"),
        include_str!("../settings/pages/notifications.rs"),
        include_str!("../settings/pages/sounds.rs"),
    ]
    .concat();
    for tag in &yapfast_core::new_tags::NEW {
        assert!(
            pages.contains(&format!("self.fresh(\"{}\")", tag.key)),
            "{}",
            tag.key
        );
    }
}

#[test]
fn a_theme_switch_lays_the_timelines_text_out_in_the_new_inks() {
    use yapfast_core::chat::{ChatMessage, Notice, NoticeKind};
    let (mut app, _folder) = preview();
    super::change(&mut app, |state| {
        state.messages = std::sync::Arc::new(vec![std::sync::Arc::new(ChatMessage {
            item: "n0".into(),
            author: "Ann".into(),
            time: "12:01".into(),
            notice: Some(Notice {
                kind: NoticeKind::Pinned(Some("m1".into())),
                text: String::new(),
            }),
            ..Default::default()
        })]);
    });
    let ink = |app: &crate::app::App| {
        app.timeline.looks[0].body.parts[0]
            .runs
            .iter()
            .find(|run| run.text.contains("Ann"))
            .and_then(|run| run.color)
    };
    assert_eq!(ink(&app), Some(crate::theme::rgb(app.theme.text)));
    assert!(app.choose_theme("catppuccin-latte"), "a light theme");
    let _ = app.sync();
    assert_eq!(
        ink(&app),
        Some(crate::theme::rgb(app.theme.text)),
        "the author's name in the light theme's ink"
    );
}

#[test]
fn the_theme_command_switches_the_colours_and_keeps_the_choice() {
    let (mut app, folder) = preview();
    assert_eq!(app.theme.id, "bonfire", "the default with the Discord look");
    let _ = app.update(Message::Control("theme sakura".into()));
    assert_eq!(app.theme.id, "sakura");
    assert!(!app.theme.dark);
    assert_eq!(app.theme().palette().background, app.theme.chat);
    let layout: serde_json::Value = serde_json::from_slice(&{
        crate::layout::flush(folder.path());
        std::fs::read(folder.path().join("layout.json")).expect("the layout")
    })
    .expect("JSON");
    assert_eq!(layout["theme"], "sakura");
    // A theme Yapfast doesn't have changes nothing.
    let _ = app.update(Message::Control("theme gone".into()));
    assert_eq!(app.theme.id, "sakura");
    // The next start shows the one chosen.
    assert_eq!(start(&folder, true).theme.id, "sakura");
}

#[test]
fn a_custom_theme_in_the_layout_shows_from_the_start() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    std::fs::write(
        folder.path().join("layout.json"),
        r##"{"theme": "custom:1", "custom_themes": [{"id": "custom:1", "name": "Plum",
            "base": "yap", "colours": {"chat": "#2a1a3a", "accent": "#c04080"}}]}"##,
    )
    .expect("the layout");
    let app = start(&folder, true);
    assert_eq!(app.theme.id, "custom:1");
    assert_eq!(app.theme.chat, crate::style::hex(0x2a1a3a));
    assert_eq!(app.theme.accent, crate::style::hex(0xc04080));
    assert_eq!(app.theme.selection.a, 0.35, "worked out from its accent");
}

#[test]
fn a_layout_key_of_the_wrong_kind_loses_only_itself() {
    let folder = tempfile::tempdir_in(local()).expect("test folder");
    std::fs::write(
        folder.path().join("layout.json"),
        r#"{"theme": "sakura", "collapse_long": "yes", "members_hidden": true}"#,
    )
    .expect("the layout");
    let app = start(&folder, true);
    assert_eq!(app.theme.id, "sakura");
    assert!(app.layout.members_hidden);
    assert!(!app.layout.collapse_long);
}

#[test]
fn edit_profile_from_settings_closes_them_under_the_editor() {
    let (mut app, _folder) = preview();
    change(&mut app, |state| state.discord_user = Some("42".into()));
    open(&mut app, Section::Account);
    let _ = app.update(Message::Settings(Event::EditProfile));
    assert!(
        app.client.state().profile_editor.is_some(),
        "the editor opens"
    );
    assert!(app.win.settings.get().is_none(), "Settings close under it");
}

/// A call going on in Lounge, joined as the preview joins, unmuted
/// unless `muted`: after the join, which turns the microphone on.
fn in_a_call(app: &mut App, muted: bool) {
    for command in [
        voice::Command::Join("lounge".into()),
        voice::Command::Mute(muted),
    ] {
        app.client
            .dispatch(yapfast_core::application::Action::Voice(command));
        let _ = app.sync();
    }
    let voice = &app.client.state().voice;
    assert_eq!(
        (voice.active(), voice.muted, voice.deafened),
        (true, muted, false)
    );
}

fn testing(app: &App) -> bool {
    app.win
        .settings
        .get()
        .is_some_and(|settings| settings.voice.testing && settings.voice.gate.is_some())
}

#[test]
fn the_mic_test_deafens_a_call_while_it_plays_back_as_discords_does() {
    let (mut app, _folder) = preview();
    in_a_call(&mut app, false);
    open(&mut app, Section::Voice);
    assert!(shows(
        &app,
        "You'll be muted and deafened in your call while you test."
    ));
    interact(&mut app, |ui| {
        ui.click("Let's check").expect("the mic test's button");
    });
    assert!(testing(&app), "the meter plays the microphone back");
    let call = |app: &App| {
        let voice = &app.client.state().voice;
        (voice.muted, voice.deafened)
    };
    assert_eq!(call(&app), (true, true), "deafened, and so muted");
    assert!(shows(&app, "Stop testing"));
    interact(&mut app, |ui| {
        ui.click("Stop testing").expect("the button");
    });
    assert!(!testing(&app));
    assert_eq!(call(&app), (false, false), "as it was");
    // Undeafened while it plays, the test ends, as Discord's does.
    let _ = app.update(Message::Settings(Event::Voice(VoiceEvent::MicTest)));
    assert!(testing(&app));
    app.client
        .dispatch(yapfast_core::application::Action::Voice(
            voice::Command::Deafen(false),
        ));
    let _ = app.sync();
    assert!(!testing(&app));
    assert!(!app.client.state().voice.deafened);
}

#[test]
fn leaving_voice_or_settings_ends_the_mic_test_and_the_call_hears_again() {
    let (mut app, _folder) = preview();
    in_a_call(&mut app, true);
    open(&mut app, Section::Voice);
    let mic_test = Message::Settings(Event::Voice(VoiceEvent::MicTest));
    let _ = app.update(mic_test.clone());
    assert!(app.client.state().voice.deafened);
    let _ = app.update(Message::Settings(Event::Open(Section::Advanced)));
    let voice = &app.client.state().voice;
    assert_eq!(
        (voice.muted, voice.deafened),
        (true, false),
        "muted as before"
    );
    open(&mut app, Section::Voice);
    assert!(!testing(&app), "it doesn't start again");
    let _ = app.update(mic_test);
    assert!(app.client.state().voice.deafened);
    let _ = app.update(Message::Settings(Event::Close));
    assert!(!app.client.state().voice.deafened, "closing ends it too");
}

#[test]
fn the_mic_test_without_a_call_deafens_nothing() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Voice);
    assert!(!shows(
        &app,
        "You'll be muted and deafened in your call while you test."
    ));
    let call = |app: &App| {
        let voice = &app.client.state().voice;
        (voice.muted, voice.deafened)
    };
    let before = call(&app);
    let _ = app.update(Message::Settings(Event::Voice(VoiceEvent::MicTest)));
    assert!(testing(&app));
    assert_eq!(call(&app), before);
}
