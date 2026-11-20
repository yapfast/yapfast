//! Voice & video: the microphone, speakers and camera with their volumes
//! and a camera's preview, how calls start and sound, noise suppression,
//! the soundboard's volume and entrance sounds, and how the microphone is
//! sent (voice activity with the gate's sensitivity, or push to talk with
//! its key and release delay).
//!
//! While it shows it may run three things, each gone when it's left: the
//! camera being tried, the Discord gate's meter (while the gate's slider
//! shows, or the mic test plays the microphone back through it) and the
//! push-to-talk key's recording.

use iced::{
    Alignment, Color, Element, Length,
    widget::{column, container, row, text},
};
use yapfast_core::{
    application::{Action, AppState},
    keybinds::Shortcut,
    noise::{Strength, Suppressor},
    picture::Feed,
    voice::Command,
};

use super::{Drop, Event as SettingsEvent, Page, Settings};
use crate::{
    app::{App, Message},
    call::device_rows,
    controls::{Kind, action, choices, slider},
    ctx::{Cx, Pref as Shown, Request},
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
/// How often the gate's meter is drawn anew while it shows.
const METER_EVERY: std::time::Duration = std::time::Duration::from_millis(50);

const SUPPRESSORS: [Suppressor; 3] = [
    Suppressor::DeepFilterNet,
    Suppressor::RNNoise,
    Suppressor::Off,
];
const STRENGTHS: [Strength; 3] = [Strength::Full, Strength::Medium, Strength::Light];

/// What Voice runs while it shows.
#[derive(Default)]
pub struct Voice {
    /// The camera being tried, and why it couldn't start.
    pub camera: Option<(yapfast_core::camera::Camera, Feed)>,
    pub camera_note: String,
    /// The Discord gate's meter while its slider shows or the mic test
    /// runs, read as it's drawn.
    pub gate: Option<yapfast_core::discord_audio::GateMeter>,
    /// The mic test: the microphone, as a call's processing leaves it,
    /// played back through the speakers.
    pub testing: bool,
    /// The test deafened the call, to undeafen as the test ends.
    pub test_deafened: bool,
    /// The call as the test last saw it: going on, and deafened.
    pub test_saw: (bool, bool),
    /// How many times the camera being tried started or stopped.
    pub tries: u32,
    /// The soundboard's volume while its slider is dragged.
    pub soundboard: Option<u32>,
    /// Entrance sounds: the server chosen ("All servers" first), and the
    /// sounds to choose from while that list is open.
    pub entrance_server: usize,
}

#[derive(Clone, Debug)]
pub enum Event {
    Device(Drop, usize),
    Refresh,
    TestCamera,
    MicTest,
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
    SoundboardVolume(u32, bool),
    EntranceServer(usize),
    Entrance(Option<String>),
}

impl Settings {
    /// Voice shows: the devices are listed again, the entrance sounds
    /// asked for, and the gate's meter starts if its slider shows.
    pub(super) fn enter_voice(&mut self, cx: &mut Cx<'_>) {
        cx.dispatch(Action::Voice(Command::Devices));
        if super::discord_linked(cx.state()) {
            cx.dispatch(Action::LoadSoundCatalog);
        }
        self.meter_gate(cx);
    }

    /// The gate's meter runs while its slider shows (on Voice, with
    /// Discord linked, in voice activity) or the mic test runs.
    pub(super) fn meter_gate(&mut self, cx: &Cx<'_>) {
        let slider = super::discord_linked(cx.state())
            && cx.layout.voice.input_mode == InputMode::VoiceActivity;
        let wanted = (slider || self.voice.testing) && self.section() == super::Section::Voice;
        match (wanted, self.voice.gate.is_some()) {
            (true, false) => {
                let voice = &cx.state().voice;
                self.voice.gate = Some(yapfast_core::discord_audio::GateMeter::start(
                    voice.microphone.as_deref(),
                    voice.speaker.as_deref(),
                    self.voice.testing,
                ));
            }
            (false, true) => self.voice.gate = None,
            _ => {}
        }
    }

    /// Starts the mic test, as Discord's does: a call going on is
    /// deafened (and so muted) while it runs, unless it already is.
    fn start_mic_test(&mut self, cx: &mut Cx<'_>) {
        let voice = &cx.state().voice;
        self.voice.test_saw = (voice.active(), voice.deafened);
        if voice.active() && !voice.deafened {
            self.voice.test_deafened = true;
            cx.dispatch(Action::Voice(Command::Deafen(true)));
        }
        self.voice.testing = true;
        // Started again, to play back.
        self.voice.gate = None;
        self.meter_gate(cx);
    }

    /// Ends the mic test, if it runs: the call it deafened hears again,
    /// muted as it was, as undeafening leaves it. The meter goes;
    /// [`Self::meter_gate`] starts it again where its slider still shows.
    pub(super) fn end_mic_test(&mut self, cx: &mut Cx<'_>) {
        if !std::mem::take(&mut self.voice.testing) {
            return;
        }
        self.voice.gate = None;
        // Whether or not the backend has said the call is deafened yet.
        if std::mem::take(&mut self.voice.test_deafened) && cx.state().voice.active() {
            cx.dispatch(Action::Voice(Command::Deafen(false)));
        }
    }

    /// The call changed: undeafened, or joined undeafened, while the mic
    /// test runs, it ends, as Discord's does, rather than play the
    /// microphone back into a call that hears. Only a change ends it: the
    /// deafening the test asked for comes back from the backend later.
    pub fn voice_sync(&mut self, cx: &mut Cx<'_>) {
        if !self.voice.testing {
            return;
        }
        let voice = &cx.state().voice;
        let now = (voice.active(), voice.deafened);
        let before = std::mem::replace(&mut self.voice.test_saw, now);
        let hears = now == (true, false);
        if hears && (before == (true, true) || !before.0) {
            // Undeafened by hand: nothing to put back.
            self.voice.test_deafened = false;
            self.end_mic_test(cx);
            self.meter_gate(cx);
            cx.touch();
        }
    }

    /// The camera being tried, for the window's video sources, and how
    /// many times it started or stopped, so the root places the videos
    /// again as it does.
    pub fn camera_test(&self) -> (Option<&Feed>, u32) {
        let feed = self.voice.camera.as_ref().map(|(_, feed)| feed);
        (feed, self.voice.tries)
    }

    pub(super) fn voice_event(&mut self, event: Event, cx: &mut Cx<'_>) {
        let root = cx.client.root().to_owned();
        let voice = &mut cx.layout.voice;
        match event {
            Event::Device(drop, index) => {
                self.open = None;
                let state = &cx.state().voice;
                let devices = match drop {
                    Drop::Microphone => &state.microphones,
                    Drop::Speaker => &state.speakers,
                    _ => &state.cameras,
                };
                let Some(id) = devices.get(index).map(|device| device.id.clone()) else {
                    return;
                };
                let command = match drop {
                    Drop::Microphone => Command::Microphone(id),
                    Drop::Speaker => Command::Speaker(id),
                    _ => Command::Camera(id),
                };
                cx.dispatch(Action::Voice(command));
                // The meter follows the microphone chosen, and the mic
                // test the speakers too; the camera being tried the
                // camera.
                let restart = match drop {
                    Drop::Microphone => self.voice.gate.is_some(),
                    Drop::Speaker => self.voice.testing,
                    _ => false,
                };
                if restart {
                    self.voice.gate = None;
                    self.meter_gate(cx);
                }
                if drop == Drop::Camera && self.voice.camera.is_some() {
                    self.voice.camera = None;
                    self.test_camera(cx);
                }
            }
            Event::Refresh => cx.dispatch(Action::Voice(Command::Devices)),
            Event::MicTest => {
                if self.voice.testing {
                    self.end_mic_test(cx);
                    self.meter_gate(cx);
                } else {
                    self.start_mic_test(cx);
                }
            }
            Event::TestCamera => {
                let testing = self.voice.camera.is_some();
                self.voice.camera = None;
                self.voice.camera_note.clear();
                self.voice.tries += 1;
                if !testing {
                    self.test_camera(cx);
                }
                return;
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
                return;
            }
            Event::UnmuteOnJoin(on) => {
                cx.layout.unmute_on_join = Some(on);
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
            Event::Noise(index) => {
                let suppressor = SUPPRESSORS[index.min(2)];
                voice.noise_suppression = suppressor;
                yapfast_core::noise::set(suppressor);
                crate::layout::store(&root, "noise_suppression", suppressor);
                cx.request(Request::Changed(Shown::Noise));
            }
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
                cx.request(Request::Changed(Shown::Keybinds));
            }
            Event::Sensitivity(value, done) => {
                let decibels = value as i32 - 80;
                voice.input_sensitivity = Some(decibels);
                yapfast_core::noise::set_sensitivity(decibels as f32);
                if done {
                    crate::layout::store(&root, "input_sensitivity", decibels);
                }
                return;
            }
            Event::TalkDelay(value, done) => {
                let delay = value.min(yapfast_core::push_to_talk::MOST_RELEASE_DELAY);
                voice.push_to_talk_release_delay = Some(delay);
                yapfast_core::push_to_talk::set_release_delay(delay);
                if done {
                    crate::layout::store(&root, "push_to_talk_release_delay", delay);
                }
                return;
            }
            Event::SoundboardVolume(value, done) => {
                if !done {
                    self.voice.soundboard = Some(value.min(100));
                    return;
                }
                // Where the drag or the keys left it; the core puts it on
                // the slider's curve.
                let value = self.voice.soundboard.take().unwrap_or(value).min(100);
                cx.dispatch(Action::SoundboardVolume(value as f32));
            }
            Event::EntranceServer(index) => {
                self.voice.entrance_server = index;
                self.open = None;
                return;
            }
            Event::Entrance(sound) => {
                let server = entrance_servers(cx.state())
                    .get(self.voice.entrance_server)
                    .cloned()
                    .flatten();
                self.open = None;
                cx.dispatch(Action::SetJoinSound { server, sound });
            }
        }
        cx.touch();
    }

    /// Starts trying the camera chosen, or says why it can't.
    fn test_camera(&mut self, cx: &Cx<'_>) {
        let camera = cx.state().voice.camera.as_deref();
        // In the window's pixels.
        let scale = cx.scale;
        let feed = Feed::new();
        feed.show_at(Some((
            (PREVIEW.0 as f32 * scale).round() as u32,
            (PREVIEW.1 as f32 * scale).round() as u32,
        )));
        self.voice.tries += 1;
        match yapfast_core::camera::Camera::open(camera, Box::new(feed.sink())) {
            Ok(camera) => self.voice.camera = Some((camera, feed)),
            Err(error) => self.voice.camera_note = error.to_string(),
        }
    }
}

impl<'a> Page<'a> {
    pub(super) fn voice_page(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let state = &self.cx.state.voice;
        let voice = &self.cx.layout.voice;
        let open = settings.open;
        let linked = self.linked();
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
            .view(self.cx.look)
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
        if yapfast_core::discord_audio::AVAILABLE {
            parts.push(self.mic_test());
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
                self.camera_preview(),
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
        .icon(
            self.cx
                .look
                .icon(Icon::Refresh, 16, crate::theme::rgb(theme.secondary)),
        )
        .into()]));
        parts.push(form::rule(theme));
        let unmute = self.cx.layout.unmute_on_join.unwrap_or(true);
        parts.push(
            switch(
                "Turn on your microphone when you join a call",
                unmute,
                Some(voice_message(Event::UnmuteOnJoin(!unmute))),
            )
            .description("Otherwise you join muted, and unmute when you're ready.")
            .view(self.cx.look),
        );
        let level = voice.level_voice.unwrap_or(true);
        parts.push(
            switch(
                "Even out your voice",
                level,
                Some(voice_message(Event::LevelVoice(!level))),
            )
            .description("Keeps your voice steady as you lean towards the microphone or away from it. Whispers and shouts still come through as they are.")
            .view(self.cx.look),
        );
        if linked {
            parts.extend(self.discord_voice());
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
            parts.extend(self.push_to_talk());
        } else if linked {
            parts.push(self.sensitivity());
        }
        form::page(parts)
    }

    /// The camera's preview, 16:9 at the column's width: black in every
    /// theme, as a camera's picture.
    fn camera_preview(self) -> Element<'a, Message> {
        let settings = self.s;
        iced::widget::responsive(move |size| {
            let testing = settings.voice.camera.is_some();
            let frame = (testing && self.cx.video.has(CAMERA_TEST)).then(|| {
                Element::from(
                    Video::new(self.cx.video, CAMERA_TEST, Color::BLACK).scale(self.cx.scale),
                )
            });
            let grey = crate::tile::NOTE;
            let body = frame.unwrap_or_else(|| {
                let note = if !settings.voice.camera_note.is_empty() {
                    settings.voice.camera_note.as_str()
                } else if testing {
                    "Starting your camera..."
                } else {
                    "Your camera is off."
                };
                column![
                    self.cx.look.glyph(Icon::Video, 30, grey),
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
    fn discord_voice(self) -> Vec<Element<'a, Message>> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let voice = &self.cx.layout.voice;
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
        let state = self.cx.state;
        let board = settings.voice.soundboard.unwrap_or_else(|| {
            yapfast_core::volume::to_slider(state.soundboard_volume / 100.0)
                .round()
                .clamp(0.0, 100.0) as u32
        });
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
        parts.push(self.entrance_sounds());
        let dereverb = voice.reduce_room_echo;
        parts.push(
            switch(
                "Take out your room's echo in Discord calls",
                dereverb,
                Some(voice_message(Event::Dereverb(!dereverb))),
            )
            .description("For a bare or echoey room that makes your voice sound far away. Uses a little more processing, and your voice goes out 10 ms later.")
            .view(self.cx.look),
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
                .view(self.cx.look),
            );
        }
        parts
    }

    /// Entrance sounds: the server to choose one for, its sound, Choose
    /// (which needs Nitro) and Remove.
    fn entrance_sounds(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let state = self.cx.state;
        let servers = entrance_servers(self.cx.state);
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
        let nitro = self.cx.app.discord_nitro();
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
                .view(self.cx.look),
            )
            .width(200)
            .into(),
            form::readout(theme, sound.clone().unwrap_or_default(), "None", false)
                .width(Length::Fill)
                .into(),
        ];
        // Choose opens the soundboard under it, a click on a sound
        // choosing it.
        let board = (nitro && open == Some(Drop::Start)).then(|| {
            let groups = catalog.map_or(&[][..], |groups| &groups[..]);
            let board = crate::call::popups::soundboard::sound_board(
                self.cx,
                "Choose an entrance sound",
                groups,
                None,
                false,
                |sound| voice_message(Event::Entrance(Some(sound.id.clone()))),
            );
            container(board)
                .style(crate::controls::menu_panel(theme))
                .into()
        });
        let choose: Element<'a, Message> = crate::popover::Popover::new(
            action(
                theme,
                "Choose",
                Kind::Plain,
                nitro.then(|| Message::Settings(SettingsEvent::Drop(Drop::Start))),
            ),
            board,
            crate::popover::Side::Below,
        )
        .width(crate::popover::Width::Own(
            crate::call::popups::SOUNDBOARD_WIDTH,
        ))
        .right()
        .gap(6.0)
        .outside(Message::Settings(SettingsEvent::Drop(Drop::Start)))
        .into();
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
    fn push_to_talk(self) -> Vec<Element<'a, Message>> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let voice = &self.cx.layout.voice;
        let recorded = settings.recorded(super::keybinds::Target::Talk);
        let recording = recorded.is_some();
        let key = voice
            .push_to_talk_key
            .as_ref()
            .map(Shortcut::name)
            .unwrap_or_default();
        let note = if recording {
            "Press the keys to hold while you talk, together. Escape stops without changing it."
                .to_owned()
        } else if key.is_empty() {
            "Push to talk is on, but no keybind is set, so your microphone sends nothing."
                .to_owned()
        } else {
            self.cx.keybinds.talk_note()
        };
        let empty = key.is_empty();
        let button =
            super::keybinds::record_button(theme, super::keybinds::Target::Talk, recording, empty);
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
                form::buttons([form::capture(theme, key, recorded, 200.0), button]),
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
    /// gate hears it.
    fn sensitivity(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let voice = &self.cx.layout.voice;
        let decibels = voice
            .input_sensitivity
            .unwrap_or(yapfast_core::noise::DEFAULT_SENSITIVITY as i32)
            .clamp(-80, 0);
        // The gate's level and whether it sends, read as they're drawn,
        // twenty times a second: no message, no view.
        let gate = settings.voice.gate.as_ref();
        let inks = GateInks {
            live: theme.live,
            loud: theme.loud,
            danger: theme.danger,
            field: theme.field,
        };
        let meter = crate::widgets::gauge(METER_EVERY, move |renderer, bounds| {
            let level = gate.map_or(-100.0, |gate| gate.level().max(-100.0));
            gate_level(renderer, bounds, level, inks);
        })
        .height(18);
        let sending = move || gate.is_some_and(yapfast_core::discord_audio::GateMeter::sending);
        let (live, muted) = (theme.live, theme.muted);
        let sending_line = crate::widgets::live_text(METER_EVERY, move || {
            if sending() { "Sending" } else { "Not sending" }.to_owned()
        })
        .size(13)
        .font(SEMIBOLD)
        .ink(move || if sending() { live } else { muted });
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
                    sending_line,
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

impl<'a> Page<'a> {
    /// Discord's mic test: its button, a meter of notches lit to the
    /// microphone's level while it plays back, and what it hears, read as
    /// it's drawn.
    fn mic_test(self) -> Element<'a, Message> {
        let settings = self.s;
        let theme = self.cx.look.theme;
        let testing = settings.voice.testing;
        let gate = settings.voice.gate.as_ref().filter(|_| testing);
        let (lit, unlit) = (theme.live, theme.button_default);
        let meter = crate::widgets::gauge(METER_EVERY, move |renderer, bounds| {
            let level = gate.map_or(-100.0, |gate| gate.level().max(-100.0));
            notches(renderer, bounds, level, lit, unlit);
        })
        .height(24);
        // Discord's captions, the failures first.
        let said = move || -> (&'static str, &'static str) {
            match gate {
                None => ("", ""),
                Some(gate) if gate.failed() => (
                    "Yapfast can't read this microphone.",
                    "Check that it's connected, or choose another.",
                ),
                Some(gate) if gate.playback_failed() => (
                    "Yapfast can't play your voice back.",
                    "Check your speakers, or choose others.",
                ),
                Some(gate) if gate.no_input() => (
                    "Yapfast isn't detecting any input from your mic.",
                    "Please check that you've selected the right input device.",
                ),
                Some(_) => ("Playing back your beautiful voice", ""),
            }
        };
        // Trouble reads in the text's colour, the playing back quieter.
        let trouble = move || !said().1.is_empty();
        let (text_ink, quiet) = (theme.text, theme.muted);
        let caption = crate::widgets::live_text(METER_EVERY, move || said().0.to_owned())
            .size(12)
            .font(SEMIBOLD)
            .ink(move || if trouble() { text_ink } else { quiet });
        let advice = crate::widgets::live_text(METER_EVERY, move || said().1.to_owned())
            .size(12)
            .color(quiet);
        let button = action(
            theme,
            if testing {
                "Stop testing"
            } else {
                "Let's check"
            },
            Kind::Primary,
            Some(voice_message(Event::MicTest)),
        )
        .small();
        let mut group = vec![
            form::heading(theme, "MIC TEST"),
            form::note(
                theme,
                "Having mic issues? Start a test and say something fun, and Yapfast plays your voice back to you, with your noise suppression and volumes. Wear headphones, or it echoes.",
            ),
            row![
                button,
                column![meter, caption, advice]
                    .spacing(4)
                    .width(Length::Fill)
            ]
            .spacing(16)
            .width(Length::Fill)
            .align_y(Alignment::Start)
            .into(),
        ];
        if !testing && self.cx.state.voice.active() {
            group.push(form::note(
                theme,
                "You'll be muted and deafened in your call while you test.",
            ));
        }
        form::group(group)
    }
}

/// The mic test's meter: notches across `bounds`, lit up to the
/// microphone's `level` (dB, -100 to 0), as Discord's.
fn notches(
    renderer: &mut crate::widgets::Renderer,
    bounds: iced::Rectangle,
    level: f32,
    lit: Color,
    unlit: Color,
) {
    use iced::advanced::{Renderer as _, renderer::Quad};
    const WIDTH: f32 = 4.0;
    const GAP: f32 = 4.0;
    let count = ((bounds.width + GAP) / (WIDTH + GAP)).floor().max(1.0) as usize;
    let shown = ((level + 100.0) / 100.0).clamp(0.0, 1.0);
    for index in 0..count {
        let on = (index as f32 + 0.5) / count as f32 <= shown;
        renderer.fill_quad(
            Quad {
                bounds: iced::Rectangle {
                    x: bounds.x + index as f32 * (WIDTH + GAP),
                    width: WIDTH,
                    ..bounds
                },
                border: iced::border::rounded(2),
                ..Default::default()
            },
            if on { lit } else { unlit },
        );
    }
}

fn voice_message(event: Event) -> Message {
    Message::Settings(SettingsEvent::Voice(event))
}

/// The gate meter's colours: green up to -18 dB, orange to -6 dB, red
/// above, on the field's ground.
#[derive(Clone, Copy)]
struct GateInks {
    live: Color,
    loud: Color,
    danger: Color,
    field: Color,
}

/// The gate's `level` (dB, -80 to 0): an 8 px bar across the middle of
/// `bounds`.
fn gate_level(
    renderer: &mut crate::widgets::Renderer,
    bounds: iced::Rectangle,
    level: f32,
    inks: GateInks,
) {
    use iced::advanced::{Renderer as _, renderer::Quad};
    let at = |decibels: f32| ((decibels + 80.0) / 80.0).clamp(0.0, 1.0);
    let (orange, red, shown) = (at(-18.0), at(-6.0), at(level));
    let bar = iced::Rectangle {
        y: bounds.y + (bounds.height - 8.0) / 2.0,
        height: 8.0,
        ..bounds
    };
    let mut fill = |from: f32, to: f32, color: Color| {
        if to > from {
            renderer.fill_quad(
                Quad {
                    bounds: iced::Rectangle {
                        x: bar.x + from * bar.width,
                        width: (to - from) * bar.width,
                        ..bar
                    },
                    ..Default::default()
                },
                color,
            );
        }
    };
    fill(0.0, 1.0, inks.field);
    fill(0.0, shown.min(orange), inks.live);
    fill(orange, shown.min(red), inks.loud);
    fill(red, shown, inks.danger);
}

/// The servers an entrance sound can be chosen for: every server (none),
/// then each Discord server by its own id.
fn entrance_servers(state: &AppState) -> Vec<Option<String>> {
    std::iter::once(None)
        .chain(state.servers.iter().filter_map(|server| {
            yapfast_core::discord::parse_server(&server.id).map(|guild| Some(guild.to_owned()))
        }))
        .collect()
}

/// The entrance sounds' pictures, while Choose's soundboard is open.
pub fn entrance_pictures(app: &App) -> Vec<String> {
    let open = app.win.settings.get().is_some_and(|settings| {
        settings.open == Some(Drop::Start) && settings.section() == crate::settings::Section::Voice
    });
    if !open {
        return Vec::new();
    }
    app.client
        .state()
        .sound_catalog
        .iter()
        .flat_map(|groups| groups.iter())
        .flat_map(|group| &group.sounds)
        .filter_map(|sound| sound.picture.clone())
        .collect()
}
