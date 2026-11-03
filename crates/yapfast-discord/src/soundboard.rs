//! Soundboard sounds: Discord's own, which every account may play, and each
//! server's. Playing one asks Discord to send VOICE_CHANNEL_EFFECT_SEND to
//! everyone in the call, and each client downloads the sound from the CDN
//! and plays it itself; nothing goes through the voice connection.
//! See <https://docs.discord.food/resources/soundboard>.

use base64::Engine;
use serde_json::Value;

use crate::{
    ready::cdn,
    settings::{Field, fields, message, put_bytes, put_field, put_fixed32, put_fixed64},
};

/// `PreloadedUserSettings.voice_and_video`, and in it `soundboard_settings`,
/// whose `volume` is a float from 0 to 100: the slider's value on the
/// power curve of [`crate`]'s other volumes, 100 unless set.
const VOICE_AND_VIDEO: u64 = 5;
const SOUNDBOARD_SETTINGS: u64 = 9;
const VOLUME: u64 = 1;
/// `PreloadedUserSettings.guilds`, an `AllGuildSettings`, whose `guilds`
/// map each server's id, or 0 for every server, to its `GuildSettings`;
/// their `join_sound` is a `CustomCallSound` of a sound and its server, 0
/// for Discord's own.
const GUILDS: u64 = 3;
const ALL_GUILDS: u64 = 1;
const KEY: u64 = 1;
const VALUE: u64 = 2;
const JOIN_SOUND: u64 = 6;
const JOIN_SOUND_ID: u64 = 1;
const JOIN_SOUND_GUILD: u64 = 2;
/// `FrecencyUserSettings.favorite_soundboard_sounds`: the favourites, and
/// the same in the order the picker shows them.
const FAVORITE_SOUNDS: u64 = 8;
const SOUND_IDS: u64 = 1;
const ORDERED_SOUND_IDS: u64 = 2;
/// Discord's client keeps no more favourites than this.
pub const MOST_FAVORITES: usize = 250;
/// The soundboard's volume unless the settings say.
pub const DEFAULT_VOLUME: f32 = 100.0;

/// A soundboard sound, as Discord describes one.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub id: String,
    pub name: String,
    /// The server it belongs to; none for Discord's own.
    pub guild: Option<String>,
    /// How loud it plays, from 0 to 1, as whoever added it chose.
    pub volume: f32,
    /// Its custom emoji's id, or its Unicode emoji in `emoji_name`.
    pub emoji_id: Option<String>,
    pub emoji_name: Option<String>,
    /// False while the server lacks the boosts its slot needs.
    pub available: bool,
}

/// A sound object from Discord, or none without an id or a name. Discord's
/// own sounds come without a server, or with "0".
pub fn parse(sound: &Value) -> Option<Sound> {
    let text = |key: &str| {
        sound[key]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    };
    Some(Sound {
        id: text("sound_id")?,
        name: text("name")?,
        guild: text("guild_id").filter(|guild| guild != "0"),
        volume: sound["volume"].as_f64().unwrap_or(1.0).clamp(0.0, 1.0) as f32,
        emoji_id: text("emoji_id"),
        emoji_name: text("emoji_name"),
        available: sound["available"].as_bool().unwrap_or(true),
    })
}

/// A list of sound objects, leaving out any [`parse`] doesn't take.
pub fn parse_all(sounds: &Value) -> Vec<Sound> {
    sounds
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse)
        .collect()
}

/// Where the sound's file is: MP3 or Ogg, at most 512 KiB.
pub fn file(id: &str) -> String {
    format!("{}/soundboard-sounds/{id}", cdn())
}

/// Someone played a sound in a call, from VOICE_CHANNEL_EFFECT_SEND; none
/// for an effect without a sound, such as an emoji alone.
#[derive(Clone, Debug, PartialEq)]
pub struct Played {
    pub channel: String,
    pub user: String,
    pub sound: String,
    /// The sound's own volume, from 0 to 1.
    pub volume: f32,
}

pub fn played(effect: &Value) -> Option<Played> {
    Some(Played {
        channel: effect["channel_id"].as_str()?.to_owned(),
        user: effect["user_id"].as_str()?.to_owned(),
        sound: effect["sound_id"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| effect["sound_id"].as_u64().map(|id| id.to_string()))?,
        volume: effect["sound_volume"]
            .as_f64()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0) as f32,
    })
}

/// A sound that plays as this account joins a voice channel, with Nitro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinSound {
    /// The server it's for; none for every server without one of its own.
    pub server: Option<String>,
    pub sound: String,
    /// The sound's server; none for Discord's own.
    pub guild: Option<String>,
}

/// What `PreloadedUserSettings` say of the soundboard, where they say:
/// a settings update carries only the top-level fields it changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    pub volume: Option<f32>,
    pub join_sounds: Option<Vec<JoinSound>>,
}

fn decode(encoded: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap_or_default()
}

fn encode(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// The id a settings proto keeps as `fixed64`: 0 for none.
fn id(value: u64) -> Option<String> {
    (value != 0).then(|| value.to_string())
}

/// The soundboard's settings in base64 `PreloadedUserSettings`, as READY's
/// `user_settings_proto` or a USER_SETTINGS_PROTO_UPDATE carries them.
pub fn read_settings(encoded: &str) -> Settings {
    let data = decode(encoded);
    // Discord's client takes a `soundboard_settings` without a volume as 0,
    // as proto3 leaves a 0 out, and none at all as 100.
    let volume = message(&data, VOICE_AND_VIDEO).map(|voice| {
        message(voice, SOUNDBOARD_SETTINGS).map_or(DEFAULT_VOLUME, |soundboard| {
            fields(soundboard)
                .find_map(|(number, value)| match value {
                    Field::Fixed32(bits) if number == VOLUME => Some(f32::from_bits(bits)),
                    _ => None,
                })
                .unwrap_or(0.0)
                .clamp(0.0, 100.0)
        })
    });
    let join_sounds = message(&data, GUILDS).map(|guilds| {
        fields(guilds)
            .filter_map(|(number, value)| match value {
                Field::Bytes(entry) if number == ALL_GUILDS => Some(entry),
                _ => None,
            })
            .filter_map(|entry| {
                let (mut server, mut settings) = (None, None);
                for (number, value) in fields(entry) {
                    match (number, value) {
                        (KEY, Field::Fixed64(key)) => server = Some(key),
                        (VALUE, Field::Bytes(value)) => settings = Some(value),
                        _ => {}
                    }
                }
                let sound = message(settings?, JOIN_SOUND)?;
                let (mut id_of, mut guild) = (0, 0);
                for (number, value) in fields(sound) {
                    match (number, value) {
                        (JOIN_SOUND_ID, Field::Fixed64(value)) => id_of = value,
                        (JOIN_SOUND_GUILD, Field::Fixed64(value)) => guild = value,
                        _ => {}
                    }
                }
                Some(JoinSound {
                    server: id(server.unwrap_or(0)),
                    sound: id(id_of)?,
                    guild: id(guild),
                })
            })
            .collect()
    });
    Settings {
        volume,
        join_sounds,
    }
}

/// `message` with its field `number` taken out, the rest as it was.
fn without(message: &[u8], number: u64) -> Vec<u8> {
    let mut kept = Vec::with_capacity(message.len());
    for (field, value) in fields(message) {
        if field != number {
            put_field(&mut kept, field, &value);
        }
    }
    kept
}

/// Base64 settings holding the whole of `voice_and_video` from `current`,
/// the account's settings, with the soundboard's volume set to `volume`. A
/// PATCH puts each top-level field it carries in place of the old one, so
/// the rest of it has to come along.
pub fn write_volume(current: &str, volume: f32) -> String {
    let data = decode(current);
    let voice = message(&data, VOICE_AND_VIDEO).unwrap_or_default();
    let soundboard = message(voice, SOUNDBOARD_SETTINGS).unwrap_or_default();
    let mut soundboard = without(soundboard, VOLUME);
    put_fixed32(&mut soundboard, VOLUME, volume.clamp(0.0, 100.0).to_bits());
    let mut voice = without(voice, SOUNDBOARD_SETTINGS);
    put_bytes(&mut voice, SOUNDBOARD_SETTINGS, &soundboard);
    let mut settings = Vec::new();
    put_bytes(&mut settings, VOICE_AND_VIDEO, &voice);
    encode(&settings)
}

/// Base64 settings holding the whole of `guilds` from `current`, with the
/// join sound of `server`, or of every server with none, set to `sound`,
/// its id and its own server's, none for Discord's own, or taken away;
/// the rest of each server's settings as they were.
pub fn write_join_sound(
    current: &str,
    server: Option<&str>,
    sound: Option<(&str, Option<&str>)>,
) -> String {
    let data = decode(current);
    let guilds = message(&data, GUILDS).unwrap_or_default();
    let key: u64 = server.and_then(|server| server.parse().ok()).unwrap_or(0);
    let join_sound = |settings: &[u8]| {
        let mut settings = without(settings, JOIN_SOUND);
        if let Some((id, guild)) = sound {
            let mut chosen = Vec::new();
            put_fixed64(&mut chosen, JOIN_SOUND_ID, id.parse().unwrap_or(0));
            let guild = guild.and_then(|guild| guild.parse().ok());
            put_fixed64(&mut chosen, JOIN_SOUND_GUILD, guild.unwrap_or(0));
            put_bytes(&mut settings, JOIN_SOUND, &chosen);
        }
        settings
    };
    let entry = |settings: &[u8]| {
        let mut entry = Vec::new();
        put_fixed64(&mut entry, KEY, key);
        put_bytes(&mut entry, VALUE, settings);
        entry
    };
    let mut written = Vec::with_capacity(guilds.len() + 32);
    let mut found = false;
    for (number, value) in fields(guilds) {
        let Field::Bytes(old) = value else {
            put_field(&mut written, number, &value);
            continue;
        };
        let of_server = number == ALL_GUILDS
            && fields(old).any(|(field, value)| {
                field == KEY && matches!(value, Field::Fixed64(at) if at == key)
            });
        if !of_server {
            put_field(&mut written, number, &value);
            continue;
        }
        found = true;
        let settings = message(old, VALUE).unwrap_or_default();
        put_bytes(&mut written, ALL_GUILDS, &entry(&join_sound(settings)));
    }
    if !found {
        put_bytes(&mut written, ALL_GUILDS, &entry(&join_sound(&[])));
    }
    let mut settings = Vec::new();
    put_bytes(&mut settings, GUILDS, &written);
    encode(&settings)
}

/// The favourite sounds in base64 `FrecencyUserSettings`, in the order the
/// picker shows them; `None` when the settings don't say.
pub fn read_favorites(encoded: &str) -> Option<Vec<String>> {
    let data = decode(encoded);
    let favorites = message(&data, FAVORITE_SOUNDS)?;
    let (mut ids, mut ordered) = (Vec::new(), Vec::new());
    for (number, value) in fields(favorites) {
        let list = match number {
            SOUND_IDS => &mut ids,
            ORDERED_SOUND_IDS => &mut ordered,
            _ => continue,
        };
        match value {
            Field::Fixed64(value) => list.push(value),
            Field::Bytes(packed) => list.extend(
                packed
                    .chunks_exact(8)
                    .map(|bytes| u64::from_le_bytes(bytes.try_into().expect("eight bytes"))),
            ),
            _ => {}
        }
    }
    // The order, then any favourite it leaves out, as an older client
    // kept only the set.
    for id in ids {
        if !ordered.contains(&id) {
            ordered.push(id);
        }
    }
    Some(ordered.into_iter().filter_map(id).collect())
}

/// Base64 `FrecencyUserSettings` holding only the favourite sounds, in
/// order, which a PATCH puts in place of the old.
pub fn write_favorites(favorites: &[String]) -> String {
    let packed: Vec<u8> = favorites
        .iter()
        .filter_map(|id| id.parse::<u64>().ok())
        .flat_map(u64::to_le_bytes)
        .collect();
    let mut message = Vec::new();
    put_bytes(&mut message, SOUND_IDS, &packed);
    put_bytes(&mut message, ORDERED_SOUND_IDS, &packed);
    let mut settings = Vec::new();
    put_bytes(&mut settings, FAVORITE_SOUNDS, &message);
    encode(&settings)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_discords_and_servers_sounds() {
        let sounds = parse_all(&json!([
            {"name": "quack", "sound_id": "1", "volume": 1.0, "emoji_id": null,
             "emoji_name": "🦆", "override_path": null, "user_id": "0", "available": true},
            {"name": "Airhorn", "sound_id": "9", "volume": 0.5, "emoji_id": "77",
             "emoji_name": "horn", "guild_id": "10", "available": false},
            {"name": "", "sound_id": "11"},
        ]));
        assert_eq!(
            sounds,
            [
                Sound {
                    id: "1".into(),
                    name: "quack".into(),
                    guild: None,
                    volume: 1.0,
                    emoji_id: None,
                    emoji_name: Some("🦆".into()),
                    available: true,
                },
                Sound {
                    id: "9".into(),
                    name: "Airhorn".into(),
                    guild: Some("10".into()),
                    volume: 0.5,
                    emoji_id: Some("77".into()),
                    emoji_name: Some("horn".into()),
                    available: false,
                },
            ]
        );
    }

    /// Settings with the custom status, a server's own settings with a
    /// channel's in them, and the AFK timeout beside the soundboard's.
    fn settings() -> String {
        let mut channel = Vec::new();
        put_bytes(&mut channel, 1, &[7, 7]);
        let mut server = Vec::new();
        put_bytes(&mut server, 1, &channel);
        let mut entry = Vec::new();
        put_fixed64(&mut entry, KEY, 10);
        put_bytes(&mut entry, VALUE, &server);
        let mut guilds = Vec::new();
        put_bytes(&mut guilds, ALL_GUILDS, &entry);
        let mut afk = Vec::new();
        crate::settings::put_varint(&mut afk, 1 << 3);
        crate::settings::put_varint(&mut afk, 600);
        let mut voice = Vec::new();
        put_bytes(&mut voice, 6, &afk);
        let mut data = decode(&crate::settings::write_status("idle"));
        put_bytes(&mut data, GUILDS, &guilds);
        put_bytes(&mut data, VOICE_AND_VIDEO, &voice);
        encode(&data)
    }

    #[test]
    fn writes_the_volume_and_join_sounds_keeping_the_rest_of_their_fields() {
        let current = settings();
        assert_eq!(
            read_settings(&current),
            Settings {
                volume: Some(DEFAULT_VOLUME),
                join_sounds: Some(vec![]),
            }
        );
        assert_eq!(
            read_settings(&crate::settings::write_status("idle")),
            Settings::default()
        );

        let volume = write_volume(&current, 42.5);
        assert_eq!(read_settings(&volume).volume, Some(42.5));
        assert_eq!(crate::settings::read(&volume).afk_timeout, Some(600));
        // Only that top-level field goes.
        assert_eq!(crate::settings::read(&volume).status, None);

        let joined = write_join_sound(&current, Some("10"), Some(("9", Some("10"))));
        let joined = write_join_sound(&joined, None, Some(("1", None)));
        assert_eq!(
            read_settings(&joined).join_sounds.unwrap(),
            [
                JoinSound {
                    server: Some("10".into()),
                    sound: "9".into(),
                    guild: Some("10".into()),
                },
                JoinSound {
                    server: None,
                    sound: "1".into(),
                    guild: None,
                },
            ]
        );
        // The server's own settings stay.
        let data = decode(&joined);
        let entry = fields(message(&data, GUILDS).unwrap())
            .find_map(|(_, value)| match value {
                Field::Bytes(entry) => Some(entry),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            message(message(message(entry, VALUE).unwrap(), 1).unwrap(), 1),
            Some(&[7u8, 7][..])
        );
        let cleared = write_join_sound(&joined, Some("10"), None);
        assert_eq!(read_settings(&cleared).join_sounds.unwrap().len(), 1);
    }

    #[test]
    fn reads_and_writes_favourites_in_order() {
        let favorites = vec!["9".to_owned(), "1".to_owned()];
        assert_eq!(
            read_favorites(&write_favorites(&favorites)),
            Some(favorites)
        );
        assert_eq!(read_favorites(&crate::settings::write_status("idle")), None);
        // A favourite the order leaves out comes after it.
        let mut message = Vec::new();
        put_fixed64(&mut message, SOUND_IDS, 3);
        put_fixed64(&mut message, SOUND_IDS, 4);
        put_fixed64(&mut message, ORDERED_SOUND_IDS, 4);
        let mut settings = Vec::new();
        put_bytes(&mut settings, FAVORITE_SOUNDS, &message);
        assert_eq!(
            read_favorites(&encode(&settings)),
            Some(vec!["4".into(), "3".into()])
        );
    }

    #[test]
    fn reads_a_played_sound_and_skips_an_emoji_alone() {
        let effect = json!({"channel_id": "5", "guild_id": "10", "user_id": "2",
            "sound_id": "9", "sound_volume": 0.5, "emoji": null, "animation_type": null});
        assert_eq!(
            played(&effect),
            Some(Played {
                channel: "5".into(),
                user: "2".into(),
                sound: "9".into(),
                volume: 0.5,
            })
        );
        let emoji = json!({"channel_id": "5", "user_id": "2", "emoji": {"name": "🔥"},
            "animation_type": 0, "animation_id": 3});
        assert_eq!(played(&emoji), None);
    }
}
