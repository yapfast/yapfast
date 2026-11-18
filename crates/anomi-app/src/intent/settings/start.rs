//! What the person's settings set up as Anomi starts, before the window
//! opens, as the Slint app's `Layout::read` and `Layout::apply` do
//! (`src/ui.rs:1625`, `:1728`) and its start (`:173`-`:193`): the person's
//! own themes and the one chosen, their font files, what the call's audio
//! and the clocks take of the layout, and the desktop's entry, links and
//! start at sign-in.

use serde_json::{Value, json};

use anomi_theme::custom::{self, Custom};
use anomi_theme::fonts::custom::{FOLDER, Loaded, Stored};

use crate::app::App;
use crate::layout::Saved;

/// Sets up what the settings keep, as Anomi starts (`preview` for the local
/// preview, which leaves the desktop alone).
pub fn start(app: &mut App, preview: bool) {
    let mut themes: Vec<Custom> = (app.layout.custom_themes.iter())
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect();
    custom::tidy(&mut themes);
    app.theme = custom::chosen(app.layout.theme(), &themes);
    let kept = &mut app.shown.settings.kept;
    kept.custom = themes;
    let fonts = stored_fonts(&app.layout.ui_fonts);
    if !fonts.is_empty() {
        let folder = app.root.join(FOLDER);
        kept.fonts = Loaded::load(&folder, Loaded::default(), &fonts);
    }
    apply(&app.layout);
    if !preview {
        // Before the window maps, as the desktop looks up its icon then.
        #[cfg(target_os = "linux")]
        anomi_desktop::desktop_entry::install();
        if cfg!(feature = "discord") && anomi_desktop::protocol::SUPPORTED {
            anomi_desktop::protocol::refresh(&app.root);
        }
        anomi_desktop::autostart::refresh();
    }
}

/// The font files `layout.json` keeps, as the Slint app writes them.
fn stored_fonts(values: &[Value]) -> Vec<Stored> {
    let text = |value: &Value, key| value.get(key)?.as_str().map(str::to_owned);
    (values.iter())
        .filter_map(|value| {
            Some(Stored {
                name: text(value, "name")?,
                file: text(value, "file")?,
            })
        })
        .collect()
}

/// A font file as `layout.json` keeps it.
pub(super) fn font_value(stored: &Stored) -> Value {
    json!({ "name": stored.name, "file": stored.file })
}

/// Hands the call's audio, the clocks, the stream's encoder and the game
/// detection what the layout says, as the Slint app's `Layout::apply`.
fn apply(saved: &Saved) {
    use yapfast_core::{leveling, noise, volume};
    super::prefs::clock(saved);
    leveling::set_enabled(saved.level_voice.unwrap_or(true));
    noise::set(saved.noise_suppression);
    noise::set_strength(saved.noise_strength);
    noise::set_dereverb(saved.reduce_room_echo);
    yapfast_core::set_windows_effects(saved.windows_microphone_effects);
    let sensitivity = (saved.input_sensitivity)
        .unwrap_or(noise::DEFAULT_SENSITIVITY as i32)
        .clamp(-80, 0);
    noise::set_sensitivity(sensitivity as f32);
    let gain = |value: Option<u8>| volume::from_slider(f32::from(value.unwrap_or(100)));
    volume::set_microphone(gain(saved.microphone_volume));
    volume::set_speakers(gain(saved.speaker_volume));
    for (person, gain) in &saved.volumes {
        volume::set(volume::Kind::Voice, person, *gain);
    }
    for (person, gain) in &saved.stream_volumes {
        volume::set(volume::Kind::Stream, person, *gain);
    }
    for person in &saved.muted_people {
        volume::mute(person, true);
    }
    for person in &saved.soundboard_muted_people {
        volume::mute_soundboard(person, true);
    }
    #[cfg(feature = "discord")]
    {
        yapfast_core::stream_worker::set_hardware_encoding(saved.gpu_encoding.unwrap_or(true));
        yapfast_core::discord::set_custom_games(&saved.game_executables);
    }
    let moving = crate::layout::carry::cosmetics(saved).moving;
    crate::pictures::set_moving_collectibles(moving);
}
