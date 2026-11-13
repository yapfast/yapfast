//! A theme's sounds and icons on the window (`docs/themes.md`, "Sounds and
//! icons"). [`load`] reads them when a theme is shown: each sound decoded
//! once and shared by every play, each icon an SVG Slint draws at the size it
//! shows. [`show`] puts them on the `IconPack` and `Cue` globals
//! (`ui/media.slint`) and [`hide`] puts the stock ones back, as
//! `theme_look`'s paint and reset do for the rest of a theme, so a protected
//! dialog shows stock icons and plays no theme sound.
//!
//! A theme with no sounds or icons sets nothing here: the globals keep their
//! stock values, `IconPack.active` stays false so no colour asks Rust, and no
//! control asks to play.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::{Brush, Color, ComponentHandle, Image};
use yapfast_core::sounds::Sound;
use yapfast_core::themes::media::{self, IconColour, SoundSlot};
use yapfast_core::themes::package::Resolved;
use yapfast_ui::{Cue, IconPack, YapfastWindow};

/// The person's choices for one theme's sounds, kept in `layout.json` by the
/// theme's id when they differ from these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Its notification sounds play, in place of Yapfast's own.
    pub on: bool,
    /// Its interface sounds play; off until the person turns them on.
    pub interface: bool,
    /// The interface sounds' volume, 0 to 100.
    pub volume: u8,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            on: true,
            interface: false,
            volume: 50,
        }
    }
}

/// What the window plays on, which the interface keeps up to date while a
/// theme has sounds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Context {
    /// Do Not Disturb: no interface sound plays.
    pub quiet: bool,
    pub speaker: Option<String>,
    /// The Sounds volume of Settings → Voice, as a gain.
    pub gain: f32,
}

struct Takes {
    takes: Vec<Arc<[i16]>>,
    volume: f32,
}

#[derive(Default)]
struct State {
    sounds: BTreeMap<SoundSlot, Takes>,
    /// Replaced icons by their index in [`NAMES`], and how they're painted.
    icons: Vec<(usize, Image, IconColour)>,
    /// The colour each icon not tinted by its place is drawn with: none for
    /// its own colours.
    painted: Vec<(Image, Option<Color>)>,
    shown: bool,
    /// The icons `IconPack` shows aren't all stock.
    swapped: bool,
    prefs: Prefs,
    context: Context,
    /// Counts the themes loaded with icons, for [`generation`].
    loads: u64,
    /// The stock icons, read from `IconPack` before a theme first set any.
    stock: Vec<Image>,
    /// Decoded sounds and loaded icons by the file's digest, kept while a
    /// theme uses them so showing it again decodes nothing.
    decoded: HashMap<u64, Arc<[i16]>>,
    svgs: HashMap<u64, Image>,
    last_hover: Option<Instant>,
    picked: u32,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    /// Counts loads, drops, shows and hides, for [`changes`].
    static CHANGES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn changed() {
    CHANGES.set(CHANGES.get().wrapping_add(1));
}

/// Changes whenever what a theme's sounds and icons are does: the window
/// checks it on each refresh and, when it moved, puts the person's choices
/// and Rust's icons in step with it.
pub fn changes() -> u64 {
    CHANGES.get()
}

macro_rules! icon_pack {
    ($($name:literal => $get:ident, $set:ident;)*) => {
        /// Every icon `IconPack` has, in the order its getters are read.
        const NAMES: &[&str] = &[$($name),*];

        fn read_stock(pack: &IconPack<'_>) -> Vec<Image> {
            vec![$(pack.$get()),*]
        }

        fn set(pack: &IconPack<'_>, index: usize, image: Image) {
            match NAMES[index] {
                $($name => pack.$set(image),)*
                _ => unreachable!("listed"),
            }
        }
    };
}

icon_pack! {
    "arrow-down" => get_arrow_down, set_arrow_down;
    "arrow-left" => get_arrow_left, set_arrow_left;
    "arrow-right" => get_arrow_right, set_arrow_right;
    "arrow-up" => get_arrow_up, set_arrow_up;
    "at-sign" => get_at_sign, set_at_sign;
    "ban" => get_ban, set_ban;
    "bell" => get_bell, set_bell;
    "bell-off" => get_bell_off, set_bell_off;
    "brush" => get_brush, set_brush;
    "calendar" => get_calendar, set_calendar;
    "chart-bar" => get_chart_bar, set_chart_bar;
    "check" => get_check, set_check;
    "chevron-down" => get_chevron_down, set_chevron_down;
    "chevron-left" => get_chevron_left, set_chevron_left;
    "chevron-right" => get_chevron_right, set_chevron_right;
    "chevrons-right" => get_chevrons_right, set_chevrons_right;
    "circle-plus" => get_circle_plus, set_circle_plus;
    "circle-stop" => get_circle_stop, set_circle_stop;
    "clipboard-paste" => get_clipboard_paste, set_clipboard_paste;
    "clock" => get_clock, set_clock;
    "community" => get_community, set_community;
    "copy" => get_copy, set_copy;
    "copy-plus" => get_copy_plus, set_copy_plus;
    "crown" => get_crown, set_crown;
    "discord" => get_discord, set_discord;
    "download" => get_download, set_download;
    "ellipsis" => get_ellipsis, set_ellipsis;
    "external-link" => get_external_link, set_external_link;
    "eye" => get_eye, set_eye;
    "eye-off" => get_eye_off, set_eye_off;
    "eyedropper" => get_eyedropper, set_eyedropper;
    "file" => get_file, set_file;
    "folder" => get_folder, set_folder;
    "folder-plus" => get_folder_plus, set_folder_plus;
    "forward" => get_forward, set_forward;
    "gamepad" => get_gamepad, set_gamepad;
    "gavel" => get_gavel, set_gavel;
    "gem" => get_gem, set_gem;
    "gif" => get_gif, set_gif;
    "hand" => get_hand, set_hand;
    "hash" => get_hash, set_hash;
    "hash-lock" => get_hash_lock, set_hash_lock;
    "hash-warning" => get_hash_warning, set_hash_warning;
    "headphone-off" => get_headphone_off, set_headphone_off;
    "headphones" => get_headphones, set_headphones;
    "help-circle" => get_help_circle, set_help_circle;
    "history" => get_history, set_history;
    "id-filled" => get_id_filled, set_id_filled;
    "image" => get_image, set_image;
    "image-lock" => get_image_lock, set_image_lock;
    "image-warning" => get_image_warning, set_image_warning;
    "inbox" => get_inbox, set_inbox;
    "link" => get_link, set_link;
    "lock" => get_lock, set_lock;
    "log-out" => get_log_out, set_log_out;
    "logo" => get_logo, set_logo;
    "maximize" => get_maximize, set_maximize;
    "megaphone" => get_megaphone, set_megaphone;
    "megaphone-lock" => get_megaphone_lock, set_megaphone_lock;
    "megaphone-warning" => get_megaphone_warning, set_megaphone_warning;
    "message-circle" => get_message_circle, set_message_circle;
    "message-square-text" => get_message_square_text, set_message_square_text;
    "message-square-text-lock" => get_message_square_text_lock, set_message_square_text_lock;
    "message-square-text-warning" => get_message_square_text_warning, set_message_square_text_warning;
    "mic" => get_mic, set_mic;
    "mic-off" => get_mic_off, set_mic_off;
    "minimize" => get_minimize, set_minimize;
    "minus" => get_minus, set_minus;
    "moon" => get_moon, set_moon;
    "music" => get_music, set_music;
    "noise-bars" => get_noise_bars, set_noise_bars;
    "noise-bars-off" => get_noise_bars_off, set_noise_bars_off;
    "palette" => get_palette, set_palette;
    "pause" => get_pause, set_pause;
    "pen" => get_pen, set_pen;
    "pencil" => get_pencil, set_pencil;
    "phone" => get_phone, set_phone;
    "phone-off" => get_phone_off, set_phone_off;
    "pin" => get_pin, set_pin;
    "pin-off" => get_pin_off, set_pin_off;
    "play" => get_play, set_play;
    "plugins" => get_plugins, set_plugins;
    "plus" => get_plus, set_plus;
    "refresh-cw" => get_refresh_cw, set_refresh_cw;
    "reply" => get_reply, set_reply;
    "restore" => get_restore, set_restore;
    "rules" => get_rules, set_rules;
    "scissors" => get_scissors, set_scissors;
    "screen-share" => get_screen_share, set_screen_share;
    "screen-share-off" => get_screen_share_off, set_screen_share_off;
    "scroll-down" => get_scroll_down, set_scroll_down;
    "scroll-left" => get_scroll_left, set_scroll_left;
    "scroll-right" => get_scroll_right, set_scroll_right;
    "scroll-up" => get_scroll_up, set_scroll_up;
    "search" => get_search, set_search;
    "send-horizontal" => get_send_horizontal, set_send_horizontal;
    "settings" => get_settings, set_settings;
    "shield" => get_shield, set_shield;
    "shop" => get_shop, set_shop;
    "signal-high" => get_signal_high, set_signal_high;
    "signal-low" => get_signal_low, set_signal_low;
    "signal-medium" => get_signal_medium, set_signal_medium;
    "slash" => get_slash, set_slash;
    "smile" => get_smile, set_smile;
    "smile-plus" => get_smile_plus, set_smile_plus;
    "soundboard" => get_soundboard, set_soundboard;
    "square" => get_square, set_square;
    "square-pen" => get_square_pen, set_square_pen;
    "stage" => get_stage, set_stage;
    "stage-lock" => get_stage_lock, set_stage_lock;
    "star" => get_star, set_star;
    "star-filled" => get_star_filled, set_star_filled;
    "sticker" => get_sticker, set_sticker;
    "sticky-note" => get_sticky_note, set_sticky_note;
    "sun" => get_sun, set_sun;
    "text-select" => get_text_select, set_text_select;
    "thread" => get_thread, set_thread;
    "trash-2" => get_trash_2, set_trash_2;
    "trending-up" => get_trending_up, set_trending_up;
    "triangle-alert" => get_triangle_alert, set_triangle_alert;
    "trophy" => get_trophy, set_trophy;
    "tv" => get_tv, set_tv;
    "user" => get_user, set_user;
    "user-minus" => get_user_minus, set_user_minus;
    "user-pen" => get_user_pen, set_user_pen;
    "user-plus" => get_user_plus, set_user_plus;
    "user-x" => get_user_x, set_user_x;
    "users" => get_users, set_users;
    "verified" => get_verified, set_verified;
    "video" => get_video, set_video;
    "video-off" => get_video_off, set_video_off;
    "volume-2" => get_volume_2, set_volume_2;
    "volume-2-bold" => get_volume_2_bold, set_volume_2_bold;
    "volume-2-lock" => get_volume_2_lock, set_volume_2_lock;
    "volume-2-warning" => get_volume_2_warning, set_volume_2_warning;
    "volume-x" => get_volume_x, set_volume_x;
    "x" => get_x, set_x;
}

pub(super) fn digest(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Reads a resolved theme's sounds and icons from its folder. `colour` gives
/// a colour token's value in this variant. Refuses the theme, naming the file,
/// if one doesn't decode; the window then keeps what it showed.
pub fn load(
    resolved: &Resolved,
    folder: &Path,
    colour: impl Fn(&str) -> Option<Color>,
    checked: bool,
) -> Result<(), String> {
    let mut sounds = BTreeMap::new();
    let mut icons = Vec::new();
    let mut decoded = HashMap::new();
    let mut svgs = HashMap::new();
    let (old_decoded, old_svgs) = STATE.with_borrow_mut(|state| {
        (
            std::mem::take(&mut state.decoded),
            std::mem::take(&mut state.svgs),
        )
    });
    let read = |file: &Path| {
        std::fs::read(folder.join(file)).map_err(|error| format!("{}: {error}", file.display()))
    };
    for (slot, choice) in &resolved.sounds {
        let mut takes = Vec::with_capacity(choice.files.len());
        for file in &choice.files {
            let bytes = read(file)?;
            let key = digest(&bytes);
            let samples = match decoded.get(&key).or_else(|| old_decoded.get(&key)) {
                Some(samples) => Arc::clone(samples),
                None => {
                    let samples = media::decode_sound(file, &bytes)
                        .map_err(|error| format!("{}: {error}", file.display()))?;
                    if media::seconds(&samples) > slot.longest() + 0.005 {
                        return Err(format!(
                            "{}: a {} sound lasts at most {} s",
                            file.display(),
                            slot.name(),
                            slot.longest()
                        ));
                    }
                    Arc::from(samples)
                }
            };
            decoded.insert(key, Arc::clone(&samples));
            takes.push(samples);
        }
        sounds.insert(
            *slot,
            Takes {
                takes,
                volume: choice.volume,
            },
        );
    }
    let mut painted = Vec::new();
    for (name, choice) in &resolved.icons {
        let Some(index) = NAMES.iter().position(|known| known == name) else {
            continue;
        };
        let bytes = read(&choice.file)?;
        // Shared only by icons painted the same way: `paint` tells icons
        // apart by their image, so one file painted two ways loads twice.
        let key = digest(&bytes) ^ digest(format!("{:?}", choice.colour).as_bytes());
        let image = match svgs.get(&key).or_else(|| old_svgs.get(&key)) {
            Some(image) => Image::clone(image),
            None => {
                // A theme prepared off the UI thread had its files checked
                // there, a moment ago.
                if !checked {
                    yapfast_core::themes::check::check_svg(&bytes)
                        .map_err(|error| format!("{}: {error}", choice.file.display()))?;
                }
                Image::load_from_svg_data(&bytes)
                    .map_err(|_| format!("{}: isn't an SVG Yapfast draws", choice.file.display()))?
            }
        };
        svgs.insert(key, image.clone());
        match &choice.colour {
            IconColour::Tint => {}
            IconColour::Keep => painted.push((image.clone(), None)),
            IconColour::Token(token) => painted.push((image.clone(), colour(token))),
            IconColour::Colour(argb) => {
                painted.push((image.clone(), Some(Color::from_argb_encoded(*argb))))
            }
        }
        icons.push((index, image, choice.colour.clone()));
    }
    STATE.with_borrow_mut(|state| {
        if !icons.is_empty() || !state.icons.is_empty() {
            state.loads += 1;
        }
        state.sounds = sounds;
        state.icons = icons;
        state.painted = painted;
        state.decoded = decoded;
        state.svgs = svgs;
    });
    changed();
    Ok(())
}

/// Sounds decoded off the UI thread, by their file's [`digest`], for the
/// next [`load`] to take instead of decoding them again.
pub fn seed_sounds(decoded: HashMap<u64, Arc<[i16]>>) {
    STATE.with_borrow_mut(|state| state.decoded.extend(decoded));
}

/// Drops the loaded theme's sounds and icons; the window should be
/// [`hide`]n first.
pub fn forget() {
    STATE.with_borrow_mut(|state| {
        state.sounds.clear();
        state.icons.clear();
        state.painted.clear();
        state.decoded.clear();
        state.svgs.clear();
    });
    changed();
}

/// Shows the loaded icons and arms the loaded sounds.
pub fn show(window: &YapfastWindow) {
    let pack = window.global::<IconPack>();
    STATE.with_borrow_mut(|state| {
        state.shown = true;
        if !state.icons.is_empty() {
            if state.stock.is_empty() {
                state.stock = read_stock(&pack);
            }
            for (index, image, _) in &state.icons {
                set(&pack, *index, image.clone());
            }
            state.swapped = true;
        }
        pack.set_active(!state.painted.is_empty());
        pack.set_revision(pack.get_revision().wrapping_add(1));
    });
    changed();
    arm(window);
}

/// Puts back every stock icon and silences the theme.
pub fn hide(window: &YapfastWindow) {
    let pack = window.global::<IconPack>();
    STATE.with_borrow_mut(|state| {
        state.shown = false;
        if state.swapped {
            for (index, image) in state.stock.iter().enumerate() {
                set(&pack, index, image.clone());
            }
            state.swapped = false;
        }
        pack.set_active(false);
    });
    changed();
    arm(window);
}

/// Tells the controls which events have a sound, and the ringer what to ring
/// with, from what's loaded, shown and chosen.
fn arm(window: &YapfastWindow) {
    let cue = window.global::<Cue>();
    STATE.with_borrow(|state| {
        let playing = state.shown && state.prefs.on;
        let interface = state.shown && state.prefs.interface && state.prefs.volume > 0;
        let has = |slots: &[SoundSlot]| {
            interface && slots.iter().any(|slot| state.sounds.contains_key(slot))
        };
        cue.set_hover(has(&[SoundSlot::Hover]));
        cue.set_press(has(&[SoundSlot::Press]));
        cue.set_menu(has(&[SoundSlot::MenuOpen, SoundSlot::MenuClose]));
        cue.set_popup(has(&[SoundSlot::PopupOpen, SoundSlot::PopupClose]));
        cue.set_toggle(has(&[SoundSlot::ToggleOn, SoundSlot::ToggleOff]));
        for (slot, tone) in [
            (SoundSlot::Ring, yapfast_core::ring::Tone::Incoming),
            (SoundSlot::Ringback, yapfast_core::ring::Tone::Outgoing),
        ] {
            let samples = playing
                .then(|| state.sounds.get(&slot))
                .flatten()
                .and_then(|takes| takes.takes.first().cloned());
            yapfast_core::ring::set_themed(tone, samples);
        }
    });
}

/// The person's choices for the shown theme.
pub fn set_prefs(window: &YapfastWindow, prefs: Prefs) {
    let changed =
        STATE.with_borrow_mut(|state| std::mem::replace(&mut state.prefs, prefs) != prefs);
    if changed {
        arm(window);
    }
}

/// Whether the shown theme has sounds, and interface sounds.
pub fn has_sounds() -> (bool, bool) {
    STATE.with_borrow(|state| {
        (
            !state.sounds.is_empty(),
            state.sounds.keys().any(|slot| slot.interface()),
        )
    })
}

/// Whether the shown theme replaces any of Yapfast's own sounds.
pub fn has_notification_sounds() -> bool {
    STATE.with_borrow(|state| state.sounds.keys().any(|slot| !slot.interface()))
}

/// Keeps what the window plays on, while a theme has sounds.
pub fn set_context(context: Context) {
    STATE.with_borrow_mut(|state| {
        if state.context != context {
            state.context = context;
        }
    });
}

/// Changes whenever the icons the window shows do, for what Rust draws
/// itself, such as the channels' icons.
pub fn generation() -> u64 {
    STATE.with_borrow(|state| {
        if state.shown && !state.icons.is_empty() {
            state.loads
        } else {
            0
        }
    })
}

/// The icon a theme shows for `name`, or the stock one in `stock`.
pub fn icon(name: &str, stock: &[u8]) -> anyhow::Result<Image> {
    let themed = STATE.with_borrow(|state| {
        let index = NAMES.iter().position(|known| *known == name)?;
        state
            .shown
            .then(|| state.icons.iter().find(|(at, _, _)| *at == index))
            .flatten()
            .map(|(_, image, _)| image.clone())
    });
    match themed {
        Some(image) => Ok(image),
        None => Ok(Image::load_from_svg_data(stock)?),
    }
}

/// `IconPack.paint`: the colour to draw `source` with where its place would
/// use `stock`.
pub fn paint(source: &Image, stock: Brush) -> Brush {
    STATE.with_borrow(
        |state| match state.painted.iter().find(|(image, _)| image == source) {
            Some((_, Some(colour))) => Brush::SolidColor(*colour),
            Some((_, None)) => Brush::SolidColor(Color::from_argb_encoded(0)),
            None => stock,
        },
    )
}

/// One of the takes of a slot, at random among several.
fn take(state: &mut State, slot: SoundSlot) -> Option<(Arc<[i16]>, f32)> {
    let takes = state.sounds.get(&slot)?;
    let count = takes.takes.len() as u32;
    // A cheap shuffle, so the same take rarely plays twice running.
    state.picked = state
        .picked
        .wrapping_mul(1_103_515_245)
        .wrapping_add(12_345);
    let index = (state.picked >> 16) % count.max(1);
    Some((Arc::clone(&takes.takes[index as usize]), takes.volume))
}

/// The theme's sound in place of Yapfast's `sound`, with its own volume,
/// while the theme shows and its sounds are on.
pub fn replacing(sound: Sound) -> Option<(Arc<[i16]>, f32)> {
    STATE.with_borrow_mut(|state| {
        if !state.shown || !state.prefs.on {
            return None;
        }
        take(state, SoundSlot::of_sound(sound))
    })
}

/// The quickest hover sounds follow one another.
const HOVER_GAP: Duration = Duration::from_millis(70);

/// Plays an interface event's sound, if the theme has one, its sounds and
/// interface sounds are on, the window is in front and the person isn't in
/// Do Not Disturb.
pub fn cue(slot: SoundSlot, focused: bool) {
    if !slot.interface() || !focused {
        return;
    }
    let playing = STATE.with_borrow_mut(|state| {
        if !state.shown
            || !state.prefs.interface
            || state.context.quiet
            || !state.sounds.contains_key(&slot)
        {
            return None;
        }
        if slot == SoundSlot::Hover {
            let now = Instant::now();
            if state
                .last_hover
                .is_some_and(|last| now.duration_since(last) < HOVER_GAP)
            {
                return None;
            }
            state.last_hover = Some(now);
        }
        let volume = state.context.gain * yapfast_core::sounds::gain(state.prefs.volume.min(100));
        let (samples, own) = take(state, slot)?;
        Some((samples, volume * own, state.context.speaker.clone()))
    });
    if let Some((samples, volume, speaker)) = playing {
        yapfast_core::sounds::play_decoded(samples, volume, speaker.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pack_has_every_icon_the_window_has() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/icons");
        let mut files: Vec<String> = std::fs::read_dir(folder)
            .unwrap()
            .filter_map(|entry| {
                let name = entry.unwrap().file_name().into_string().unwrap();
                name.strip_suffix(".svg").map(str::to_owned)
            })
            .collect();
        files.sort();
        let mut pack: Vec<&str> = NAMES.to_vec();
        pack.sort();
        let mut themes: Vec<&str> = media::ICONS.to_vec();
        themes.sort();
        assert_eq!(pack, files);
        assert_eq!(themes, files);
    }
}
