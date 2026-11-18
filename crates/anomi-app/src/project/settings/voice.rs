//! Voice & video: the devices the call lists, the layout's voice settings,
//! and the soundboard's volume and entrance sounds while the page shows
//! (`Layout::apply`, `src/ui.rs:1760`; `show_soundboard_settings`,
//! `src/ui/refresh.rs:4838`). What moves on its own (the gate's level, the
//! microphone test, the keybind being recorded) is the window's and stays
//! as the page is built again.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use yapfast_core::application::Application;
use yapfast_core::noise::{DEFAULT_SENSITIVITY, Strength, Suppressor};
use yapfast_lang::tr;

use anomi_vm::settings::voice::{Entrance, Group, Voice};

use super::Kept;
use crate::layout::{InputMode, Layout};
use crate::project::voice::devices::rows;

/// The noise suppression choices and strengths, in the page's order.
pub(crate) const SUPPRESSORS: [Suppressor; 3] = [
    Suppressor::DeepFilterNet,
    Suppressor::RNNoise,
    Suppressor::Off,
];
pub(crate) const STRENGTHS: [Strength; 3] = [Strength::Full, Strength::Medium, Strength::Light];

/// Where `chosen` is among `list`, the first for none.
fn place<T: PartialEq>(list: &[T], chosen: &T) -> usize {
    list.iter().position(|one| one == chosen).unwrap_or(0)
}

/// The voice gate's sensitivity, in whole dBFS from -80 to 0.
pub(crate) fn sensitivity(layout: &Layout) -> i32 {
    (layout.input_sensitivity)
        .unwrap_or(DEFAULT_SENSITIVITY as i32)
        .clamp(-80, 0)
}

/// The Discord servers an entrance sound can be chosen for, by guild id, in
/// the list's order after "All servers".
pub(crate) fn entrance_servers(core: &Application) -> impl Iterator<Item = (&str, &str)> {
    (core.state.servers.iter()).filter_map(|server| {
        let guild = yapfast_core::discord::parse_server(&server.id)?;
        Some((guild, server.name.as_str()))
    })
}

/// A stamp of what the page reads of the core's state.
pub(super) fn stamp(core: &Application, hasher: &mut impl Hasher) {
    let state = &core.state;
    let voice = &state.voice;
    for list in [&voice.microphones, &voice.speakers, &voice.cameras] {
        Arc::as_ptr(list).addr().hash(hasher);
    }
    (&voice.microphone, &voice.speaker, &voice.camera).hash(hasher);
    (
        voice.active(),
        super::people::linked(state),
        core.discord_nitro(),
    )
        .hash(hasher);
    state.soundboard_volume.to_bits().hash(hasher);
    (state.servers.len(), state.list_changes).hash(hasher);
    for join in &state.join_sounds {
        (&join.server, &join.sound, &join.guild).hash(hasher);
    }
    (state.sound_catalog.as_ref())
        .map(|groups| Arc::as_ptr(groups).addr())
        .hash(hasher);
}

pub fn show(core: &Application, layout: &Layout, kept: &Kept, page: &mut Voice) {
    let state = &core.state;
    let voice = &state.voice;
    let (microphones, microphone) = rows(&voice.microphones, voice.microphone.as_deref());
    let (speakers, speaker) = rows(&voice.speakers, voice.speaker.as_deref());
    let (cameras, camera) = rows(&voice.cameras, voice.camera.as_deref());
    let servers: Vec<_> = entrance_servers(core).collect();
    let server = (kept.entrance.as_deref())
        .and_then(|chosen| servers.iter().position(|(guild, _)| *guild == chosen))
        .map_or(0, |at| at + 1);
    let built = Voice {
        microphones,
        speakers,
        cameras,
        microphone: microphone.unwrap_or(0),
        speaker: speaker.unwrap_or(0),
        camera: camera.unwrap_or(0),
        unmute_on_join: layout.unmute_on_join(),
        alone_wait: super::pages::alone_choice(layout),
        level_voice: layout.level_voice.unwrap_or(true),
        noise_suppression: place(&SUPPRESSORS, &layout.noise_suppression),
        noise_strength: place(&STRENGTHS, &layout.noise_strength),
        dereverb: layout.reduce_room_echo,
        windows_effects: layout.windows_microphone_effects,
        windows_effects_choice: cfg!(windows),
        sensitivity: sensitivity(layout),
        calling: voice.active(),
        input_mode: usize::from(layout.input_mode == InputMode::PushToTalk),
        talk_delay: layout.release_delay() as i32,
        discord: super::people::linked(state),
        microphone_volume: layout.microphone_volume.map_or(100, i32::from),
        speaker_volume: layout.speaker_volume.map_or(100, i32::from),
        soundboard_volume: yapfast_core::volume::to_slider(state.soundboard_volume / 100.0).round()
            as i32,
        entrance_servers: std::iter::once(tr!("All servers").to_owned())
            .chain(servers.iter().map(|(_, name)| (*name).to_owned()))
            .collect(),
        entrance_server: server,
        entrance_sound: entrance_sound(core, kept.entrance.as_deref()),
        entrance_groups: entrance_groups(core),
        nitro: core.discord_nitro(),
        ..moving(page)
    };
    if *page != built {
        *page = built;
    }
}

/// What the window moves on its own, kept as the page is built again.
fn moving(page: &Voice) -> Voice {
    Voice {
        camera_testing: page.camera_testing,
        camera_preview: page.camera_preview.clone(),
        camera_note: page.camera_note.clone(),
        gate: page.gate.clone(),
        gate_sending: page.gate_sending,
        gate_note: page.gate_note.clone(),
        microphone_testing: page.microphone_testing && !page.calling,
        microphone_test_note: page.microphone_test_note.clone(),
        talk_key: page.talk_key.clone(),
        talk_recording: page.talk_recording,
        recorded_keys: page.recorded_keys.clone(),
        talk_note: page.talk_note.clone(),
        entrance_open: page.entrance_open,
        ..Voice::default()
    }
}

/// The entrance sound chosen for `server`, by its name.
fn entrance_sound(core: &Application, server: Option<&str>) -> String {
    let state = &core.state;
    let catalog = state.sound_catalog.as_ref();
    let Some(join) = (state.join_sounds.iter()).find(|join| join.server.as_deref() == server)
    else {
        return String::new();
    };
    let named = (catalog.iter())
        .flat_map(|groups| groups.iter())
        .flat_map(|group| &group.sounds)
        .find(|sound| sound.id == join.sound && sound.guild == join.guild);
    match (named, catalog) {
        (Some(sound), _) => sound.name.clone(),
        (None, None) => tr!("Loading...").to_owned(),
        (None, Some(_)) => tr!("A sound that's gone").to_owned(),
    }
}

/// The sounds an entrance sound can be, under their servers' names, once
/// the page asked for them.
fn entrance_groups(core: &Application) -> Vec<Group> {
    let Some(groups) = &core.state.sound_catalog else {
        return Vec::new();
    };
    (groups.iter())
        .map(|group| Group {
            heading: group.name.to_uppercase(),
            sounds: (group.sounds.iter())
                .map(|sound| Entrance {
                    id: sound.id.clone(),
                    name: sound.name.clone(),
                })
                .collect(),
        })
        .collect()
}
