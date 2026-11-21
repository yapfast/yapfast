//! What the window keeps in the data folder's `layout.json`: the theme and
//! the person's own themes (`theme.rs`), the rail's open folders, each
//! server's collapsed channel categories, whether the member list is
//! hidden, whether long messages fold, pinned DMs, the people settings
//! the core is told of as the window opens ([`Layout::actions`]), and what
//! the core counts of this device's use ([`Layout::remember`]). Each
//! change ([`store`]) sets its one key in the file as it's kept in memory,
//! leaving the rest as it was; a thread of its own writes the file once
//! changes have stopped for a second, and reading it, or the window ending,
//! writes what's waiting first. People's files hold these keys already, so
//! a key keeps its name and its meaning.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock, PoisonError, mpsc},
    time::Duration,
};

use serde::Deserialize;
use yapfast_core::{
    application::{Application, StickerUse},
    chat::Sticker,
    keybinds::{Binding, Shortcut},
    theme::Custom,
};

pub use yapfast_core::layout_file::LAYOUT;

/// What this client reads of `layout.json`.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
pub struct Layout {
    /// The version whose settings this person counts as having met, and
    /// the NEW settings they have looked at; see `yapfast_core::new_tags`.
    pub first_version: Option<String>,
    pub seen_new: Vec<String>,
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
    /// How wide the sidebar and the member list were dragged; unset, each
    /// has its own width.
    pub sidebar_width: Option<f32>,
    pub members_width: Option<f32>,
    /// How wide the thread beside the channel was dragged.
    pub aside_width: Option<f32>,
    /// How wide the expression picker's edge was dragged.
    #[serde(alias = "gif_picker_width")]
    pub picker_width: Option<f32>,
    /// How far the window is zoomed (`zoom::LEVELS`), and whether the
    /// desktop draws its title bar, as Appearance chose; unset, the
    /// desktop's way decides.
    pub zoom: Option<f32>,
    pub system_title_bar: Option<bool>,
    /// The window's size, its place and whether it was maximized, for the
    /// next start.
    pub placement: Option<Placement>,
    /// Each server's channel last open, by server id, which a click on the
    /// server opens again.
    pub channels: HashMap<String, String>,
    /// How often this device added each reaction, for the ones offered
    /// first; the stickers it sent, for Frequently Used, and those it
    /// keeps, for Favorites.
    pub reactions: HashMap<String, u32>,
    pub sticker_uses: Vec<KeptSticker>,
    pub sticker_favorites: Vec<KeptSticker>,
    /// The Keybinds page's keybinds, in its order.
    pub keybinds: Vec<Binding>,
    /// Each place's searches, newest first, by server or DM id, as
    /// Discord's box keeps them (`search_popout::HISTORY`).
    pub search_history: HashMap<String, Vec<String>>,
    /// What calls send and play; see [`Voice`].
    #[serde(flatten)]
    pub voice: Voice,
    /// What Settings' other pages keep; see [`Prefs`].
    #[serde(flatten)]
    pub prefs: Prefs,
}

/// The window's size while it isn't maximized, in the desktop's own units
/// (the zoom multiplies the app's into them), where it was then, and
/// whether it was maximized.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Placement {
    pub width: u32,
    pub height: u32,
    /// The frame's top left corner in the desktop's pixels, where the
    /// desktop says it and puts a window back (`Desktop::window_place`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[i32; 2]>,
    pub maximized: bool,
}

/// A sticker this device sent or keeps, as `layout.json` holds it.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct KeptSticker {
    pub id: String,
    pub name: String,
    /// Discord's `format_type`, which its picture's address takes.
    pub format: u64,
    /// Its picture at the picker's tile size.
    pub picture: String,
    /// How often it was sent; a kept one has no count.
    #[serde(skip_serializing_if = "is_zero")]
    pub uses: u32,
}

fn is_zero(count: &u32) -> bool {
    *count == 0
}

impl KeptSticker {
    fn of(sticker: &Sticker, uses: u32) -> Self {
        Self {
            id: sticker.id.clone(),
            name: sticker.name.clone(),
            format: sticker.format,
            picture: sticker.picture.clone(),
            uses,
        }
    }

    fn is(&self, sticker: &Sticker, uses: u32) -> bool {
        self.uses == uses
            && self.id == sticker.id
            && self.name == sticker.name
            && self.format == sticker.format
            && self.picture == sticker.picture
    }

    fn sticker(&self) -> Sticker {
        Sticker {
            id: self.id.clone(),
            name: self.name.clone(),
            format: self.format,
            picture: self.picture.clone(),
            usable: true,
        }
    }
}

/// Settings' pages besides Voice's, each under its key in `layout.json`:
/// `None` is the default, which for most switches is on.
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

    /// Hands the date's order and the clock to what formats times.
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
/// window opens ([`Voice::apply`]), and Settings' Voice & video changes.
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
    /// Push to talk, its keybind and its release delay in ms.
    pub input_mode: InputMode,
    pub push_to_talk_key: Option<Shortcut>,
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
        // The tests make many windows at once, and each plays its volumes
        // as it opens: one checking them holds them meanwhile.
        #[cfg(test)]
        let _held = (!tests::HOLDING.get()).then(tests::lock);
        let gain = |value: Option<u8>| {
            yapfast_core::volume::from_slider(f32::from(value.unwrap_or(100).min(200)))
        };
        yapfast_core::volume::set_microphone(gain(self.microphone_volume));
        yapfast_core::volume::set_speakers(gain(self.speaker_volume));
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
    /// What the core is told of these as the window opens.
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
    /// Hands the core what the last start kept of this device's use: each
    /// server's last channel, the reactions' counts, the stickers sent and
    /// kept.
    pub fn remember(&self, app: &mut Application) {
        app.remember_channels(self.channels.clone());
        app.remember_reaction_uses(self.reactions.clone());
        app.remember_sticker_favorites(
            self.sticker_favorites
                .iter()
                .map(KeptSticker::sticker)
                .collect(),
        );
        app.remember_sticker_uses(
            self.sticker_uses
                .iter()
                .map(|kept| StickerUse {
                    sticker: kept.sticker(),
                    uses: kept.uses,
                })
                .collect(),
        );
    }

    /// Keeps what the core changed of those for the next start. This runs
    /// after every event, so each is compared in place and copied only
    /// once it differs; the last channels change only as another room
    /// opens, which `switched` says.
    pub fn keep_uses(&mut self, root: &Path, app: &Application, switched: bool) {
        if app.reaction_uses() != &self.reactions {
            self.reactions.clone_from(app.reaction_uses());
            store(root, "reactions", &self.reactions);
        }
        let uses = app.sticker_uses();
        if uses.len() != self.sticker_uses.len()
            || !uses
                .iter()
                .zip(&self.sticker_uses)
                .all(|(used, kept)| kept.is(&used.sticker, used.uses))
        {
            self.sticker_uses = uses
                .iter()
                .map(|used| KeptSticker::of(&used.sticker, used.uses))
                .collect();
            store(root, "sticker_uses", &self.sticker_uses);
        }
        let favorites = app.sticker_favorites();
        if favorites.len() != self.sticker_favorites.len()
            || !favorites
                .iter()
                .zip(&self.sticker_favorites)
                .all(|(sticker, kept)| kept.is(sticker, 0))
        {
            self.sticker_favorites = favorites
                .iter()
                .map(|sticker| KeptSticker::of(sticker, 0))
                .collect();
            store(root, "sticker_favorites", &self.sticker_favorites);
        }
        // The preview's servers are no account's.
        if switched && !app.state.preview && app.state.account.is_some() {
            let mut changed = false;
            for (server, room) in app.last_channels() {
                if self.channels.get(&server) != Some(&room) {
                    self.channels.insert(server, room);
                    changed = true;
                }
            }
            if changed {
                store(root, "channels", &self.channels);
            }
        }
    }
}

impl Layout {
    /// Reads `layout.json`; a missing or unreadable file keeps nothing. A
    /// key this client can't read (of the wrong type, or from another
    /// version) loses only itself, not every other setting.
    pub fn read(root: &Path) -> Self {
        flush(root);
        let Ok(bytes) = std::fs::read(root.join(LAYOUT)) else {
            return Self::default();
        };
        let value = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(%error, "layout.json isn't JSON; starting from nothing");
                return Self::default();
            }
        };
        match Self::deserialize(&value) {
            Ok(layout) => layout,
            Err(error) => {
                tracing::warn!(%error, "layout.json has keys this window can't read");
                Self::lenient(value)
            }
        }
    }

    /// The layout of every key of `value` that reads, one at a time.
    fn lenient(value: serde_json::Value) -> Self {
        let serde_json::Value::Object(keys) = value else {
            return Self::default();
        };
        let mut kept = serde_json::Map::new();
        for (key, value) in keys {
            kept.insert(key.clone(), value);
            if Self::deserialize(&serde_json::Value::Object(kept.clone())).is_err() {
                tracing::warn!(key, "a layout.json key this window can't read is left out");
                kept.remove(&key);
            }
        }
        Self::deserialize(&serde_json::Value::Object(kept)).unwrap_or_default()
    }

    /// Reads `layout.json` as the window opens: a fresh install keeps the
    /// version it has met, so no setting there now is NEW to it, and an
    /// older layout without one has met the release before NEW tags.
    pub fn open(root: &Path, running: &str) -> Self {
        let fresh = !root.join(LAYOUT).exists();
        let mut layout = Self::read(root);
        if layout.first_version.is_none() {
            let first = yapfast_core::new_tags::first_version(fresh, running);
            if fresh {
                store(root, "first_version", &first);
            }
            layout.first_version = Some(first);
        }
        layout
    }
}

/// How long a change waits for the next before `layout.json` is written,
/// so changes that come together (a theme's colours, a drag's end, a
/// toggle's two keys) write it once.
const WRITE_AFTER: Duration = Duration::from_millis(1000);

/// Where a `layout.json` that wasn't one is kept, rather than lost under
/// the one key written next.
const SET_ASIDE: &str = "layout.json.unread";

/// Each data folder's layout with changes not yet written: the whole file
/// as it will be, and whether the folder was there (one gone since, as a
/// test's, isn't made again for it).
static PENDING: Mutex<BTreeMap<PathBuf, Waiting>> = Mutex::new(BTreeMap::new());

struct Waiting {
    layout: serde_json::Value,
    folder: bool,
}

fn pending() -> MutexGuard<'static, BTreeMap<PathBuf, Waiting>> {
    PENDING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Keeps `value` under `key` in `layout.json`, leaving the rest as it is:
/// written a second after the changes stop.
pub fn store(root: &Path, key: &str, value: impl serde::Serialize) {
    let value = serde_json::to_value(value).expect("layout values serialize");
    pending()
        .entry(root.to_owned())
        .or_insert_with(|| Waiting {
            layout: on_disk(root),
            folder: root.is_dir(),
        })
        .layout[key] = value;
    writer().send(()).ok();
}

/// The file as it is: nothing for a missing one, and one that isn't a
/// layout set aside.
fn on_disk(root: &Path) -> serde_json::Value {
    let path = root.join(LAYOUT);
    let empty = || serde_json::Value::Object(Default::default());
    let Ok(bytes) = std::fs::read(&path) else {
        return empty();
    };
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(layout) if layout.is_object() => layout,
        _ => {
            tracing::warn!("layout.json isn't a layout; keeping it as {SET_ASIDE}");
            if let Err(error) = std::fs::rename(&path, root.join(SET_ASIDE)) {
                tracing::warn!(%error, "could not set layout.json aside");
            }
            empty()
        }
    }
}

/// The thread that writes the layouts a while after their last change.
fn writer() -> &'static mpsc::Sender<()> {
    static WRITER: OnceLock<mpsc::Sender<()>> = OnceLock::new();
    WRITER.get_or_init(|| {
        let (sender, changes) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("yapfast-layout".into())
            .spawn(move || {
                while changes.recv().is_ok() {
                    // Each change waits out the next.
                    while changes.recv_timeout(WRITE_AFTER).is_ok() {}
                    flush_all();
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "no layout writer; changes are written as they're read");
        }
        sender
    })
}

/// Writes `root`'s waiting changes now.
pub fn flush(root: &Path) {
    let mut pending = pending();
    if let Some(waiting) = pending.remove(root) {
        write(root, &waiting);
    }
}

/// Writes every waiting change now, as the window ends.
pub fn flush_all() {
    // Held while writing, so a change meanwhile waits for the file it
    // goes on rather than reading the one being replaced.
    let mut pending = pending();
    for (root, waiting) in std::mem::take(&mut *pending) {
        write(&root, &waiting);
    }
}

fn write(root: &Path, waiting: &Waiting) {
    if waiting.folder && !root.is_dir() {
        return;
    }
    let bytes = serde_json::to_vec(&waiting.layout).expect("layout serializes");
    if let Err(error) = yapfast_core::layout_file::write(root, &bytes) {
        tracing::warn!(%error, "could not keep the window's layout");
    }
}

#[cfg(test)]
mod writing {
    use super::*;

    #[test]
    fn changes_together_write_once_and_reading_writes_them_first() {
        let folder = tempfile::tempdir().expect("a folder");
        let root = folder.path();
        std::fs::write(root.join(LAYOUT), r#"{"theme": "sakura"}"#).expect("the layout");
        store(root, "members_hidden", true);
        store(root, "zoom", 1.25);
        let on_disk = std::fs::read_to_string(root.join(LAYOUT)).expect("the file");
        assert_eq!(on_disk, r#"{"theme": "sakura"}"#, "not yet written");
        let layout = Layout::read(root);
        assert!(layout.members_hidden);
        assert_eq!(layout.zoom, Some(1.25));
        assert_eq!(layout.theme.as_deref(), Some("sakura"), "the rest kept");
    }

    #[test]
    fn a_file_that_isnt_a_layout_is_set_aside_not_overwritten() {
        let folder = tempfile::tempdir().expect("a folder");
        let root = folder.path();
        std::fs::write(root.join(LAYOUT), b"{\"theme\": \"sak").expect("the layout");
        store(root, "zoom", 1.25);
        flush(root);
        let aside = std::fs::read(root.join(SET_ASIDE)).expect("set aside");
        assert_eq!(aside, b"{\"theme\": \"sak");
        assert_eq!(Layout::read(root).zoom, Some(1.25));
    }

    #[test]
    fn the_writer_writes_a_while_after_the_last_change() {
        let folder = tempfile::tempdir().expect("a folder");
        let root = folder.path();
        store(root, "zoom", 1.25);
        // Other tests' changes wait out theirs too.
        let written = (0..300).any(|_| {
            std::thread::sleep(WRITE_AFTER / 10);
            std::fs::read_to_string(root.join(LAYOUT)).is_ok_and(|file| file.contains("1.25"))
        });
        assert!(written);
    }
}

/// The process's volumes are one for every window; see [`hold_volumes`].
#[cfg(test)]
pub(crate) mod tests {
    use std::{
        cell::Cell,
        sync::{Mutex, MutexGuard, PoisonError},
    };

    static VOLUMES: Mutex<()> = Mutex::new(());

    thread_local! {
        /// This thread holds [`VOLUMES`].
        pub static HOLDING: Cell<bool> = const { Cell::new(false) };
    }

    pub fn lock() -> MutexGuard<'static, ()> {
        VOLUMES.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Held by a test that sets the volumes and reads them back: until it
    /// drops, no other window plays its own.
    pub struct Held(#[allow(dead_code, reason = "held for its drop")] MutexGuard<'static, ()>);

    impl Drop for Held {
        fn drop(&mut self) {
            HOLDING.set(false);
        }
    }

    pub fn hold_volumes() -> Held {
        let held = Held(lock());
        HOLDING.set(true);
        held
    }
}
