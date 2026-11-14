//! What the window keeps in the data folder's `layout.json`, the file the
//! Slint client keeps the same things in, so both clients share them: the
//! theme and the person's own themes (`theme.rs`), the rail's open folders,
//! each server's collapsed channel categories, whether the member list
//! is hidden, whether long messages fold, pinned DMs, and the people
//! settings the core is told of as the window opens ([`Layout::actions`]). Each change writes its one key back, leaving the rest of the
//! file as it was.

use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
};

use yapfast_core::theme::Custom;

/// The file, in the data folder.
pub const LAYOUT: &str = "layout.json";

/// What this client reads of `layout.json`.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
pub struct Layout {
    pub theme: Option<String>,
    pub custom_themes: Vec<Custom>,
    /// The rail's open folders, by folder id.
    pub open_folders: BTreeSet<String>,
    /// Each server's collapsed categories, by server id: a Discord category
    /// by its room id, any other by its heading.
    pub collapsed: HashMap<String, BTreeSet<String>>,
    pub members_hidden: bool,
    /// Copy ID shows in menus, as Discord's Developer Mode.
    pub developer_mode: bool,
    /// How calls are joined: unmuted unless turned off, and deafened.
    pub unmute_on_join: Option<bool>,
    pub deafen_on_join: bool,
    /// Long messages fold to twelve lines, with Show more.
    pub collapse_long: bool,
    /// DMs pinned to the top of the list, by room id, the first pinned
    /// first.
    pub pinned: Vec<String>,
    /// A one-to-one DM's profile beside it is hidden; messages from people
    /// blocked don't show at all; a call from someone's menu starts without
    /// asking; the member list's Activity shows its header alone.
    pub profile_panel_hidden: bool,
    pub hide_blocked: bool,
    pub skip_call_prompt: bool,
    pub activity_cards_hidden: bool,
    /// How wide the thread beside the channel was dragged.
    pub aside_width: Option<f32>,
    /// How far the window is zoomed (`zoom::LEVELS`), and whether the
    /// desktop draws its title bar, as Appearance chose; unset, the
    /// desktop's way decides.
    pub zoom: Option<f32>,
    pub system_title_bar: Option<bool>,
    /// What calls send and play, as Voice's settings in the Slint client
    /// keep them; see [`Voice`].
    #[serde(flatten)]
    pub voice: Voice,
    /// What Settings' other pages keep; see [`Prefs`].
    #[serde(flatten)]
    pub prefs: Prefs,
}

/// Settings' pages besides Voice's, under the keys the Slint client keeps
/// them by, each as it means there: `None` is the default, which for most
/// switches is on.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Messages: the clock, its seconds, the date's order (an index into
    /// `clock::DateFormat::ALL`, following the clock unless chosen), the
    /// text's size (0 small, 1 default, 2 large), link previews, boost gems,
    /// mentioning those replied to, tracking taken out of links sent,
    /// sharing that this account types, and when animations play.
    pub twelve_hour_clock: bool,
    pub clock_seconds: bool,
    pub date_format: Option<i32>,
    pub text_size: Option<i32>,
    pub link_previews: Option<bool>,
    pub boost_gems: Option<bool>,
    pub reply_ping: Option<bool>,
    pub strip_tracking: Option<bool>,
    pub share_typing: Option<bool>,
    pub animate: Animate,
    /// Appearance's profile switches, and nameplates.
    pub nameplates: Option<bool>,
    pub profile_effects: Option<bool>,
    pub decorations: Option<bool>,
    pub moving_cosmetics: Option<bool>,
    pub name_styles: Option<bool>,
    pub server_tags: Option<bool>,
    pub profile_frames: Option<bool>,
    /// Notifications: shown, saying what messages say, flashing the
    /// taskbar.
    pub notifications: Option<bool>,
    pub notification_text: Option<bool>,
    pub flash_taskbar: Option<bool>,
    /// Advanced: rendering with the GPU from the next start, closing to the
    /// tray, starting another Yapfast rather than showing this one.
    pub gpu_rendering: bool,
    pub close_to_tray: Option<bool>,
    pub multiple_windows: bool,
    /// Discord: channels this account can't see listed locked, reading
    /// marked on Discord, idle shown while away.
    pub show_hidden_channels: bool,
    pub sync_discord_reads: Option<bool>,
    pub show_idle: Option<bool>,
    /// Game Activity's own games: an executable and what to show for it.
    pub game_executables: Vec<(String, String)>,
    /// About: looking for releases, nightlies among them, the release
    /// skipped, and the last told of in a notification.
    pub check_for_updates: bool,
    pub update_nightlies: bool,
    pub skipped_update: Option<String>,
    pub announced_update: Option<String>,
}

/// When GIFs, animated emoji and stickers play, as Messages' choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Animate {
    /// While Yapfast's window has the focus, as in Discord.
    #[default]
    Focused,
    Always,
    Never,
}

impl Prefs {
    /// The profile switches, each on unless turned off.
    pub fn cosmetics(&self) -> yapfast_core::application::Cosmetics {
        let default = yapfast_core::application::Cosmetics::default();
        yapfast_core::application::Cosmetics {
            effects: self.profile_effects.unwrap_or(default.effects),
            decorations: self.decorations.unwrap_or(default.decorations),
            moving: self.moving_cosmetics.unwrap_or(default.moving),
            name_styles: self.name_styles.unwrap_or(default.name_styles),
            server_tags: self.server_tags.unwrap_or(default.server_tags),
            frames: self.profile_frames.unwrap_or(default.frames),
        }
    }

    /// Hands the date's order and the clock to what formats times, as the
    /// Slint client does.
    pub fn apply_clock(&self) {
        use yapfast_core::clock;
        clock::set_date_format(self.date_format.and_then(|index| {
            clock::DateFormat::ALL
                .get(usize::try_from(index).ok()?)
                .copied()
        }));
        // After the date's order, which Discord's timestamps follow.
        clock::set_twelve_hour_clock(self.twelve_hour_clock);
    }
}

/// Voice's settings in `layout.json`, which the core is told of as the
/// window opens ([`Voice::apply`]), as the Slint client tells it, and
/// Settings' Voice & video changes.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
pub struct Voice {
    /// Your voice is evened out in calls; on unless turned off.
    pub level_voice: Option<bool>,
    pub noise_suppression: yapfast_core::noise::Suppressor,
    pub noise_strength: yapfast_core::noise::Strength,
    pub reduce_room_echo: bool,
    pub windows_microphone_effects: bool,
    /// The quietest speech Discord calls send, in dBFS.
    pub input_sensitivity: Option<i32>,
    /// Push to talk, its key (kept as the Slint client wrote it, for the
    /// root crate's watcher) and its release delay in ms.
    pub input_mode: InputMode,
    pub push_to_talk_key: Option<serde_json::Value>,
    pub push_to_talk_release_delay: Option<u32>,
    /// How loud calls send the microphone and play, as sliders'
    /// percentages from 0% to 200%; 100% unless changed.
    pub microphone_volume: Option<u8>,
    pub speaker_volume: Option<u8>,
    /// People's own volumes and their streams', by Discord or Matrix id.
    pub volumes: std::collections::BTreeMap<String, f32>,
    pub stream_volumes: std::collections::BTreeMap<String, f32>,
    /// Those muted for this account alone, and those whose soundboard
    /// sounds it doesn't hear.
    pub muted_people: BTreeSet<String>,
    pub soundboard_muted_people: BTreeSet<String>,
    /// Encoding a shared screen on the GPU; on unless turned off.
    pub gpu_encoding: Option<bool>,
    /// Yapfast's sounds: their instrument, how loud as a slider's
    /// percentage, and each one's own volume while adjusted.
    pub sound_set: yapfast_core::sounds::SoundSet,
    pub sound_volume: Option<u8>,
    pub adjust_each_sound: bool,
    pub each_sound: std::collections::BTreeMap<yapfast_core::sounds::Sound, u8>,
}

/// How calls decide when to send the microphone, as Discord's Input Mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    #[default]
    VoiceActivity,
    PushToTalk,
}

impl Voice {
    /// Tells the core how calls send and play.
    pub fn apply(&self) {
        use yapfast_core::{leveling, noise, push_to_talk, volume};
        leveling::set_enabled(self.level_voice.unwrap_or(true));
        noise::set(self.noise_suppression);
        noise::set_strength(self.noise_strength);
        noise::set_dereverb(self.reduce_room_echo);
        yapfast_core::set_windows_effects(self.windows_microphone_effects);
        let sensitivity = self
            .input_sensitivity
            .unwrap_or(noise::DEFAULT_SENSITIVITY as i32)
            .clamp(-80, 0);
        noise::set_sensitivity(sensitivity as f32);
        push_to_talk::set_on(self.input_mode == InputMode::PushToTalk);
        push_to_talk::set_release_delay(
            self.push_to_talk_release_delay
                .unwrap_or(push_to_talk::DEFAULT_RELEASE_DELAY)
                .min(push_to_talk::MOST_RELEASE_DELAY),
        );
        self.play_volumes();
        for (person, gain) in &self.volumes {
            volume::set(volume::Kind::Voice, person, *gain);
        }
        for (person, gain) in &self.stream_volumes {
            volume::set(volume::Kind::Stream, person, *gain);
        }
        for person in &self.muted_people {
            volume::mute(person, true);
        }
        for person in &self.soundboard_muted_people {
            volume::mute_soundboard(person, true);
        }
        yapfast_core::stream_worker::set_hardware_encoding(self.gpu_encoding.unwrap_or(true));
    }

    /// Calls send the microphone and play at the volume sliders' values.
    pub fn play_volumes(&self) {
        let gain = |value: Option<u8>| {
            yapfast_core::volume::from_slider(f32::from(value.unwrap_or(100).min(200)))
        };
        yapfast_core::volume::set_microphone(gain(self.microphone_volume));
        yapfast_core::volume::set_speakers(gain(self.speaker_volume));
    }

    /// The push-to-talk key, while push to talk is on.
    pub fn talk_key(&self) -> Option<&serde_json::Value> {
        self.push_to_talk_key
            .as_ref()
            .filter(|_| self.input_mode == InputMode::PushToTalk)
    }

    /// How loud one of Yapfast's sounds plays, as a gain.
    pub fn sound_gain(&self, sound: yapfast_core::sounds::Sound) -> f32 {
        use yapfast_core::sounds::gain;
        let each = self
            .each_sound
            .get(&sound)
            .copied()
            .filter(|_| self.adjust_each_sound)
            .unwrap_or(100);
        gain(self.sound_volume.unwrap_or(100)) * gain(each)
    }
}

impl Layout {
    /// What the core is told of these as the window opens, as the Slint
    /// client tells it.
    pub fn actions(&self) -> Vec<yapfast_core::application::Action> {
        use yapfast_core::application::Action;
        vec![
            Action::ShowProfilePanel(!self.profile_panel_hidden),
            Action::HideBlocked(self.hide_blocked),
            Action::SkipCallPrompt(self.skip_call_prompt),
            Action::ActivityCardsHidden(self.activity_cards_hidden),
            Action::SetPinned(self.pinned.clone()),
            Action::ShowHiddenChannels(self.prefs.show_hidden_channels),
            Action::SyncDiscordReads(self.prefs.sync_discord_reads.unwrap_or(true)),
            Action::ReplyPing(self.prefs.reply_ping.unwrap_or(true)),
            Action::SetCosmetics(self.prefs.cosmetics()),
            Action::StripTracking(self.prefs.strip_tracking.unwrap_or(true)),
            Action::HideNameplates(!self.prefs.nameplates.unwrap_or(true)),
        ]
    }
}

impl Layout {
    /// Reads `layout.json`; a missing or unreadable file keeps nothing.
    pub fn read(root: &Path) -> Self {
        std::fs::read(root.join(LAYOUT))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
}

/// Keeps `value` under `key` in `layout.json`, leaving the rest as it is.
pub fn store(root: &Path, key: &str, value: impl serde::Serialize) {
    let path = root.join(LAYOUT);
    let mut layout = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    layout[key] = serde_json::to_value(value).expect("layout values serialize");
    let result = std::fs::create_dir_all(root).and_then(|()| {
        std::fs::write(
            &path,
            serde_json::to_vec(&layout).expect("layout serializes"),
        )
    });
    if let Err(error) = result {
        tracing::warn!(%error, key, "could not keep the window's layout");
    }
}
