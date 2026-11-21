//! Settings: a theme chosen in Appearance shows at once and is kept, a
//! custom theme made, edited, deleted and shared as a file there,
//! preferences kept across a restart, a device chosen in Voice & video, and
//! push to talk's key recorded and kept.

use iced::{
    Point,
    keyboard::{Key, Location, key},
    mouse,
    widget::Id,
};
use yapfast_core::voice;

use super::{change, interact, local, preview, shows, start};
use crate::{
    app::{
        App, Message,
        layers::{Layer, Popup},
    },
    settings::{
        Event, Section, THEME_COLOUR_CODE, ThemeEvent, appearance_event, voice::Event as VoiceEvent,
    },
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
fn a_custom_theme_is_exported_from_the_editor_and_imported_by_another_install() {
    use crate::settings::{ThemeEvent, appearance_event as theme};
    use yapfast_core::theme::Theme;
    let files = tempfile::tempdir_in(super::local()).expect("test folder");
    let (mut app, _folder) = preview();
    open(&mut app, Section::Appearance);
    let file = |app: &App| app.win.settings.get().expect("Settings").theme_file();
    let note = |app: &App| app.win.settings.get().expect("Settings").theme_note.clone();

    // Nothing to export outside the editor, or while a field holds no colour.
    assert!(file(&app).is_none());
    let _ = app.update(theme(ThemeEvent::Choose("tokyo-night".into())));
    interact(&mut app, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let _ = app.update(theme(ThemeEvent::Name("Night / Shift".into())));
    let chat = Theme::NAMES
        .iter()
        .position(|name| *name == "chat")
        .expect("chat");
    let _ = app.update(theme(ThemeEvent::Colour(chat, "#102030".into())));
    let _ = app.update(theme(ThemeEvent::Dark(false)));
    let made = app
        .win
        .settings
        .get()
        .and_then(|settings| settings.editor.as_ref())
        .map(|editor| editor.theme)
        .expect("the editor");
    let _ = app.update(theme(ThemeEvent::Colour(chat, "#10".into())));
    assert!(file(&app).is_none());
    let _ = app.update(theme(ThemeEvent::Colour(chat, "#102030".into())));

    // The unsaved edit exports as it stands.
    let (file_name, text) = file(&app).expect("a theme to export");
    assert_eq!(file_name, "Night - Shift.yapfast-theme.json");
    let path = files.path().join(&file_name);
    std::fs::write(&path, text).expect("the file");
    let _ = app.update(theme(ThemeEvent::Exported(file_name.clone(), Ok(()))));
    assert!(shows(&app, "Exported to Night - Shift.yapfast-theme.json."));
    assert_eq!(note(&app).map(|(_, wrong)| wrong), Some(false));
    let _ = app.update(theme(ThemeEvent::Exported(
        file_name,
        Err("disk full".into()),
    )));
    assert!(shows(&app, "Couldn't save the theme file: disk full"));
    assert_eq!(note(&app).map(|(_, wrong)| wrong), Some(true));
    interact(&mut app, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert_eq!(note(&app), None, "gone with the editor");

    // Another install loads it as a theme of its own, chosen, and keeps it.
    let (mut other, other_folder) = preview();
    open(&mut other, Section::Appearance);
    let _ = other.update(theme(ThemeEvent::Imported(path.clone())));
    assert_eq!(other.theme.id, "custom:1");
    assert_eq!(other.theme.chat, crate::style::hex(0x102030));
    assert!(!other.theme.dark);
    assert_eq!(other.themes.custom.len(), 1);
    assert_eq!(other.themes.custom[0].name, "Night / Shift");
    assert_eq!(other.themes.custom[0].base, "tokyo-night");
    assert_eq!(other.themes.custom[0].theme, made);
    assert!(shows(&other, "Night / Shift"), "among the tiles");
    assert!(shows(&other, "Imported Night / Shift."));
    assert_eq!(note(&other).map(|(_, wrong)| wrong), Some(false));
    let layout = stored(&other_folder);
    assert_eq!(layout["theme"], "custom:1");
    assert_eq!(layout["custom_themes"][0]["base"], "tokyo-night");
    assert_eq!(layout["custom_themes"][0]["colours"]["chat"], "#102030");

    // Importing it again adds another with its own id.
    let _ = other.update(theme(ThemeEvent::Imported(path.clone())));
    assert_eq!(other.theme.id, "custom:2");
    assert_eq!(other.themes.custom.len(), 2);

    // A file that isn't a theme, or can't be read, changes nothing but the
    // note.
    let junk = files.path().join("junk.json");
    std::fs::write(&junk, "{}").expect("the file");
    let _ = other.update(theme(ThemeEvent::Imported(junk)));
    assert!(shows(&other, "That file is not a Yapfast theme."));
    assert_eq!(note(&other).map(|(_, wrong)| wrong), Some(true));
    let missing = files.path().join("missing.json");
    let _ = other.update(theme(ThemeEvent::Imported(missing)));
    assert!(note(&other).is_some_and(|(note, wrong)| wrong && note.starts_with("Couldn't read")));
    assert_eq!(other.theme.id, "custom:2");
    assert_eq!(other.themes.custom.len(), 2);

    // The editor owns the window's colours while it is open.
    interact(&mut other, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let _ = other.update(theme(ThemeEvent::Imported(path)));
    assert!(shows(
        &other,
        "Save or cancel the theme being edited first."
    ));
    assert_eq!(other.themes.custom.len(), 2);
    interact(&mut other, |ui| {
        ui.click("Cancel").expect("Cancel");
    });
    assert_eq!(note(&other), None);
}

#[test]
fn the_theme_editors_colour_picker_opens_picks_and_dismisses() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Appearance);
    interact(&mut app, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let chat = yapfast_core::theme::Theme::NAMES
        .iter()
        .position(|name| *name == "chat")
        .expect("chat");
    let picker = |app: &App| {
        app.win
            .settings
            .get()
            .and_then(|settings| settings.editor.as_ref())
            .and_then(|editor| editor.picker)
    };
    let swatch_at = |ui: &mut iced_test::Simulator<'_, Message>, name: &str| {
        let label = ui
            .find(name)
            .expect(name)
            .visible_bounds()
            .expect("in view");
        Point::new(label.x - 10.0 - 14.0, label.center_y())
    };
    let click_at = |ui: &mut iced_test::Simulator<'_, Message>, point: Point| {
        ui.point_at(point);
        ui.simulate([
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
    };

    // Clicking a colour's swatch opens its ColourPad card under it.
    assert_eq!(picker(&app), None);
    interact(&mut app, |ui| {
        let swatch = swatch_at(ui, "chat");
        click_at(ui, swatch);
    });
    assert_eq!(picker(&app), Some(chat));

    // Clicking on the ColourPad's hue strip and square updates the hex
    // field and live preview immediately.
    let before = app.theme.chat;
    interact(&mut app, |ui| {
        let code = ui
            .find(Id::new(THEME_COLOUR_CODE))
            .expect("the picker's code field")
            .bounds();
        // One third across the hue strip (10 px tall, 14 px above the code field).
        click_at(ui, Point::new(code.x + code.width / 3.0, code.y - 19.0));
        // Upper-right of the saturation/brightness square (150 px tall, 34 px above code).
        click_at(ui, Point::new(code.x + code.width - 2.0, code.y - 180.0));
    });
    assert_ne!(app.theme.chat, before, "live preview updated from pad");
    let pad_hex = app
        .win
        .settings
        .get()
        .and_then(|settings| settings.editor.as_ref())
        .expect("editor")
        .fields[chat]
        .1
        .clone();
    assert!(shows(&app, &pad_hex), "hex field updated from pad");

    // Exact HSV picks update hex + preview, and hue stays while on black/grey.
    let _ = app.update(appearance_event(ThemeEvent::Hsv([240.0, 1.0, 0.5])));
    assert_eq!(app.theme.chat, crate::style::hex(0x000080));
    assert!(shows(&app, "#000080"));
    let _ = app.update(appearance_event(ThemeEvent::Hsv([120.0, 0.0, 0.0])));
    assert_eq!(app.theme.chat, crate::style::hex(0x000000));
    let hsv_hue = app
        .win
        .settings
        .get()
        .and_then(|settings| settings.editor.as_ref())
        .expect("editor")
        .hsv[0];
    assert_eq!(hsv_hue, 120.0, "hue kept while on black");
    let _ = app.update(appearance_event(ThemeEvent::Hsv([120.0, 1.0, 1.0])));
    assert_eq!(app.theme.chat, crate::style::hex(0x00ff00));

    // Typing a hex code into the picker's field updates the preview and pad.
    interact(&mut app, |ui| {
        ui.click(Id::new(THEME_COLOUR_CODE))
            .expect("the picker's code field");
        ui.tap_key(Key::Named(key::Named::End));
        for _ in 0..7 {
            ui.tap_key(Key::Named(key::Named::Backspace));
        }
        ui.typewrite("#123456");
    });
    assert_eq!(app.theme.chat, crate::style::hex(0x123456));
    assert!(shows(&app, "#123456"));

    // Clicking outside the popover dismisses the picker without closing Settings.
    interact(&mut app, |ui| {
        click_at(ui, Point::new(10.0, 10.0));
    });
    assert_eq!(picker(&app), None);
    assert!(app.win.settings.is_open());
    assert_eq!(app.theme.chat, crate::style::hex(0x123456));

    // Reopening and pressing Escape closes the picker first, then Settings.
    interact(&mut app, |ui| {
        let swatch = swatch_at(ui, "chat");
        click_at(ui, swatch);
    });
    assert_eq!(picker(&app), Some(chat));
    let _ = app.update(Message::Key(
        Key::Named(key::Named::Escape),
        Default::default(),
    ));
    assert_eq!(picker(&app), None);
    assert!(app.win.settings.is_open(), "Settings stays open");
    let _ = app.update(Message::Key(
        Key::Named(key::Named::Escape),
        Default::default(),
    ));
    assert!(!app.win.settings.is_open());
}

#[test]
fn the_theme_colour_picker_preserves_alpha_on_translucent_tokens() {
    let (mut app, _folder) = preview();
    open(&mut app, Section::Appearance);
    interact(&mut app, |ui| {
        ui.click("New theme").expect("New theme");
    });
    let accent_soft = yapfast_core::theme::Theme::NAMES
        .iter()
        .position(|name| *name == "accent_soft")
        .expect("accent_soft");
    let _ = app.update(appearance_event(ThemeEvent::Picker(Some(accent_soft))));
    let _ = app.update(appearance_event(ThemeEvent::Hsv([120.0, 1.0, 1.0])));
    let hex = &app
        .win
        .settings
        .get()
        .and_then(|settings| settings.editor.as_ref())
        .expect("editor")
        .fields[accent_soft]
        .1;
    assert_eq!(hex, "#00ff0026", "alpha kept on translucent token");
    assert_eq!(
        app.theme.accent_soft,
        iced::Color::from_rgba8(0, 255, 0, f32::from(0x26u8) / 255.0)
    );
    // Shorthand #rgb and #rrggbbaa work too, and picking preserves the typed alpha.
    let _ = app.update(appearance_event(ThemeEvent::Colour(
        accent_soft,
        "#345".into(),
    )));
    assert_eq!(app.theme.accent_soft, crate::style::hex(0x334455));
    let _ = app.update(appearance_event(ThemeEvent::Colour(
        accent_soft,
        "#11223380".into(),
    )));
    let _ = app.update(appearance_event(ThemeEvent::Hsv([0.0, 1.0, 1.0])));
    let hex = &app
        .win
        .settings
        .get()
        .and_then(|settings| settings.editor.as_ref())
        .expect("editor")
        .fields[accent_soft]
        .1;
    assert_eq!(hex, "#ff000080");
    // Switching the Start From template closes any open picker.
    let _ = app.update(appearance_event(ThemeEvent::Start(0)));
    assert_eq!(
        app.win
            .settings
            .get()
            .and_then(|settings| settings.editor.as_ref())
            .and_then(|editor| editor.picker),
        None
    );
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

/// The language chosen in Appearance reads at once in the page itself and
/// is kept under the key older versions wrote.
#[test]
fn the_language_chosen_in_appearance_changes_every_text_and_is_kept() {
    let (mut app, folder) = preview();
    open(&mut app, Section::Appearance);
    assert!(shows(&app, "LANGUAGE"));
    interact(&mut app, |ui| {
        ui.click("English (US)").expect("the language");
    });
    interact(&mut app, |ui| {
        ui.click("Deutsch").expect("German");
    });
    assert_eq!(app.layout.prefs.interface_language.as_deref(), Some("de"));
    assert!(shows(&app, "SPRACHE"), "the heading in German");
    assert!(!shows(&app, "LANGUAGE"), "none left in English");
    assert_eq!(stored(&folder)["interface_language"], "de");
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
    assert!(!shows(&app, "Soundboard volume"));
    interact(&mut app, |ui| {
        ui.click("Show additional audio settings")
            .expect("the additional settings switch");
    });
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
    interact(&mut app, |ui| {
        ui.click("Show additional audio settings")
            .expect("the additional settings switch");
    });
    assert!(!shows(&app, "Soundboard volume"));
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
    open(&mut app, Section::Account);
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

/// Appearance's Font lists no file until one is chosen. A file that is no
/// font is left out, with the reason under the buttons, and isn't kept for
/// the next start. (A real font isn't loaded here: the font database is the
/// process's, and the other tests measure their text in DM Sans.)
#[test]
fn a_file_that_is_no_font_is_left_out_of_appearances_font() {
    use crate::settings::{FontEvent, font_message};
    let (mut app, folder) = preview();
    open(&mut app, Section::Appearance);
    assert!(shows(&app, "FONT") && shows(&app, "Choose font files"));
    assert!(!shows(&app, "Reset to default"), "nothing to reset yet");
    let words = folder.path().join("notes.ttf");
    std::fs::write(&words, b"only words, no font tables in here").expect("a file");
    let _ = app.update(font_message(FontEvent::Chosen(vec![
        words.to_str().expect("a path").to_owned(),
    ])));
    assert!(
        shows(
            &app,
            "notes.ttf: Not a font file. Yapfast reads TTF, OTF, TTC and WOFF."
        ),
        "the note names the file and says why"
    );
    assert!(!shows(&app, "Reset to default"), "nothing is loaded");
    assert_eq!(stored(&folder)["ui_font_files"], serde_json::json!([]));
    assert!(app.layout.prefs.ui_font_files.is_empty());
    // Going back to DM Sans with nothing loaded is no change either.
    let _ = app.update(font_message(FontEvent::Reset));
    assert!(!shows(
        &app,
        "notes.ttf: Not a font file. Yapfast reads TTF, OTF, TTC and WOFF."
    ));
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
fn the_mic_test_deafens_a_call_while_it_plays_back() {
    let (mut app, _folder) = preview();
    in_a_call(&mut app, false);
    open(&mut app, Section::Voice);
    change(&mut app, |state| {
        state.voice.microphones = vec![voice::Device {
            id: "test-mic".into(),
            name: "Test microphone device".into(),
        }];
        state.voice.speakers = vec![voice::Device {
            id: "test-speaker".into(),
            name: "Test speaker device".into(),
        }];
    });
    assert!(shows(
        &app,
        "The test temporarily mutes your microphone and call audio."
    ));
    interact(&mut app, |ui| {
        ui.click("Test microphone").expect("the mic test's button");
    });
    assert!(testing(&app), "the meter plays the microphone back");
    let call = |app: &App| {
        let voice = &app.client.state().voice;
        (voice.muted, voice.deafened)
    };
    assert_eq!(call(&app), (true, true), "deafened, and so muted");
    assert!(shows(&app, "Stop microphone test"));
    interact(&mut app, |ui| {
        ui.click("Stop microphone test").expect("the button");
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
        "The test temporarily mutes your microphone and call audio."
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
