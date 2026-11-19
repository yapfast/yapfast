//! The Voice and video settings page: what it shows and what it asks for.
//! Task S03 (docs/iced/tasks/settings.md). The Slint side: `VoiceSettings`
//! (`ui/settings.slint:826`), set from `src/ui/callbacks.rs:2296`, `:3828`
//! and `:3861`.

use crate::kind::{Level, Live};

/// A sound the person can choose as an entrance sound.
#[derive(Clone, Debug, PartialEq)]
pub struct Entrance {
    pub id: String,
    pub name: String,
}

/// A heading in the list of entrance sounds and the sounds under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub heading: String,
    pub sounds: Vec<Entrance>,
}

/// The Voice and video settings page: what its screen draws. The window holds
/// it as `vm.settings.voice`. The devices are named by the system, so the
/// names are data; the chosen one is its place in the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Voice {
    pub microphones: Vec<String>,
    pub speakers: Vec<String>,
    pub cameras: Vec<String>,
    pub microphone: usize,
    pub speaker: usize,
    pub camera: usize,
    /// Filming to try the camera, what it films (no frame until one came;
    /// the preview reads its newest as it draws), and why it can't, if it
    /// couldn't start.
    pub camera_testing: bool,
    pub camera_preview: Live,
    pub camera_note: String,
    pub unmute_on_join: bool,
    /// How long a DM call with no one else in it waits: 0 never, then 3, 10
    /// and 30 minutes and an hour.
    pub alone_wait: usize,
    pub level_voice: bool,
    /// 0 DeepFilterNet, 1 RNNoise, 2 nothing; and how far: 0 full, 1 medium,
    /// 2 light.
    pub noise_suppression: usize,
    pub noise_strength: usize,
    pub dereverb: bool,
    /// Windows runs its microphone effects in Discord calls; the switch shows
    /// where there are any (`windows_effects_choice`).
    pub windows_effects: bool,
    pub windows_effects_choice: bool,
    /// The quietest speech Discord calls send, in dBFS; the microphone as the
    /// gate hears it, in dBFS (the app stores it while the page is in front,
    /// and the meters read it as they draw), and whether the gate is open.
    /// `gate_note` says why the level can't be shown.
    pub sensitivity: i32,
    pub gate: Level,
    pub gate_sending: bool,
    pub gate_note: String,
    /// Testing the microphone, why it can't play, and a call going on, which
    /// leaves the test off.
    pub microphone_testing: bool,
    pub microphone_test_note: String,
    pub calling: bool,
    /// 0 voice activity, 1 push to talk.
    pub input_mode: usize,
    /// The push-to-talk keybind's name, empty while there's none; whether the
    /// keys pressed next become it, and those pressed so far.
    pub talk_key: String,
    pub talk_recording: bool,
    pub recorded_keys: String,
    /// How long the microphone keeps sending after the key comes up, in ms.
    pub talk_delay: i32,
    /// Where the key works.
    pub talk_note: String,
    /// Discord is linked, so its calls' settings show.
    pub discord: bool,
    /// Percentages, 0 to 200; the soundboard's 0 to 100.
    pub microphone_volume: i32,
    pub speaker_volume: i32,
    pub soundboard_volume: i32,
    /// Entrance sounds: the servers to choose one for ("All servers" first),
    /// the one chosen, its sound's name or empty, the sounds to choose from
    /// (they need Nitro), and whether that list is open.
    pub entrance_servers: Vec<String>,
    pub entrance_server: usize,
    pub entrance_sound: String,
    pub entrance_groups: Vec<Group>,
    pub entrance_open: bool,
    pub nitro: bool,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            microphones: Vec::new(),
            speakers: Vec::new(),
            cameras: Vec::new(),
            microphone: 0,
            speaker: 0,
            camera: 0,
            camera_testing: false,
            camera_preview: Live::default(),
            camera_note: String::new(),
            unmute_on_join: true,
            alone_wait: 1,
            level_voice: false,
            noise_suppression: 0,
            noise_strength: 0,
            dereverb: false,
            windows_effects: false,
            windows_effects_choice: false,
            sensitivity: -60,
            gate: Level::default(),
            gate_sending: false,
            gate_note: String::new(),
            microphone_testing: false,
            microphone_test_note: String::new(),
            calling: false,
            input_mode: 0,
            talk_key: String::new(),
            talk_recording: false,
            recorded_keys: String::new(),
            talk_delay: 20,
            talk_note: String::new(),
            discord: false,
            microphone_volume: 100,
            speaker_volume: 100,
            soundboard_volume: 100,
            entrance_servers: Vec::new(),
            entrance_server: 0,
            entrance_sound: String::new(),
            entrance_groups: Vec::new(),
            entrance_open: false,
            nitro: false,
        }
    }
}

/// The Voice and video settings page: what a click or a key on it asks for.
/// Volumes come as a pair, the move under a drag (the app plays each step of
/// the call volumes) and the settle, which saves. Keep `#[non_exhaustive]`:
/// the app matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    Microphone(usize),
    Speaker(usize),
    Camera(usize),
    /// Start or stop filming the camera to try it.
    TestCamera(bool),
    /// Start or stop playing the microphone back.
    TestMicrophone(bool),
    /// Look for devices again.
    Refresh,
    UnmuteOnJoin(bool),
    LevelVoice(bool),
    AloneWait(usize),
    NoiseSuppression(usize),
    NoiseStrength(usize),
    Dereverb(bool),
    /// An input profile, by its place in [`Profile::ALL`]: a preset sets
    /// its switches, and Custom changes nothing.
    Profile(usize),
    WindowsEffects(bool),
    InputMode(usize),
    /// Start or stop recording the push-to-talk keybind.
    RecordTalkKey(bool),
    TalkDelay(i32),
    TalkDelaySettled(i32),
    Sensitivity(i32),
    SensitivitySettled(i32),
    MicrophoneVolume(i32),
    MicrophoneVolumeSettled(i32),
    SpeakerVolume(i32),
    SpeakerVolumeSettled(i32),
    SoundboardVolume(i32),
    SoundboardVolumeSettled(i32),
    /// The server to choose an entrance sound for; the app loads its sound.
    EntranceServer(usize),
    /// The list of entrance sounds opens or closes; the app loads the names
    /// when it opens.
    EntranceList(bool),
    /// An entrance sound was chosen by its id, or taken away with "".
    Entrance(String),
}

/// The input profiles, as Discord offers them in the microphone's menu and
/// in Voice & video. Only the switches are kept: the profile is what they
/// make, so choosing one sets them as the page's own intents would.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    /// All that cleans the voice on: leveling, DeepFilterNet at full
    /// strength, the room's echo taken out.
    Isolation,
    /// None of it: the microphone as it is.
    Studio,
    /// Whatever the switches say otherwise.
    #[default]
    Custom,
}

impl Profile {
    pub const ALL: [Self; 3] = [Self::Isolation, Self::Studio, Self::Custom];

    /// The profile the switches make: leveling, the noise suppression and
    /// its strength by their places on the page (0 DeepFilterNet, 2 none;
    /// 0 full), and the room's echo.
    pub fn of(level: bool, noise: usize, strength: usize, dereverb: bool) -> Self {
        match (level, noise, strength, dereverb) {
            (true, 0, 0, true) => Self::Isolation,
            (false, 2, _, false) => Self::Studio,
            _ => Self::Custom,
        }
    }

    /// The page's intents that set a preset's switches; none for Custom.
    pub fn switches(self) -> Vec<Intent> {
        match self {
            Self::Isolation => vec![
                Intent::LevelVoice(true),
                Intent::NoiseSuppression(0),
                Intent::NoiseStrength(0),
                Intent::Dereverb(true),
            ],
            Self::Studio => vec![
                Intent::LevelVoice(false),
                Intent::NoiseSuppression(2),
                Intent::Dereverb(false),
            ],
            Self::Custom => Vec::new(),
        }
    }

    /// Its place in [`Profile::ALL`].
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&one| one == self).unwrap_or(2)
    }
}

impl Voice {
    /// The input profile the switches make.
    pub fn profile(&self) -> Profile {
        Profile::of(
            self.level_voice,
            self.noise_suppression,
            self.noise_strength,
            self.dereverb,
        )
    }

    /// What an intent of the page changes in it, for the app's update and for
    /// the simulator's tests.
    pub fn apply(&mut self, intent: &Intent) {
        let last = |len: usize| len.saturating_sub(1);
        match intent {
            Intent::Microphone(at) => self.microphone = (*at).min(last(self.microphones.len())),
            Intent::Speaker(at) => self.speaker = (*at).min(last(self.speakers.len())),
            Intent::Camera(at) => self.camera = (*at).min(last(self.cameras.len())),
            Intent::TestCamera(on) => self.camera_testing = *on,
            Intent::TestMicrophone(on) => self.microphone_testing = *on && !self.calling,
            Intent::UnmuteOnJoin(on) => self.unmute_on_join = *on,
            Intent::LevelVoice(on) => self.level_voice = *on,
            Intent::AloneWait(at) => self.alone_wait = (*at).min(4),
            Intent::NoiseSuppression(at) => self.noise_suppression = (*at).min(2),
            Intent::NoiseStrength(at) => self.noise_strength = (*at).min(2),
            Intent::Dereverb(on) => self.dereverb = *on,
            Intent::Profile(at) => {
                let profile = Profile::ALL[(*at).min(Profile::ALL.len() - 1)];
                for switch in profile.switches() {
                    self.apply(&switch);
                }
            }
            Intent::WindowsEffects(on) => self.windows_effects = *on,
            Intent::InputMode(at) => self.input_mode = (*at).min(1),
            Intent::RecordTalkKey(on) => self.talk_recording = *on,
            Intent::TalkDelay(ms) | Intent::TalkDelaySettled(ms) => {
                self.talk_delay = (*ms).clamp(0, 2000);
            }
            Intent::Sensitivity(db) | Intent::SensitivitySettled(db) => {
                self.sensitivity = (*db).clamp(-100, 0);
            }
            Intent::MicrophoneVolume(v) | Intent::MicrophoneVolumeSettled(v) => {
                self.microphone_volume = (*v).clamp(0, 200);
            }
            Intent::SpeakerVolume(v) | Intent::SpeakerVolumeSettled(v) => {
                self.speaker_volume = (*v).clamp(0, 200);
            }
            Intent::SoundboardVolume(v) | Intent::SoundboardVolumeSettled(v) => {
                self.soundboard_volume = (*v).clamp(0, 100);
            }
            Intent::EntranceServer(at) => {
                self.entrance_server = (*at).min(last(self.entrance_servers.len()));
            }
            Intent::EntranceList(open) => self.entrance_open = *open && self.nitro,
            Intent::Entrance(_) => self.entrance_open = false,
            Intent::Refresh => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_sets_its_switches_and_the_switches_make_the_profile() {
        let mut page = Voice::default();
        assert_eq!(page.profile(), Profile::Custom, "DeepFilterNet alone");
        page.apply(&Intent::Profile(0));
        assert_eq!(page.profile(), Profile::Isolation);
        assert!(page.level_voice && page.dereverb);
        assert_eq!((page.noise_suppression, page.noise_strength), (0, 0));
        page.apply(&Intent::Profile(1));
        assert_eq!(page.profile(), Profile::Studio);
        assert_eq!(page.noise_suppression, 2);
        // Custom keeps the switches; changing one leaves a preset.
        page.apply(&Intent::Profile(2));
        assert_eq!(page.profile(), Profile::Studio);
        page.apply(&Intent::Dereverb(true));
        assert_eq!(page.profile(), Profile::Custom);
        page.apply(&Intent::Profile(0));
        page.apply(&Intent::NoiseStrength(1));
        assert_eq!(page.profile(), Profile::Custom, "not at full strength");
    }

    #[test]
    fn values_stay_in_their_ranges() {
        let mut page = Voice::default();
        page.apply(&Intent::MicrophoneVolumeSettled(500));
        page.apply(&Intent::SoundboardVolume(500));
        page.apply(&Intent::Camera(9));
        assert_eq!(
            (page.microphone_volume, page.soundboard_volume, page.camera),
            (200, 100, 0)
        );
    }

    #[test]
    fn a_call_going_on_leaves_the_microphone_test_off() {
        let mut page = Voice {
            calling: true,
            ..Voice::default()
        };
        page.apply(&Intent::TestMicrophone(true));
        assert!(!page.microphone_testing);
    }
}
