//! Voice & video, as the Slint client's `VoiceSettings`: the microphone,
//! speakers and camera with their volumes and a camera's preview, how calls
//! start and sound, noise suppression, the soundboard's volume and entrance
//! sounds, and how the microphone is sent (voice activity with the gate's
//! sensitivity, or push to talk with its key and release delay).
//!
//! While it shows it may run three things, each gone when it's left: the
//! camera being tried, the Discord gate's meter (only while the gate's
//! slider shows) and the push-to-talk key's recording.

use std::sync::{Arc, Mutex};

use iced::{
    Alignment, Color, Element, Length, Task,
    keyboard::{self, Key, Location, key},
    widget::{column, container, row, text},
};
use yapfast_core::{
    application::Action,
    noise::{Strength, Suppressor},
    picture::Feed,
    voice::Command,
};

use super::{Drop, Event as SettingsEvent, Settings};
use crate::{
    app::{App, Message},
    call::device_rows,
    controls::{Kind, action, bar, choices, slider},
    fonts::SEMIBOLD,
    form::{self, switch},
    layout::InputMode,
    pictures::Icon,
    video::Video,
};

/// The camera's preview: its key among the window's video sources, and the
/// size it's filmed at.
pub const CAMERA_TEST: &str = "camera:settings";
const PREVIEW: (u32, u32) = (400, 225);
/// How often the gate's meter is read while it shows.
pub const METER_EVERY: std::time::Duration = std::time::Duration::from_millis(50);

const SUPPRESSORS: [Suppressor; 3] = [
    Suppressor::DeepFilterNet,
    Suppressor::RNNoise,
    Suppressor::Off,
];
const STRENGTHS: [Strength; 3] = [Strength::Full, Strength::Medium, Strength::Light];

/// What Voice runs while it shows.
#[derive(Default)]
pub struct Voice {
    /// The next key pressed becomes push to talk's.
    pub recording: bool,
    /// The camera being tried, and why it couldn't start.
    pub camera: Option<(yapfast_core::camera::Camera, Feed)>,
    pub camera_note: String,
    /// The Discord gate's meter while its slider shows, and what it read.
    pub gate: Option<yapfast_core::discord_audio::GateMeter>,
    pub level: f32,
    pub sending: bool,
    /// Entrance sounds: the server chosen ("All servers" first), and the
    /// sounds to choose from while that list is open.
    pub entrance_server: usize,
}

#[derive(Clone, Debug)]
pub enum Event {
    Device(Drop, usize),
    Refresh,
    TestCamera,
    Volume(bool, u32, bool),
    UnmuteOnJoin(bool),
    LevelVoice(bool),
    Dereverb(bool),
    WindowsEffects(bool),
    Noise(usize),
    Strength(usize),
    InputMode(usize),
    Sensitivity(u32, bool),
    TalkDelay(u32, bool),
    Record(bool),
    /// A key was pressed while recording: the key as `layout.json` keeps
    /// it, or none for Escape or a key that has no scan code.
    Recorded(Option<serde_json::Value>),
    SoundboardVolume(u32, bool),
    EntranceServer(usize),
    Entrance(Option<String>),
    Meter,
}

impl App {
    /// Voice shows: the devices are listed again, the entrance sounds
    /// asked for, and the gate's meter starts if its slider shows.
    pub(super) fn enter_voice_settings(&mut self) -> Task<Message> {
        self.client.dispatch(Action::Voice(Command::Devices));
        if self.discord_linked() {
            self.client.dispatch(Action::LoadSoundCatalog);
        }
        self.meter_gate();
        self.sync()
    }

    /// Voice is left: the camera, the meter and the recording stop.
    pub(super) fn leave_voice_settings(&mut self) {
        let Some(settings) = &mut self.settings else {
            return;
        };
        let had_camera = settings.voice.camera.is_some();
        settings.voice = super::voice::Voice::default();
        if had_camera {
            self.place_videos();
        }
    }

    /// The gate's meter runs while its slider shows: on Voice, with Discord
    /// linked, in voice activity.
    fn meter_gate(&mut self) {
        let wanted = self.discord_linked()
            && self.layout.voice.input_mode == InputMode::VoiceActivity
            && self
                .settings
                .as_ref()
                .is_some_and(|settings| settings.section() == super::Section::Voice);
        let microphone = self.client.state().voice.microphone.clone();
        let Some(settings) = &mut self.settings else {
            return;
        };
        match (wanted, settings.voice.gate.is_some()) {
            (true, false) => {
                settings.voice.gate = Some(yapfast_core::discord_audio::GateMeter::start(
                    microphone.as_deref(),
                ));
            }
            (false, true) => settings.voice.gate = None,
            _ => {}
        }
    }

    /// Whether the gate's meter is read now.
    pub fn gate_metered(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|settings| settings.voice.gate.is_some())
    }

    /// Whether the next key is push to talk's.
    pub fn recording_talk_key(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|settings| settings.voice.recording)
    }

    /// The camera being tried, for the window's video sources.
    pub fn camera_test(&self) -> Option<&Feed> {
        let settings = self.settings.as_ref()?;
        settings.voice.camera.as_ref().map(|(_, feed)| feed)
    }

    pub(super) fn voice_settings_event(&mut self, event: Event) -> Task<Message> {
        let root = self.client.root().to_owned();
        let voice = &mut self.layout.voice;
        match event {
            Event::Device(drop, index) => {
                if let Some(settings) = &mut self.settings {
                    settings.open = None;
                }
                let state = &self.client.state().voice;
                let devices = match drop {
                    Drop::Microphone => &state.microphones,
                    Drop::Speaker => &state.speakers,
                    _ => &state.cameras,
                };
                let Some(id) = devices.get(index).map(|device| device.id.clone()) else {
                    return Task::none();
                };
                let command = match drop {
                    Drop::Microphone => Command::Microphone(id.clone()),
                    Drop::Speaker => Command::Speaker(id.clone()),
                    _ => Command::Camera(id.clone()),
                };
                self.client.dispatch(Action::Voice(command));
                if let Some(settings) = &mut self.settings {
                    // The meter follows the microphone chosen, and the
                    // camera being tried the camera.
                    if drop == Drop::Microphone && settings.voice.gate.is_some() {
                        settings.voice.gate =
                            Some(yapfast_core::discord_audio::GateMeter::start(Some(&id)));
                    }
                    if drop == Drop::Camera && settings.voice.camera.is_some() {
                        settings.voice.camera = None;
                        self.test_camera();
                    }
                }
            }
            Event::Refresh => self.client.dispatch(Action::Voice(Command::Devices)),
            Event::TestCamera => {
                let testing = self
                    .settings
                    .as_ref()
                    .is_some_and(|settings| settings.voice.camera.is_some());
                if let Some(settings) = &mut self.settings {
                    settings.voice.camera = None;
                    settings.voice.camera_note.clear();
                }
                if !testing {
                    self.test_camera();
                }
                self.place_videos();
                return Task::none();
            }
            Event::Volume(input, value, done) => {
                let value = value.min(200) as u8;
                let (kept, key) = if input {
                    (&mut voice.microphone_volume, "microphone_volume")
                } else {
                    (&mut voice.speaker_volume, "speaker_volume")
                };
                *kept = Some(value);
                voice.play_volumes();
                if done {
                    crate::layout::store(&root, key, value);
                }
                return Task::none();
            }
            Event::UnmuteOnJoin(on) => {
                self.layout.unmute_on_join = Some(on);
                crate::layout::store(&root, "unmute_on_join", on);
            }
            Event::LevelVoice(on) => {
                voice.level_voice = Some(on);
                yapfast_core::leveling::set_enabled(on);
                crate::layout::store(&root, "level_voice", on);
            }
            Event::Dereverb(on) => {
                voice.reduce_room_echo = on;
                yapfast_core::noise::set_dereverb(on);
                crate::layout::store(&root, "reduce_room_echo", on);
            }
            Event::WindowsEffects(on) => {
                voice.windows_microphone_effects = on;
                yapfast_core::set_windows_effects(on);
                crate::layout::store(&root, "windows_microphone_effects", on);
            }
            Event::Noise(index) => self.choose_suppressor(SUPPRESSORS[index.min(2)]),
            Event::Strength(index) => {
                voice.noise_strength = STRENGTHS[index.min(2)];
                yapfast_core::noise::set_strength(voice.noise_strength);
                crate::layout::store(&root, "noise_strength", voice.noise_strength);
            }
            Event::InputMode(index) => {
                voice.input_mode = if index == 1 {
                    InputMode::PushToTalk
                } else {
                    InputMode::VoiceActivity
                };
                yapfast_core::push_to_talk::set_on(voice.input_mode == InputMode::PushToTalk);
                crate::layout::store(&root, "input_mode", voice.input_mode);
                self.watch_talk_key();
                self.meter_gate();
            }
            Event::Sensitivity(value, done) => {
                let decibels = value as i32 - 80;
                voice.input_sensitivity = Some(decibels);
                yapfast_core::noise::set_sensitivity(decibels as f32);
                if done {
                    crate::layout::store(&root, "input_sensitivity", decibels);
                }
                return Task::none();
            }
            Event::TalkDelay(value, done) => {
                let delay = value.min(yapfast_core::push_to_talk::MOST_RELEASE_DELAY);
                voice.push_to_talk_release_delay = Some(delay);
                yapfast_core::push_to_talk::set_release_delay(delay);
                if done {
                    crate::layout::store(&root, "push_to_talk_release_delay", delay);
                }
                return Task::none();
            }
            Event::Record(on) => {
                if let Some(settings) = &mut self.settings {
                    settings.voice.recording = on;
                }
                return Task::none();
            }
            Event::Recorded(key) => {
                if let Some(settings) = &mut self.settings {
                    settings.voice.recording = false;
                }
                if let Some(key) = key {
                    tracing::info!(key = %key, "push-to-talk key recorded");
                    crate::layout::store(&root, "push_to_talk_key", &key);
                    self.layout.voice.push_to_talk_key = Some(key);
                    self.watch_talk_key();
                }
                return Task::none();
            }
            Event::SoundboardVolume(value, done) => {
                if done {
                    self.client
                        .dispatch(Action::SoundboardVolume(value.min(100) as f32));
                } else if let Some(settings) = &mut self.settings {
                    settings.voice.level = value as f32;
                }
            }
            Event::EntranceServer(index) => {
                if let Some(settings) = &mut self.settings {
                    settings.voice.entrance_server = index;
                    settings.open = None;
                }
                return Task::none();
            }
            Event::Entrance(sound) => {
                let server = self
                    .settings
                    .as_ref()
                    .and_then(|settings| {
                        entrance_servers(self)
                            .get(settings.voice.entrance_server)
                            .cloned()
                    })
                    .flatten();
                if let Some(settings) = &mut self.settings {
                    settings.open = None;
                }
                self.client.dispatch(Action::SetJoinSound { server, sound });
            }
            Event::Meter => {
                if let Some(settings) = &mut self.settings
                    && let Some(gate) = &settings.voice.gate
                {
                    settings.voice.level = gate.level().max(-100.0);
                    settings.voice.sending = gate.sending();
                }
                return Task::none();
            }
        }
        self.sync()
    }

    /// Starts trying the camera chosen, or says why it can't.
    fn test_camera(&mut self) {
        let camera = self.client.state().voice.camera.clone();
        let Some(settings) = &mut self.settings else {
            return;
        };
        let feed = Feed::new();
        feed.show_at(Some(PREVIEW));
        match yapfast_core::camera::Camera::open(camera.as_deref(), Box::new(feed.sink())) {
            Ok(camera) => settings.voice.camera = Some((camera, feed)),
            Err(error) => settings.voice.camera_note = error.to_string(),
        }
        self.place_videos();
    }

    /// Watches push to talk's key while it's the input mode, as the root
    /// crate's watcher does it; the old watcher goes first, ending its
    /// portal session.
    pub fn watch_talk_key(&mut self) {
        self.talk.watcher = None;
        let note = self.talk.note.clone();
        if let Ok(mut note) = note.lock() {
            note.clear();
        }
        let Some(watch) = crate::talk_key_watcher() else {
            return;
        };
        let Some(key) = self.layout.voice.talk_key() else {
            return;
        };
        let told = Box::new(move |words: String| {
            if let Ok(mut note) = note.lock() {
                *note = words;
            }
        });
        self.talk.watcher = watch(key, self.client.runtime(), told);
    }

    pub(super) fn voice_page<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        let theme = &self.theme;
        let state = &self.client.state().voice;
        let voice = &self.layout.voice;
        let open = settings.open;
        let linked = self.discord_linked();
        let (microphones, microphone) =
            device_rows(&state.microphones, state.microphone.as_deref());
        let (speakers, speaker) = device_rows(&state.speakers, state.speaker.as_deref());
        let (cameras, camera) = device_rows(&state.cameras, state.camera.as_deref());
        let device = |drop: Drop, options: Vec<String>, current: Option<usize>| {
            form::dropdown(
                options,
                current,
                open == Some(drop),
                Message::Settings(SettingsEvent::Drop(drop)),
                move |index| voice_message(Event::Device(drop, index)),
            )
            .view(self)
        };
        let mic_volume = u32::from(voice.microphone_volume.unwrap_or(100));
        let speaker_volume = u32::from(voice.speaker_volume.unwrap_or(100));
        let mut parts = vec![
            form::title(theme, "Voice & video"),
            form::halves(
                form::group([
                    form::heading(theme, "MICROPHONE"),
                    device(Drop::Microphone, microphones, microphone),
                    slider("Microphone volume", mic_volume, 200, |value| {
                        voice_message(Event::Volume(true, value, false))
                    })
                    .on_release(voice_message(Event::Volume(true, mic_volume, true)))
                    .view(theme),
                ]),
                form::group([
                    form::heading(theme, "SPEAKERS"),
                    device(Drop::Speaker, speakers, speaker),
                    slider("Speaker volume", speaker_volume, 200, |value| {
                        voice_message(Event::Volume(false, value, false))
                    })
                    .on_release(voice_message(Event::Volume(false, speaker_volume, true)))
                    .view(theme),
                ]),
            ),
        ];
        if state.microphones.is_empty() && state.speakers.is_empty() {
            parts.push(form::note(
                theme,
                "No audio devices found. Connect one, then refresh.",
            ));
        }
        let mut camera_group = vec![
            form::heading(theme, "CAMERA"),
            device(Drop::Camera, cameras, camera),
        ];
        if state.cameras.is_empty() {
            camera_group.push(form::note(
                theme,
                "No camera found. Connect one, then refresh.",
            ));
        }
        parts.push(form::halves(
            form::group(camera_group),
            form::group([
                form::heading(theme, "PREVIEW"),
                self.camera_preview(settings),
                form::buttons([action(
                    theme,
                    if settings.voice.camera.is_some() {
                        "Stop video"
                    } else {
                        "Test video"
                    },
                    Kind::Plain,
                    (!state.cameras.is_empty()).then(|| voice_message(Event::TestCamera)),
                )
                .small()
                .into()]),
            ]),
        ));
        parts.push(form::buttons([action(
            theme,
            "Refresh devices",
            Kind::Ghost,
            Some(voice_message(Event::Refresh)),
        )
        .small()
        .icon(self.icon(Icon::Refresh, 16, crate::theme::rgb(theme.secondary)))
        .into()]));
        parts.push(form::rule(theme));
        let unmute = self.layout.unmute_on_join.unwrap_or(true);
        parts.push(
            switch(
                "Turn on your microphone when you join a call",
                unmute,
                Some(voice_message(Event::UnmuteOnJoin(!unmute))),
            )
            .description("Otherwise you join muted, and unmute when you're ready.")
            .view(self),
        );
        let level = voice.level_voice.unwrap_or(true);
        parts.push(
            switch(
                "Even out your voice",
                level,
                Some(voice_message(Event::LevelVoice(!level))),
            )
            .description("Keeps your voice steady as you lean towards the microphone or away from it. Whispers and shouts still come through as they are.")
            .view(self),
        );
        if linked {
            parts.extend(self.discord_voice(settings));
        }
        let push = voice.input_mode == InputMode::PushToTalk;
        parts.push(form::group([
            form::heading(theme, "INPUT MODE"),
            choices(
                theme,
                &["Voice activity", "Push to talk"],
                usize::from(push),
                |index| voice_message(Event::InputMode(index)),
            ),
            form::note(
                theme,
                if push {
                    "Yapfast sends your microphone only while you hold a key."
                } else {
                    "Yapfast sends your voice when it hears you speak."
                },
            ),
        ]));
        if push {
            parts.extend(self.push_to_talk(settings));
        } else if linked {
            parts.push(self.sensitivity(settings));
        }
        form::page(parts)
    }

    /// The camera's preview, 16:9 at the column's width: black in every
    /// theme, as a camera's picture.
    fn camera_preview<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        iced::widget::responsive(move |size| {
            let testing = settings.voice.camera.is_some();
            let frame =
                testing
                    .then(|| self.frames.get(CAMERA_TEST))
                    .flatten()
                    .map(|(handle, size)| {
                        Element::from(Video::new(handle.clone(), *size, Color::BLACK))
                    });
            let grey = crate::style::hex(0xb5b8c1);
            let body = frame.unwrap_or_else(|| {
                let note = if !settings.voice.camera_note.is_empty() {
                    settings.voice.camera_note.as_str()
                } else if testing {
                    "Starting your camera..."
                } else {
                    "Your camera is off."
                };
                column![
                    self.glyph(Icon::Video, 30, grey),
                    text(note).size(14).color(grey).center()
                ]
                .spacing(10)
                .padding(12)
                .align_x(Alignment::Center)
                .into()
            });
            container(body)
                .center_x(size.width)
                .height(size.width * 9.0 / 16.0)
                .align_y(Alignment::Center)
                .clip(true)
                .style(|_| crate::style::rounded(Color::BLACK, 8))
                .into()
        })
        .height(Length::Shrink)
        .into()
    }

    /// Discord's calls' own: noise suppression and its strength, the
    /// soundboard's volume, entrance sounds, the room's echo and Windows'
    /// effects.
    fn discord_voice<'a>(&'a self, settings: &'a Settings) -> Vec<Element<'a, Message>> {
        let theme = &self.theme;
        let voice = &self.layout.voice;
        let noise = SUPPRESSORS
            .iter()
            .position(|kind| *kind == voice.noise_suppression)
            .unwrap_or(0);
        let mut parts = vec![form::group([
            form::heading(theme, "NOISE SUPPRESSION IN DISCORD CALLS"),
            choices(
                theme,
                &["DeepFilterNet", "RNNoise", "Off"],
                noise,
                |index| voice_message(Event::Noise(index)),
            ),
            form::note(
                theme,
                "DeepFilterNet also takes out typing and other sudden noise. RNNoise uses less processing. Off sends your microphone as it is, for a quiet room. A call going on switches straight away.",
            ),
        ])];
        if voice.noise_suppression != Suppressor::Off {
            let strength = STRENGTHS
                .iter()
                .position(|kind| *kind == voice.noise_strength)
                .unwrap_or(1);
            parts.push(form::group([
                form::heading(theme, "NOISE SUPPRESSION STRENGTH"),
                choices(theme, &["Full", "Medium", "Light"], strength, |index| {
                    voice_message(Event::Strength(index))
                }),
                form::note(
                    theme,
                    "Full takes out the most noise, but can cut off giggles and breathy laughs. Medium and Light leave a little of the room's sound in, which can sound more natural, and let a little more noise through.",
                ),
            ]));
        }
        let state = self.client.state();
        let board = yapfast_core::volume::to_slider(state.soundboard_volume / 100.0)
            .round()
            .clamp(0.0, 100.0) as u32;
        parts.push(form::group([
            form::heading(theme, "SOUNDBOARD VOLUME"),
            slider("Soundboard volume", board, 100, |value| {
                voice_message(Event::SoundboardVolume(value, false))
            })
            .mark(false)
            .on_release(voice_message(Event::SoundboardVolume(board, true)))
            .view(theme),
            form::note(
                theme,
                "How loud soundboard sounds play in Discord calls, yours and everyone else's. Discord keeps it for all your devices.",
            ),
        ]));
        parts.push(self.entrance_sounds(settings));
        let dereverb = voice.reduce_room_echo;
        parts.push(
            switch(
                "Take out your room's echo in Discord calls",
                dereverb,
                Some(voice_message(Event::Dereverb(!dereverb))),
            )
            .description("For a bare or echoey room that makes your voice sound far away. Uses a little more processing, and your voice goes out 10 ms later.")
            .view(self),
        );
        if cfg!(windows) {
            let effects = voice.windows_microphone_effects;
            parts.push(
                switch(
                    "Let Windows process your microphone in Discord calls",
                    effects,
                    Some(voice_message(Event::WindowsEffects(!effects))),
                )
                .description("Runs the effects Windows and your sound driver add to the microphone, such as their own noise suppression. Off, Yapfast skips them, as they can muffle your voice on top of Yapfast's. Turn it on if your microphone sounds worse without them, as some laptop microphones can.")
                .view(self),
            );
        }
        parts
    }

    /// Entrance sounds: the server to choose one for, its sound, Choose
    /// (which needs Nitro) and Remove.
    fn entrance_sounds<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        let theme = &self.theme;
        let state = self.client.state();
        let servers = entrance_servers(self);
        let chosen = settings.voice.entrance_server.min(servers.len() - 1);
        let server = servers[chosen].clone();
        let names: Vec<String> = std::iter::once("All servers".to_owned())
            .chain(state.servers.iter().filter_map(|server| {
                yapfast_core::discord::parse_server(&server.id).map(|_| server.name.clone())
            }))
            .collect();
        let catalog = state.sound_catalog.as_ref();
        let sound = state
            .join_sounds
            .iter()
            .find(|join| join.server == server)
            .map(|join| {
                let named = catalog
                    .iter()
                    .flat_map(|groups| groups.iter())
                    .flat_map(|group| &group.sounds)
                    .find(|sound| sound.id == join.sound && sound.guild == join.guild);
                match (named, catalog) {
                    (Some(sound), _) => sound.name.clone(),
                    (None, None) => "Loading...".to_owned(),
                    (None, Some(_)) => "A sound that's gone".to_owned(),
                }
            });
        let nitro = self.client.app.discord_nitro();
        let sounds: Vec<(String, String)> = catalog
            .iter()
            .flat_map(|groups| groups.iter())
            .flat_map(|group| &group.sounds)
            .map(|sound| (sound.id.clone(), sound.name.clone()))
            .collect();
        let labels: Vec<String> = sounds.iter().map(|(_, name)| name.clone()).collect();
        let ids: Vec<String> = sounds.into_iter().map(|(id, _)| id).collect();
        let open = settings.open;
        let mut line = vec![
            container(
                form::dropdown(
                    names,
                    Some(chosen),
                    open == Some(Drop::Entrance),
                    Message::Settings(SettingsEvent::Drop(Drop::Entrance)),
                    |index| voice_message(Event::EntranceServer(index)),
                )
                .view(self),
            )
            .width(200)
            .into(),
            form::readout(theme, sound.clone().unwrap_or_default(), "None", false)
                .width(Length::Fill)
                .into(),
        ];
        let choose: Element<'a, Message> = if nitro && open == Some(Drop::Start) {
            form::dropdown(
                labels,
                None,
                true,
                Message::Settings(SettingsEvent::Drop(Drop::Start)),
                move |index| voice_message(Event::Entrance(ids.get(index).cloned())),
            )
            .width(200)
            .view(self)
        } else {
            action(
                theme,
                "Choose",
                Kind::Plain,
                nitro.then(|| Message::Settings(SettingsEvent::Drop(Drop::Start))),
            )
            .into()
        };
        line.push(choose);
        if sound.is_some() {
            line.push(
                action(
                    theme,
                    "Remove",
                    Kind::Ghost,
                    Some(voice_message(Event::Entrance(None))),
                )
                .into(),
            );
        }
        form::group([
            form::heading(theme, "ENTRANCE SOUNDS"),
            row(line).spacing(8).align_y(Alignment::Center).into(),
            form::note(
                theme,
                if nitro {
                    "Plays to everyone in a voice channel or DM call as you join it, outside stage channels. A server's own comes before the one for all servers."
                } else {
                    "Entrance sounds play to everyone as you join a voice channel. They need Nitro."
                },
            ),
        ])
    }

    /// Push to talk's key, with its recording, and its release delay.
    fn push_to_talk<'a>(&'a self, settings: &'a Settings) -> Vec<Element<'a, Message>> {
        let theme = &self.theme;
        let voice = &self.layout.voice;
        let recording = settings.voice.recording;
        let key = voice
            .push_to_talk_key
            .as_ref()
            .and_then(|key| key.get("name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let note = if recording {
            "Press the key to hold while you talk. Escape stops without changing it.".to_owned()
        } else if key.is_empty() {
            "Push to talk is on, but no keybind is set, so your microphone sends nothing."
                .to_owned()
        } else {
            self.talk
                .note
                .lock()
                .map(|note| note.clone())
                .unwrap_or_default()
        };
        let button = action(
            theme,
            if recording {
                "Stop recording"
            } else if key.is_empty() {
                "Record keybind"
            } else {
                "Edit keybind"
            },
            if recording { Kind::Danger } else { Kind::Plain },
            Some(voice_message(Event::Record(!recording))),
        );
        let delay = voice
            .push_to_talk_release_delay
            .unwrap_or(yapfast_core::push_to_talk::DEFAULT_RELEASE_DELAY)
            .min(yapfast_core::push_to_talk::MOST_RELEASE_DELAY);
        let shown = if delay >= 1000 {
            format!("{:.2} s", delay as f32 / 1000.0)
        } else {
            format!("{delay} ms")
        };
        vec![
            form::group([
                form::heading(theme, "PUSH TO TALK KEYBIND"),
                form::buttons([form::capture(theme, key, recording), button.into()]),
                form::note(theme, note),
            ]),
            form::group([
                form::heading(theme, "PUSH TO TALK RELEASE DELAY"),
                slider(
                    "Push to talk release delay",
                    delay,
                    yapfast_core::push_to_talk::MOST_RELEASE_DELAY,
                    |value| voice_message(Event::TalkDelay(value, false)),
                )
                .step(10)
                .mark(false)
                .shown(shown)
                .on_release(voice_message(Event::TalkDelay(delay, true)))
                .view(theme),
                form::note(
                    theme,
                    "How long your microphone keeps sending after you let go of the key, so the end of a word isn't cut off.",
                ),
            ]),
        ]
    }

    /// The Discord gate's sensitivity, over the microphone's level as the
    /// gate hears it, as the Slint client's `SensitivitySlider`.
    fn sensitivity<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        let theme = &self.theme;
        let voice = &self.layout.voice;
        let decibels = voice
            .input_sensitivity
            .unwrap_or(yapfast_core::noise::DEFAULT_SENSITIVITY as i32)
            .clamp(-80, 0);
        let level = settings.voice.level;
        let sending = settings.voice.sending;
        let at = |decibels: f32| ((decibels + 80.0) / 80.0).clamp(0.0, 1.0);
        let (live, loud, danger, field) = (theme.live, theme.loud, theme.danger, theme.field);
        let (orange, red, shown) = (at(-18.0), at(-6.0), at(level));
        let meter = iced::widget::responsive(move |size| {
            let width = size.width;
            let segment = |from: f32, to: f32, color: Color| -> Element<'_, Message> {
                container(bar(color, ((to - from).max(0.0)) * width, 8))
                    .padding(iced::Padding {
                        left: from * width,
                        ..iced::Padding::ZERO
                    })
                    .into()
            };
            container(
                iced::widget::stack![
                    bar(field, Length::Fill, 8),
                    segment(0.0, shown.min(orange), live),
                    segment(orange, shown.min(red), loud),
                    segment(red, shown, danger),
                ]
                .height(8),
            )
            .padding([5, 0])
            .clip(true)
            .into()
        })
        .height(18);
        let knob = theme.text;
        let track = iced::widget::slider(0..=80u32, (decibels + 80) as u32, |value| {
            voice_message(Event::Sensitivity(value, false))
        })
        .on_release(voice_message(Event::Sensitivity(
            (decibels + 80) as u32,
            true,
        )))
        .height(18)
        .style(move |_, _| iced::widget::slider::Style {
            rail: iced::widget::slider::Rail {
                backgrounds: (Color::TRANSPARENT.into(), Color::TRANSPARENT.into()),
                width: 8.0,
                border: iced::Border::default(),
            },
            handle: iced::widget::slider::Handle {
                shape: iced::widget::slider::HandleShape::Circle { radius: 7.0 },
                background: knob.into(),
                border_width: 0.0,
                border_color: Color::TRANSPARENT,
            },
        });
        let note = if !yapfast_core::discord_audio::AVAILABLE {
            "Discord voice and its level aren't available on this system yet."
        } else if settings
            .voice
            .gate
            .as_ref()
            .is_some_and(|gate| gate.failed())
        {
            "Yapfast can't read this microphone. Check that it's connected, or choose another."
        } else {
            "Yapfast sends your voice when it's louder than the handle, and other sounds when they're 18 dB louder still. The bar is your microphone after noise suppression and at its volume, as Yapfast hears it to decide."
        };
        form::group([
            form::heading(theme, "INPUT SENSITIVITY IN DISCORD CALLS"),
            column![
                row![
                    text("Input sensitivity")
                        .size(13)
                        .font(SEMIBOLD)
                        .color(theme.secondary)
                        .width(Length::Fill),
                    text(if sending { "Sending" } else { "Not sending" })
                        .size(13)
                        .font(SEMIBOLD)
                        .color(if sending { theme.live } else { theme.muted }),
                    text(format!("{decibels} dB"))
                        .size(13)
                        .font(SEMIBOLD)
                        .color(theme.text),
                ]
                .spacing(10),
                iced::widget::stack![meter, track]
            ]
            .spacing(6)
            .into(),
            form::note(theme, note),
        ])
    }
}

fn voice_message(event: Event) -> Message {
    Message::Settings(SettingsEvent::Voice(event))
}

/// The servers an entrance sound can be chosen for: every server (none),
/// then each Discord server by its own id.
fn entrance_servers(app: &App) -> Vec<Option<String>> {
    std::iter::once(None)
        .chain(app.client.state().servers.iter().filter_map(|server| {
            yapfast_core::discord::parse_server(&server.id).map(|guild| Some(guild.to_owned()))
        }))
        .collect()
}

/// Push to talk's key, as the root crate's `Keybind` keeps it in
/// `layout.json`, from a key pressed: its scan code (evdev's on Linux,
/// set 1's on Windows, as winit gives it), its name as Settings shows it,
/// and on Linux its keysym, which the desktop portal takes. None for
/// Escape, which stops recording, or a key with no scan code.
pub fn recorded(
    key: &Key,
    physical: key::Physical,
    location: Location,
) -> Option<serde_json::Value> {
    if *key == Key::Named(key::Named::Escape) {
        return None;
    }
    let scancode = scancode(physical)?;
    let mut keybind = serde_json::json!({
        "scancode": scancode,
        "name": key_name(key, physical, location),
    });
    if let Some(trigger) = trigger(key, location) {
        keybind["trigger"] = trigger.into();
    }
    Some(keybind)
}

/// The scan code winit gives the key: iced's codes are winit's by name.
fn scancode(physical: key::Physical) -> Option<u32> {
    use winit::platform::scancode::PhysicalKeyExtScancode;
    let key::Physical::Code(code) = physical else {
        return None;
    };
    let wanted = format!("{code:?}");
    let codes = if cfg!(windows) {
        (0..0x100).chain(0xE000..0xE100).collect::<Vec<u32>>()
    } else {
        (0..0x300).collect()
    };
    codes.into_iter().find(|&scancode| {
        matches!(
            winit::keyboard::PhysicalKey::from_scancode(scancode),
            winit::keyboard::PhysicalKey::Code(found) if format!("{found:?}") == wanted
        )
    })
}

/// A key's name as Settings shows it, as the root crate's push to talk
/// names it: "V", "Right Ctrl", "Numpad 1".
fn key_name(key: &Key, physical: key::Physical, location: Location) -> String {
    let side = match location {
        Location::Left => "Left ",
        Location::Right => "Right ",
        Location::Numpad => "Numpad ",
        Location::Standard => "",
    };
    match key {
        Key::Character(text) => format!("{side}{}", text.to_uppercase()),
        Key::Named(named) => {
            let named = match named {
                key::Named::Control => "Ctrl".to_owned(),
                key::Named::Super | key::Named::Meta => "Super".to_owned(),
                key::Named::ArrowUp => "Up".to_owned(),
                key::Named::ArrowDown => "Down".to_owned(),
                key::Named::ArrowLeft => "Left".to_owned(),
                key::Named::ArrowRight => "Right".to_owned(),
                key::Named::Space => "Space".to_owned(),
                named => format!("{named:?}"),
            };
            format!("{side}{named}")
        }
        Key::Unidentified => match physical {
            key::Physical::Code(code) => format!("{code:?}"),
            key::Physical::Unidentified(_) => "Unknown key".to_owned(),
        },
    }
}

/// The key's keysym name, as the XDG shortcuts specification writes a
/// trigger; on Linux, where only the portal needs it.
fn trigger(key: &Key, location: Location) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let right = location == Location::Right;
    let side =
        |left: &str, right_name: &str| Some(if right { right_name } else { left }.to_owned());
    match key {
        Key::Character(text) => {
            let mut chars = text.chars();
            let (Some(char), None) = (chars.next(), chars.next()) else {
                return None;
            };
            let name = xkeysym::Keysym::from_char(char).name()?;
            Some(name.strip_prefix("XK_").unwrap_or(name).to_owned())
        }
        Key::Named(named) => match named {
            key::Named::Control => side("Control_L", "Control_R"),
            key::Named::Shift => side("Shift_L", "Shift_R"),
            key::Named::Alt => side("Alt_L", "Alt_R"),
            key::Named::Super | key::Named::Meta => side("Super_L", "Super_R"),
            key::Named::AltGraph => Some("ISO_Level3_Shift".to_owned()),
            key::Named::CapsLock => Some("Caps_Lock".to_owned()),
            key::Named::ScrollLock => Some("Scroll_Lock".to_owned()),
            key::Named::NumLock => Some("Num_Lock".to_owned()),
            key::Named::PageUp => Some("Prior".to_owned()),
            key::Named::PageDown => Some("Next".to_owned()),
            key::Named::Space => Some("space".to_owned()),
            key::Named::Enter => Some("Return".to_owned()),
            key::Named::Backspace => Some("BackSpace".to_owned()),
            key::Named::ArrowUp => Some("Up".to_owned()),
            key::Named::ArrowDown => Some("Down".to_owned()),
            key::Named::ArrowLeft => Some("Left".to_owned()),
            key::Named::ArrowRight => Some("Right".to_owned()),
            key::Named::Tab
            | key::Named::Insert
            | key::Named::Delete
            | key::Named::Home
            | key::Named::End
            | key::Named::Pause => Some(format!("{named:?}")),
            key::Named::ContextMenu => Some("Menu".to_owned()),
            named => {
                let name = format!("{named:?}");
                (name.starts_with('F') && name[1..].parse::<u8>().is_ok()).then_some(name)
            }
        },
        Key::Unidentified => None,
    }
}

/// While the key is recorded, every key press goes to it, whatever has the
/// keys.
pub fn record_keys() -> iced::Subscription<Message> {
    iced::event::listen_with(|event, _, _| match event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            location,
            repeat: false,
            ..
        }) => Some(voice_message(Event::Recorded(recorded(
            &key,
            physical_key,
            location,
        )))),
        _ => None,
    })
}

/// Push to talk's watcher and what it said of where the key works.
#[derive(Default)]
pub struct Talk {
    pub watcher: Option<Box<dyn std::any::Any>>,
    pub note: Arc<Mutex<String>>,
}
