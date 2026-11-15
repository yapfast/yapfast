//! A theme's sounds and icons: `"sounds"` replaces Yapfast's own sounds by
//! name and adds sounds to a closed list of interface events; `"icons"`
//! replaces any of the window's icons by name. Both are files in the theme's
//! folder, checked before anything plays or draws them: a sound is decoded
//! whole, under limits, and an icon is an SVG with nothing in it that runs or
//! reaches outside the file (see [`super::check::check_svg`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::package::{COLOURS, EXTRA_COLOURS, Error, parse_colour, relative};

macro_rules! bail {
    ($($arg:tt)*) => { return Err(Error(format!($($arg)*))) };
}

/// Where a theme's sound plays: one of Yapfast's own sounds, which it
/// replaces, or an interface event, which only themes give a sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SoundSlot {
    Join,
    Leave,
    Mute,
    Unmute,
    Deafen,
    Undeafen,
    Message,
    Mention,
    ViewerJoin,
    ViewerLeave,
    StreamStart,
    StreamStop,
    /// A DM call ringing this account, repeated until answered.
    Ring,
    /// Ringing someone else, repeated until they answer.
    Ringback,
    /// The pointer comes onto a button.
    Hover,
    /// A button is clicked.
    Press,
    /// A context menu opens, and closes.
    MenuOpen,
    MenuClose,
    /// A popup or dialog opens, and closes.
    PopupOpen,
    PopupClose,
    /// A switch turns on, and off.
    ToggleOn,
    ToggleOff,
    /// This account sends a message.
    Send,
    /// This account adds a reaction.
    ReactionAdd,
    /// The theme is applied, or its trial starts.
    ThemeApplied,
}

impl SoundSlot {
    pub const ALL: [Self; 25] = [
        Self::Join,
        Self::Leave,
        Self::Mute,
        Self::Unmute,
        Self::Deafen,
        Self::Undeafen,
        Self::Message,
        Self::Mention,
        Self::ViewerJoin,
        Self::ViewerLeave,
        Self::StreamStart,
        Self::StreamStop,
        Self::Ring,
        Self::Ringback,
        Self::Hover,
        Self::Press,
        Self::MenuOpen,
        Self::MenuClose,
        Self::PopupOpen,
        Self::PopupClose,
        Self::ToggleOn,
        Self::ToggleOff,
        Self::Send,
        Self::ReactionAdd,
        Self::ThemeApplied,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Join => "join",
            Self::Leave => "leave",
            Self::Mute => "mute",
            Self::Unmute => "unmute",
            Self::Deafen => "deafen",
            Self::Undeafen => "undeafen",
            Self::Message => "message",
            Self::Mention => "mention",
            Self::ViewerJoin => "viewer-join",
            Self::ViewerLeave => "viewer-leave",
            Self::StreamStart => "stream-start",
            Self::StreamStop => "stream-stop",
            Self::Ring => "ring",
            Self::Ringback => "ringback",
            Self::Hover => "hover",
            Self::Press => "press",
            Self::MenuOpen => "menu-open",
            Self::MenuClose => "menu-close",
            Self::PopupOpen => "popup-open",
            Self::PopupClose => "popup-close",
            Self::ToggleOn => "toggle-on",
            Self::ToggleOff => "toggle-off",
            Self::Send => "send",
            Self::ReactionAdd => "reaction-add",
            Self::ThemeApplied => "theme-applied",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.name() == name)
    }

    /// The interface's events, which only a theme gives a sound: they play
    /// at the theme's own volume, and only while the window is in front.
    pub fn interface(self) -> bool {
        self.index() >= Self::Hover.index()
    }

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|slot| *slot == self)
            .expect("listed")
    }

    /// The Yapfast sound it replaces, if it's one.
    pub fn app_sound(self) -> Option<crate::sounds::Sound> {
        use crate::sounds::Sound;
        Some(match self {
            Self::Join => Sound::Join,
            Self::Leave => Sound::Leave,
            Self::Mute => Sound::Mute,
            Self::Unmute => Sound::Unmute,
            Self::Deafen => Sound::Deafen,
            Self::Undeafen => Sound::Undeafen,
            Self::Message => Sound::Message,
            Self::Mention => Sound::Mention,
            Self::ViewerJoin => Sound::ViewerJoin,
            Self::ViewerLeave => Sound::ViewerLeave,
            Self::StreamStart => Sound::StreamStart,
            Self::StreamStop => Sound::StreamStop,
            _ => return None,
        })
    }

    /// The slot that replaces a Yapfast sound.
    pub fn of_sound(sound: crate::sounds::Sound) -> Self {
        Self::ALL
            .into_iter()
            .find(|slot| slot.app_sound() == Some(sound))
            .expect("every sound has a slot")
    }

    /// The longest a sound in this slot may last, in seconds: an interface
    /// event's is a blip, a ring repeats.
    pub fn longest(self) -> f32 {
        match self {
            Self::Ring | Self::Ringback => 6.0,
            _ if self.interface() => 1.5,
            _ => 4.0,
        }
    }
}

/// A slot's sound: one file, or several that take turns at random, at a
/// volume against the others.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundChoice {
    pub files: Vec<PathBuf>,
    /// 0 to 1, where 1 is the file as it is.
    pub volume: f32,
}

/// The most files a slot picks among.
pub const MAX_SOUND_FILES: usize = 8;

/// The kinds of sound file a theme ships, by extension.
pub const SOUND_EXTENSIONS: [&str; 4] = ["wav", "ogg", "opus", "mp3"];

/// The largest sound file.
pub const MAX_SOUND_BYTES: u64 = 1 << 20;

/// What a replaced icon is painted with.
#[derive(Clone, Debug, PartialEq)]
pub enum IconColour {
    /// As the stock icon is where it's drawn: one colour, the place's own.
    Tint,
    /// Its own colours, as the SVG draws them.
    Keep,
    /// One of the theme's colours, by its name.
    Token(&'static str),
    /// One colour, ARGB.
    Colour(u32),
}

/// A replaced icon.
#[derive(Clone, Debug, PartialEq)]
pub struct IconChoice {
    pub file: PathBuf,
    pub colour: IconColour,
}

/// The icons a theme may replace, by their name in `ui/icons/` without
/// `.svg`; the interface checks this list against its own.
pub const ICONS: [&str; 135] = [
    "arrow-left",
    "arrow-right",
    "at-sign",
    "ban",
    "bell-off",
    "bell",
    "brush",
    "calendar",
    "chart-bar",
    "check",
    "chevron-down",
    "chevron-left",
    "chevron-right",
    "chevrons-right",
    "circle-plus",
    "circle-stop",
    "clipboard-paste",
    "clock",
    "community",
    "copy-plus",
    "copy",
    "crown",
    "discord",
    "download",
    "ellipsis",
    "external-link",
    "eye-off",
    "eye",
    "eyedropper",
    "file",
    "folder-plus",
    "folder",
    "forward",
    "gamepad",
    "gavel",
    "gem",
    "gif",
    "github",
    "hand",
    "hash-lock",
    "hash-warning",
    "hash",
    "headphone-off",
    "headphones",
    "help-circle",
    "history",
    "id-filled",
    "image-lock",
    "image-warning",
    "image",
    "inbox",
    "link",
    "lock",
    "log-out",
    "logo",
    "maximize",
    "megaphone-lock",
    "megaphone-warning",
    "megaphone",
    "message-circle",
    "message-square-text-lock",
    "message-square-text-warning",
    "message-square-text",
    "mic-off",
    "mic",
    "minimize",
    "minus",
    "moon",
    "music",
    "noise-bars-off",
    "noise-bars",
    "palette",
    "pause",
    "pen",
    "pencil",
    "phone-off",
    "phone",
    "pin-off",
    "pin",
    "play",
    "plus",
    "refresh-cw",
    "reply",
    "restore",
    "rules",
    "scissors",
    "screen-share-off",
    "screen-share",
    "scroll-down",
    "scroll-left",
    "scroll-right",
    "scroll-up",
    "search",
    "send-horizontal",
    "settings",
    "shield",
    "shop",
    "signal-high",
    "signal-low",
    "signal-medium",
    "slash",
    "smile-plus",
    "smile",
    "soundboard",
    "square-pen",
    "square",
    "stage-lock",
    "stage",
    "star-filled",
    "star",
    "sticker",
    "sticky-note",
    "sun",
    "text-select",
    "thread",
    "trash-2",
    "trending-up",
    "triangle-alert",
    "trophy",
    "tv",
    "user-minus",
    "user-pen",
    "user-plus",
    "user-x",
    "user",
    "users",
    "verified",
    "video-off",
    "video",
    "volume-2-bold",
    "volume-2-lock",
    "volume-2-warning",
    "volume-2",
    "volume-x",
    "x",
];

/// The icon of that name, as the static name Yapfast knows it by.
pub fn icon_named(name: &str) -> Option<&'static str> {
    ICONS.iter().copied().find(|icon| *icon == name)
}

/// Whether a file is a sound, by its extension.
pub fn is_sound(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SOUND_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

fn sound_file(value: &Value, place: &str) -> Result<PathBuf, Error> {
    let text = value
        .as_str()
        .ok_or_else(|| Error(format!("\"{place}\" names a sound file in the theme")))?;
    let path = relative(text, place)?;
    if !is_sound(&path) {
        bail!("\"{place}\" names \"{text}\": sounds are .wav, .ogg, .opus or .mp3 files");
    }
    Ok(path)
}

/// Reads `"sounds"`: each slot a file, a list of files, or
/// `{ "file" | "files", "volume" }`. `null` takes a slot's sound away again,
/// as a variant may.
pub(super) fn read_sounds(
    value: &Value,
    place: &str,
    sounds: &mut BTreeMap<SoundSlot, SoundChoice>,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    let Value::Object(slots) = value else {
        bail!("\"{place}\" has to be an object");
    };
    for (name, written) in slots {
        let at = format!("{place}.{name}");
        let Some(slot) = SoundSlot::named(name) else {
            notes.push(format!(
                "\"{at}\" isn't a sound or an event Yapfast plays; ignored"
            ));
            continue;
        };
        let files_at = |value: &Value, here: &str| -> Result<Vec<PathBuf>, Error> {
            match value {
                Value::Array(items) => {
                    if items.is_empty() || items.len() > MAX_SOUND_FILES {
                        bail!("\"{here}\" lists 1 to {MAX_SOUND_FILES} sound files");
                    }
                    items
                        .iter()
                        .enumerate()
                        .map(|(index, item)| sound_file(item, &format!("{here}[{index}]")))
                        .collect()
                }
                value => Ok(vec![sound_file(value, here)?]),
            }
        };
        let choice =
            match written {
                Value::Null => {
                    sounds.remove(&slot);
                    continue;
                }
                Value::Object(fields) => {
                    let mut files = None;
                    let mut volume = 1.0;
                    for (key, value) in fields {
                        let here = format!("{at}.{key}");
                        match key.as_str() {
                            "file" | "files" => files = Some(files_at(value, &here)?),
                            "volume" => {
                                let number = value.as_f64().ok_or_else(|| {
                                    Error(format!("\"{here}\" has to be a number"))
                                })? as f32;
                                if !(0.0..=1.0).contains(&number) {
                                    bail!("\"{here}\" is {number}, outside 0 to 1");
                                }
                                volume = number;
                            }
                            _ => notes
                                .push(format!("\"{here}\" isn't something a sound takes; ignored")),
                        }
                    }
                    SoundChoice {
                        files: files.ok_or_else(|| Error(format!("\"{at}\" needs a \"file\"")))?,
                        volume,
                    }
                }
                value => SoundChoice {
                    files: files_at(value, &at)?,
                    volume: 1.0,
                },
            };
        sounds.insert(slot, choice);
    }
    Ok(())
}

/// Reads `"icons"`: each icon a file, or `{ "file", "colour" }` where the
/// colour is `tint` (the place's own, as stock), `keep` (the SVG's own), a
/// colour's name such as `accent-text`, or `#rrggbb`. `null` gives a variant
/// the stock icon back.
pub(super) fn read_icons(
    value: &Value,
    place: &str,
    icons: &mut BTreeMap<&'static str, IconChoice>,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    let Value::Object(entries) = value else {
        bail!("\"{place}\" has to be an object");
    };
    for (name, written) in entries {
        let at = format!("{place}.{name}");
        let Some(icon) = icon_named(name) else {
            notes.push(format!("\"{at}\" isn't one of Yapfast's icons; ignored"));
            continue;
        };
        let file = |value: &Value, here: &str| -> Result<PathBuf, Error> {
            let text = value
                .as_str()
                .ok_or_else(|| Error(format!("\"{here}\" names an SVG file in the theme")))?;
            let path = relative(text, here)?;
            let svg = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"));
            if !svg {
                bail!("\"{here}\" names \"{text}\": icons are .svg files");
            }
            Ok(path)
        };
        let choice = match written {
            Value::Null => {
                icons.remove(icon);
                continue;
            }
            Value::Object(fields) => {
                let mut path = None;
                let mut colour = IconColour::Tint;
                for (key, value) in fields {
                    let here = format!("{at}.{key}");
                    match key.as_str() {
                        "file" => path = Some(file(value, &here)?),
                        "colour" | "color" => {
                            let text = value.as_str().unwrap_or_default();
                            colour = match text {
                                "tint" => IconColour::Tint,
                                "keep" => IconColour::Keep,
                                _ => {
                                    if let Some(token) = COLOURS
                                        .iter()
                                        .chain(EXTRA_COLOURS.iter())
                                        .copied()
                                        .find(|token| *token == text)
                                    {
                                        IconColour::Token(token)
                                    } else if let Some(argb) = parse_colour(text) {
                                        IconColour::Colour(argb)
                                    } else {
                                        bail!(
                                            "\"{here}\" is tint, keep, a colour's name such as accent-text, or a colour such as #ff71ce"
                                        );
                                    }
                                }
                            };
                        }
                        _ => {
                            notes.push(format!("\"{here}\" isn't something an icon takes; ignored"))
                        }
                    }
                }
                IconChoice {
                    file: path.ok_or_else(|| Error(format!("\"{at}\" needs a \"file\"")))?,
                    colour,
                }
            }
            value => IconChoice {
                file: file(value, &at)?,
                colour: IconColour::Tint,
            },
        };
        icons.insert(icon, choice);
    }
    Ok(())
}

/// The loudest a theme's sound plays before any volume: Yapfast's own
/// sounds' peak, 12 dB under full scale, so a theme can't be louder than
/// they are. A quieter file stays as quiet.
pub const PEAK: f32 = 0.25;

/// A sound file decoded whole, as 48 kHz interleaved stereo, lowered to
/// [`PEAK`] if it's louder. Refused if it isn't a sound Yapfast decodes,
/// is silent, or lasts longer than 6 seconds. The decoder runs inside a
/// guard that refuses a file it panics on.
pub fn decode_sound(path: &Path, bytes: &[u8]) -> Result<Vec<i16>, String> {
    if bytes.len() as u64 > MAX_SOUND_BYTES {
        return Err("a sound is at most 1 MiB".into());
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let decoded = std::panic::catch_unwind(|| match extension.as_str() {
        "wav" => wav(bytes),
        #[cfg(feature = "discord")]
        "ogg" | "opus" | "mp3" => crate::soundboard::decode(bytes)
            .map_err(|_| "isn't a sound Yapfast can play".to_owned()),
        #[cfg(not(feature = "discord"))]
        "ogg" | "opus" | "mp3" => Err("this Yapfast plays only .wav sounds".to_owned()),
        _ => Err("isn't a sound: use .wav, .ogg, .opus or .mp3".to_owned()),
    });
    let mut samples = match decoded {
        Ok(result) => result?,
        Err(_) => return Err("isn't a sound Yapfast can play".into()),
    };
    if samples.len() > (LONGEST * RATE as f32) as usize * 2 {
        return Err(format!("lasts longer than {LONGEST} seconds"));
    }
    let peak = samples
        .iter()
        .map(|sample| sample.unsigned_abs())
        .max()
        .unwrap_or(0);
    if peak < 16 {
        return Err("is silent".into());
    }
    let most = (PEAK * f32::from(i16::MAX)) as u16;
    if peak > most {
        let gain = f32::from(most) / f32::from(peak);
        for sample in &mut samples {
            *sample = (f32::from(*sample) * gain) as i16;
        }
    }
    samples.shrink_to_fit();
    Ok(samples)
}

/// How long decoded samples last, in seconds.
pub fn seconds(samples: &[i16]) -> f32 {
    samples.len() as f32 / 2.0 / RATE as f32
}

const RATE: u32 = crate::audio_io::RATE;

/// The longest any theme sound lasts, a ring's.
const LONGEST: f32 = 6.0;

/// A RIFF WAVE file of PCM (8, 16, 24 or 32 bits) or 32-bit float samples,
/// mono or stereo, at 8 to 96 kHz, as 48 kHz stereo. Read by hand, bounded
/// by the file's own length: no chunk is trusted past the bytes there are.
fn wav(bytes: &[u8]) -> Result<Vec<i16>, String> {
    let refused = || "isn't a WAV file Yapfast reads".to_owned();
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(refused());
    }
    let u16_at = |at: usize| -> Option<u16> {
        Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
    };
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let mut at = 12;
    let mut format = None;
    let mut data = None;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(at + 4).ok_or_else(refused)? as usize;
        let start = at + 8;
        let end = start
            .checked_add(size)
            .ok_or_else(refused)?
            .min(bytes.len());
        match id {
            b"fmt " => {
                let tag = u16_at(start).ok_or_else(refused)?;
                let channels = u16_at(start + 2).ok_or_else(refused)?;
                let rate = u32_at(start + 4).ok_or_else(refused)?;
                let bits = u16_at(start + 14).ok_or_else(refused)?;
                // WAVE_FORMAT_EXTENSIBLE names its real format in its GUID.
                let tag = if tag == 0xfffe {
                    u16_at(start + 24).ok_or_else(refused)?
                } else {
                    tag
                };
                format = Some((tag, channels, rate, bits));
            }
            b"data" => data = Some(&bytes[start..end]),
            _ => {}
        }
        at = start.saturating_add(size + (size & 1));
    }
    let (tag, channels, rate, bits) = format.ok_or_else(refused)?;
    let data = data.ok_or_else(refused)?;
    if !(1..=2).contains(&channels) || !(8_000..=96_000).contains(&rate) {
        return Err("a WAV sound is mono or stereo, at 8 to 96 kHz".into());
    }
    let width = match (tag, bits) {
        (1, 8) => 1,
        (1, 16) => 2,
        (1, 24) => 3,
        (1, 32) | (3, 32) => 4,
        _ => return Err("a WAV sound is 8, 16, 24 or 32-bit PCM, or 32-bit float".into()),
    };
    let frame = width * usize::from(channels);
    let frames = data.len() / frame;
    if frames > (LONGEST * rate as f32) as usize + rate as usize {
        return Err(format!("lasts longer than {LONGEST} seconds"));
    }
    let sample = |bytes: &[u8]| -> f32 {
        match (tag, width) {
            (_, 1) => (f32::from(bytes[0]) - 128.0) / 128.0,
            (_, 2) => f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.0,
            (_, 3) => {
                (i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8) as f32 / 8_388_608.0
            }
            (3, _) => {
                let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                if value.is_finite() { value } else { 0.0 }
            }
            _ => {
                i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32
                    / 2_147_483_648.0
            }
        }
    };
    let stereo: Vec<[f32; 2]> = data
        .chunks_exact(frame)
        .map(|frame| {
            let left = sample(&frame[..width]);
            let right = if channels == 2 {
                sample(&frame[width..])
            } else {
                left
            };
            [left, right]
        })
        .collect();
    // To 48 kHz between neighbouring frames: enough for short blips.
    let step = f64::from(rate) / f64::from(RATE);
    let out_frames = (stereo.len() as f64 / step) as usize;
    let mut out = Vec::with_capacity(out_frames * 2);
    for index in 0..out_frames {
        let position = index as f64 * step;
        let first = position as usize;
        let between = (position - first as f64) as f32;
        let a = stereo[first.min(stereo.len() - 1)];
        let b = stereo[(first + 1).min(stereo.len() - 1)];
        for (from, to) in a.iter().zip(b) {
            let value = from * (1.0 - between) + to * between;
            out.push((value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16);
        }
    }
    if out.is_empty() {
        return Err("the sound is empty".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mono 16-bit WAV of `seconds` of a sine at `rate`.
    pub(crate) fn sine_wav(rate: u32, seconds: f32, amplitude: f32) -> Vec<u8> {
        let frames = (rate as f32 * seconds) as usize;
        let mut data = Vec::with_capacity(frames * 2);
        for index in 0..frames {
            let value = (std::f32::consts::TAU * 660.0 * index as f32 / rate as f32).sin();
            data.extend_from_slice(&((value * amplitude * 32767.0) as i16).to_le_bytes());
        }
        let mut file = Vec::new();
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&rate.to_le_bytes());
        file.extend_from_slice(&(rate * 2).to_le_bytes());
        file.extend_from_slice(&2u16.to_le_bytes());
        file.extend_from_slice(&16u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend_from_slice(&data);
        file
    }

    #[test]
    fn decodes_a_wav_to_48_khz_stereo_no_louder_than_yapfasts_own() {
        let samples = decode_sound(Path::new("a.wav"), &sine_wav(44_100, 0.25, 1.0)).unwrap();
        assert!((seconds(&samples) - 0.25).abs() < 0.01);
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak <= (PEAK * 32767.0) as u16 + 1, "peaks at {peak}");
        // A quieter sound stays as quiet.
        let quiet = decode_sound(Path::new("a.wav"), &sine_wav(48_000, 0.1, 0.1)).unwrap();
        let peak = quiet.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!((3000..3400).contains(&peak), "peaks at {peak}");
    }

    #[test]
    fn refuses_what_isnt_a_short_sound() {
        for (bytes, why) in [
            (b"RIFF\0\0\0\0WAVEjunk".to_vec(), "WAV file"),
            (sine_wav(48_000, 7.0, 0.5), "longer than"),
            (sine_wav(48_000, 0.2, 0.0), "silent"),
            (vec![0; 2 << 20], "at most 1 MiB"),
        ] {
            let error = decode_sound(Path::new("a.wav"), &bytes).unwrap_err();
            assert!(error.contains(why), "{error}");
        }
        // A chunk that says it's larger than the file reads only what's there.
        let mut lying = sine_wav(48_000, 0.1, 0.5);
        let at = lying.len() - 9600 - 4;
        lying[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_sound(Path::new("a.wav"), &lying).is_ok());
        assert!(decode_sound(Path::new("a.flac"), &lying).is_err());
    }

    #[test]
    fn reads_sounds_and_icons_by_their_closed_names() {
        let mut sounds = BTreeMap::new();
        let mut icons = BTreeMap::new();
        let mut notes = Vec::new();
        read_sounds(
            &serde_json::json!({
                "message": "sounds/drop.ogg",
                "send": ["sounds/a.wav", "sounds/b.wav"],
                "hover": { "file": "sounds/tick.wav", "volume": 0.4 },
                "sneeze": "sounds/x.wav"
            }),
            "sounds",
            &mut sounds,
            &mut notes,
        )
        .unwrap();
        assert_eq!(sounds.len(), 3);
        assert_eq!(sounds[&SoundSlot::Send].files.len(), 2);
        assert_eq!(sounds[&SoundSlot::Hover].volume, 0.4);
        read_icons(
            &serde_json::json!({
                "hash": "icons/hash.svg",
                "logo": { "file": "icons/logo.svg", "colour": "keep" },
                "pin": { "file": "icons/pin.svg", "colour": "accent-text" },
                "x": { "file": "icons/x.svg", "colour": "#ff71ce" },
                "teapot": "icons/teapot.svg"
            }),
            "icons",
            &mut icons,
            &mut notes,
        )
        .unwrap();
        assert_eq!(icons["logo"].colour, IconColour::Keep);
        assert_eq!(icons["pin"].colour, IconColour::Token("accent-text"));
        assert_eq!(icons["x"].colour, IconColour::Colour(0xffff71ce));
        assert_eq!(notes.len(), 2, "{notes:?}");
        for (bad, why) in [
            (
                serde_json::json!({ "hash": "https://x/y.svg" }),
                "never links",
            ),
            (
                serde_json::json!({ "hash": "icons/hash.png" }),
                ".svg files",
            ),
            (
                serde_json::json!({ "hash": { "file": "a.svg", "colour": "plaid" } }),
                "tint, keep",
            ),
        ] {
            let error = read_icons(&bad, "icons", &mut icons, &mut notes).unwrap_err();
            assert!(error.0.contains(why), "{error}");
        }
        for (bad, why) in [
            (
                serde_json::json!({ "send": "../a.wav" }),
                "inside the theme",
            ),
            (serde_json::json!({ "send": "a.flac" }), ".wav, .ogg"),
            (
                serde_json::json!({ "send": { "file": "a.wav", "volume": 2 } }),
                "outside 0 to 1",
            ),
            (serde_json::json!({ "send": [] }), "1 to 8"),
        ] {
            let error = read_sounds(&bad, "sounds", &mut sounds, &mut notes).unwrap_err();
            assert!(error.0.contains(why), "{error}");
        }
    }

    #[test]
    fn every_yapfast_sound_has_a_slot() {
        for sound in crate::sounds::Sound::ALL {
            assert_eq!(SoundSlot::of_sound(sound).app_sound(), Some(sound));
        }
        assert!(SoundSlot::Hover.interface() && !SoundSlot::Ringback.interface());
    }
}
