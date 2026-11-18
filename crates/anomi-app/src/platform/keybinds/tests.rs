//! Keybinds: what's watched, what a toggle does with the window and in the
//! tray, the keys that don't type, a watcher's old word, and recording.

use std::sync::Mutex;

use anomi_desktop::keybinds::{Action, Binding, Key, Reach, Shortcut, Status, TALK_SHORTCUT};
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{Key as Logical, Location};
use yapfast_core::voice::Phase;

use super::record::{self, Input, Target};
use super::*;
use crate::project::voice::tests::in_call;

/// The tests that change push to talk's and recording's process-wide state
/// take turns.
static GLOBAL: Mutex<()> = Mutex::new(());

fn keys(scancode: u32, name: &str) -> Shortcut {
    Shortcut::new(vec![Key {
        scancode,
        name: name.into(),
        trigger: Some(name.to_lowercase()),
    }])
    .unwrap()
}

fn binding(id: u32, action: Action, keys: Option<Shortcut>) -> Binding {
    Binding { id, action, keys }
}

#[test]
fn push_to_talk_is_watched_only_in_push_to_talk_and_a_keybind_only_with_keys() {
    let mut saved = Saved {
        push_to_talk_key: Some(keys(47, "V")),
        keybinds: vec![
            binding(1, Action::ToggleMute, Some(keys(50, "M"))),
            binding(2, Action::PushToTalk, Some(keys(48, "B"))),
            binding(3, Action::ToggleDeafen, None),
            binding(4, Action::Unassigned, Some(keys(32, "D"))),
        ],
        ..Saved::default()
    };
    let ids = |saved: &Saved| -> Vec<String> {
        watched(saved)
            .into_iter()
            .map(|watched| watched.id)
            .collect()
    };
    assert_eq!(ids(&saved), ["keybind-1"]);
    saved.input_mode = InputMode::PushToTalk;
    assert_eq!(ids(&saved), [TALK_SHORTCUT, "keybind-1", "keybind-2"]);
}

#[test]
fn the_keys_that_talk_dont_type_while_the_call_hears_the_microphone() {
    let (mut app, folder) = in_call("keybinds-talk");
    app.layout.edit(|saved| {
        saved.input_mode = InputMode::PushToTalk;
        saved.push_to_talk_key = Some(keys(47, "V"));
    });
    watch(&mut app);
    assert!(
        app.desktop.keybinds.watcher.is_none(),
        "no channel, no watcher"
    );
    assert_eq!(talk_keys(&app), [Physical::Code(Code::KeyV)]);
    app.core.state.voice.muted = true;
    assert!(talk_keys(&app).is_empty(), "muted, V types");
    app.core.state.voice.muted = false;
    app.core.state.voice.phase = Phase::Disconnected;
    app.core.state.voice.room = None;
    assert!(talk_keys(&app).is_empty(), "out of a call, V types");
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn toggle_vad_switches_the_input_mode_and_what_is_watched() {
    let _turn = GLOBAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (mut app, folder) = in_call("keybinds-vad");
    app.layout
        .edit(|saved| saved.push_to_talk_key = Some(keys(47, "V")));
    watch(&mut app);
    drop(toggled(&mut app, Action::ToggleVoiceMode));
    assert_eq!(app.layout.input_mode, InputMode::PushToTalk);
    assert!(yapfast_core::push_to_talk::on());
    assert_eq!(app.desktop.keybinds.watched[0].id, TALK_SHORTCUT);
    drop(toggled(&mut app, Action::ToggleVoiceMode));
    assert_eq!(app.layout.input_mode, InputMode::VoiceActivity);
    assert!(!yapfast_core::push_to_talk::on());
    assert!(app.desktop.keybinds.watched.is_empty());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn out_of_a_call_mute_says_how_the_next_starts_but_not_from_the_tray() {
    let (mut app, folder) = in_call("keybinds-mute");
    app.core.state.voice = Default::default();
    let unmuted = app.layout.unmute_on_join();
    drop(toggled(&mut app, Action::ToggleMute));
    assert_ne!(app.layout.unmute_on_join(), unmuted, "as the card's button");
    app.hidden = true;
    drop(toggled(&mut app, Action::ToggleMute));
    assert_ne!(
        app.layout.unmute_on_join(),
        unmuted,
        "no call, nothing in the tray"
    );
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn an_old_watchers_word_is_let_go_and_a_new_ones_shows() {
    let (mut app, folder) = in_call("keybinds-told");
    app.layout.edit(|saved| {
        saved.input_mode = InputMode::PushToTalk;
        saved.push_to_talk_key = Some(keys(47, "V"));
        saved.keybinds = vec![binding(1, Action::ToggleMute, Some(keys(50, "M")))];
    });
    watch(&mut app);
    let count = app.desktop.keybinds.count;
    let status = |reach| Status {
        reach,
        given: [("keybind-1".to_owned(), "CTRL+M".to_owned())].into(),
    };
    drop(told(
        &mut app,
        count - 1,
        Told::Status(status(Reach::Desktop)),
    ));
    assert!(app.vm.settings.keybinds.note.is_empty(), "an old watch's");
    drop(told(&mut app, count, Told::Status(status(Reach::Desktop))));
    let page = &app.vm.settings.keybinds;
    assert!(!page.note.is_empty());
    assert_eq!(page.rows[0].keys, "M");
    assert!(page.rows[0].desktop.contains("CTRL+M"));
    assert!(!app.vm.settings.voice.talk_note.is_empty());
    assert_eq!(app.vm.settings.voice.talk_key, "V");
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}

fn press(app: &mut App, key: Logical, code: Code, location: Location, pressed: bool) {
    let input = Input {
        key,
        physical: Physical::Code(code),
        location,
        pressed,
        repeat: false,
    };
    record::key(app, input);
}

#[test]
fn a_recording_takes_the_keys_held_together_and_escape_first_stops_it() {
    let _turn = GLOBAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (mut app, folder) = in_call("keybinds-record");
    record::add(&mut app);
    assert_eq!(app.layout.keybinds[0].id, 1);
    record::choose(&mut app, 0, 3);
    assert_eq!(app.layout.keybinds[0].action, Action::ToggleMute);
    record::record(&mut app, Some(Target::Row(0)));
    assert!(record::recording());
    assert_eq!(app.vm.settings.keybinds.recording, Some(0));
    let ctrl = || Logical::Named(Named::Control);
    let m = || Logical::Character("m".into());
    press(&mut app, ctrl(), Code::ControlLeft, Location::Left, true);
    press(&mut app, m(), Code::KeyM, Location::Standard, true);
    assert_eq!(app.vm.settings.keybinds.recorded.trim_end(), "Ctrl + M");
    press(&mut app, m(), Code::KeyM, Location::Standard, false);
    press(&mut app, ctrl(), Code::ControlLeft, Location::Left, false);
    assert!(!record::recording(), "done once the keys are up");
    assert_eq!(app.vm.settings.keybinds.rows[0].keys, "Ctrl + M");
    assert_eq!(app.desktop.keybinds.watched.len(), 1, "and watched");

    record::record(&mut app, Some(Target::Talk));
    assert!(app.vm.settings.voice.talk_recording);
    let escape = || Logical::Named(Named::Escape);
    press(&mut app, escape(), Code::Escape, Location::Standard, true);
    assert!(!record::recording() && !app.vm.settings.voice.talk_recording);
    assert!(
        app.layout.push_to_talk_key.is_none(),
        "Escape changes nothing"
    );
    record::remove(&mut app, 0);
    assert!(app.layout.keybinds.is_empty() && app.desktop.keybinds.watched.is_empty());
    drop(app);
    let _ = std::fs::remove_dir_all(folder);
}
