use serde_json::{Value, json};
use yapfast_core::application::{Action, Application};
use yapfast_core::sounds::Sound;

use super::*;

/// A layout as the Slint app writes it: every key of its `Layout`, those
/// whose types it keeps to itself too, and a key a newer version added.
fn slint_written() -> Value {
    serde_json::from_str(SLINT_WRITTEN).unwrap()
}

const SLINT_WRITTEN: &str = r##"{
    "sidebar_width": 291.5, "aside_width": null, "members_width": 264.0,
    "call_height": null, "picker_width": 512.0, "members_hidden": true,
    "profile_panel_hidden": false, "zoom": 1.1,
    "channels": {"discord:1": "discord:11", "discord:2": "discord:22"},
    "collapsed": {"discord:1": ["discord:5"]}, "open_folders": ["7"],
    "reactions": {"👍": 4}, "sticker_uses": [{"id": "9", "name": "Wave", "format": 1,
        "picture": "https://media/9.png", "uses": 3}],
    "sticker_favorites": [{"id": "9", "name": "Wave", "format": 1, "picture": "https://media/9.png"}],
    "show_hidden_channels": true, "sync_discord_reads": false, "show_idle": null,
    "game_executables": [["game.exe", "A Game"]], "hide_blocked": true,
    "skip_call_prompt": false, "activity_cards_hidden": true, "flash_taskbar": false,
    "pinned": ["discord:dm:3", "discord:dm:4"], "language": "en",
    "twelve_hour_clock": true, "clock_seconds": false, "date_format": 2, "text_size": 1,
    "compact_messages": false, "link_previews": true, "boost_gems": null,
    "reply_ping": false, "strip_tracking": true, "nameplates": false,
    "collapse_long": true, "animate": "never", "profile_effects": false,
    "decorations": true, "moving_cosmetics": null, "name_styles": true,
    "server_tags": false, "profile_frames": true, "unmute_on_join": false,
    "deafen_on_join": true, "alone_timeout": 5, "level_voice": true,
    "noise_suppression": "rnnoise", "noise_strength": "light",
    "reduce_room_echo": false, "windows_microphone_effects": false,
    "input_sensitivity": -52, "input_mode": "push_to_talk",
    "push_to_talk_key": {"code": "KeyV", "modifiers": ["ctrl"]},
    "push_to_talk_release_delay": 40,
    "keybinds": [{"action": "toggle_mute", "keys": {"code": "KeyM", "modifiers": []}}],
    "microphone_volume": 120, "speaker_volume": 90,
    "volumes": {"1016732398987264101": 1.25}, "stream_volumes": {},
    "muted_people": ["5"], "soundboard_muted_people": [], "share_typing": true,
    "notifications": true, "notification_text": false, "sound_set": "felt",
    "sound_volume": 80, "adjust_each_sound": true, "each_sound": {"message": 50},
    "gpu_rendering": false, "gpu_encoding": true, "theme": "nord",
    "custom_themes": [{"name": "Mine", "colors": {"accent": "#ff0000"}}],
    "close_to_tray": true, "multiple_windows": false, "developer_mode": true,
    "check_for_updates": false, "update_nightlies": false, "skipped_update": null,
    "announced_update": "v0.9.0", "first_version": "0.8.0", "seen_new": ["zoom"],
    "system_title_bar": null,
    "ui_fonts": [{"name": "Inter", "file": "fonts/inter.ttf"}],
    "window": {"x": 10, "y": 20, "width": 1400, "height": 860, "maximized": false},
    "a_newer_setting": {"kept": [1, 2]}
    }"##;

/// A folder of its own for each test.
fn folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("anomi-layout-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn written(folder: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(folder.join(FILE)).unwrap()).unwrap()
}

#[test]
fn a_slint_layout_opens_unchanged_and_keeps_every_key() {
    let folder = folder("slint");
    let file = serde_json::to_vec(&slint_written()).unwrap();
    std::fs::write(folder.join(FILE), &file).unwrap();
    let mut layout = Layout::read(&folder);
    assert_eq!(layout.zoom, Some(1.1));
    assert!(layout.members_hidden);
    assert_eq!(layout.pinned, ["discord:dm:3", "discord:dm:4"]);
    assert_eq!(layout.animate, Animate::Never);
    assert_eq!(layout.input_mode, InputMode::PushToTalk);
    assert!(layout.odd.is_empty());
    // Opening writes nothing, and neither does a wait with nothing changed.
    assert_eq!(layout.wait(), None);
    layout.flush();
    assert_eq!(std::fs::read(folder.join(FILE)).unwrap(), file);

    layout.edit(|saved| saved.sidebar_width = Some(300.0));
    layout.flush();
    let mut expected = slint_written();
    expected["sidebar_width"] = json!(300.0);
    assert_eq!(written(&folder), expected);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn the_old_name_of_the_pickers_width_reads() {
    let layout = Layout::from_values(
        PathBuf::new(),
        serde_json::from_str(r#"{"gif_picker_width": 480.0}"#).unwrap(),
    );
    assert_eq!(layout.picker_width, Some(480.0));
    assert!(layout.other.is_empty());
}

#[test]
fn a_setting_that_doesnt_read_loses_nothing_else() {
    let folder = folder("odd");
    std::fs::write(
        folder.join(FILE),
        r#"{"zoom":"big","members_hidden":true,"theme":"nord"}"#,
    )
    .unwrap();
    let mut layout = Layout::read(&folder);
    assert_eq!(layout.zoom, None);
    assert!(layout.members_hidden);
    assert_eq!(layout.theme(), Some("nord"));
    layout.edit(|saved| saved.members_hidden = false);
    layout.flush();
    assert_eq!(
        written(&folder)["zoom"],
        json!("big"),
        "kept while unset here"
    );
    assert_eq!(written(&folder)["members_hidden"], json!(false));
    layout.edit(|saved| saved.zoom = Some(1.2));
    layout.flush();
    assert_eq!(
        written(&folder)["zoom"].as_f64().map(|zoom| zoom as f32),
        Some(1.2)
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn quiet_notifications_keep_messages_quiet() {
    let layout = Layout::from_values(
        PathBuf::new(),
        serde_json::from_str(r#"{"notification_sound": false}"#).unwrap(),
    );
    assert!(layout.adjust_each_sound);
    assert_eq!(layout.each_sound.get(&Sound::Message), Some(&0));
    assert_eq!(layout.each_sound.get(&Sound::Mention), Some(&0));
    assert_eq!(layout.notification_sound, None, "read once, never written");
}

#[test]
fn changes_together_write_once_after_the_last() {
    let folder = folder("wait");
    let mut layout = Layout::read(&folder);
    layout.edit(|saved| saved.zoom = Some(1.1));
    let first = layout.wait().expect("a change waits");
    layout.edit(|saved| saved.zoom = Some(1.2));
    let second = layout.wait().expect("each change starts the wait over");
    assert_eq!(layout.wait(), None);
    layout.due(first);
    assert!(
        !folder.join(FILE).exists(),
        "a later change is still waiting"
    );
    layout.due(second);
    // The write went to the writer's thread; flushing waits for it.
    layout.flush();
    assert_eq!(
        written(&folder)["zoom"].as_f64().map(|zoom| zoom as f32),
        Some(1.2)
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn the_core_takes_the_switches_and_the_layout_keeps_what_it_changes() {
    let layout_values = serde_json::from_value(slint_written()).unwrap();
    let mut layout = Layout::from_values(PathBuf::new(), layout_values);
    let mut core = Application::default();
    carry::give(&layout, &mut core);
    let state = &core.state;
    assert!(state.show_hidden_channels);
    assert!(!state.sync_discord_reads);
    assert_eq!(state.pinned, ["discord:dm:3", "discord:dm:4"]);
    assert_eq!(core.reaction_uses().get("👍"), Some(&4));
    assert_eq!(core.sticker_uses().len(), 1);
    assert_eq!(core.sticker_favorites().len(), 1);
    carry::follow(&mut layout, &core);
    assert_eq!(layout.wait(), None, "what the core took back is no change");

    drop(core.dispatch(Action::SetPinned(vec!["discord:dm:4".into()])));
    carry::follow(&mut layout, &core);
    assert_eq!(layout.pinned, ["discord:dm:4"]);
    assert!(layout.wait().is_some());
}

#[test]
fn a_file_that_isnt_a_layout_is_set_aside_not_overwritten() {
    let folder = std::env::temp_dir().join(format!("anomi-layout-aside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(FILE), b"{\"theme\": \"sak").unwrap();
    let mut layout = Layout::read(&folder);
    assert_eq!(layout.theme(), None);
    layout.edit(|saved| saved.zoom = Some(1.25));
    layout.flush();
    let aside = std::fs::read(folder.join(SET_ASIDE)).expect("set aside");
    assert_eq!(aside, b"{\"theme\": \"sak");
    // A layout is read, not set aside.
    assert_eq!(Layout::read(&folder).zoom, Some(1.25));
    assert!(folder.join(FILE).exists());
    std::fs::remove_dir_all(folder).unwrap();
}
