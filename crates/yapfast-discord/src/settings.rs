//! The account's status as Discord's user settings keep it: online, idle, do not
//! disturb or invisible, and a custom status; and its server list's order and
//! folders.
//! The settings are a protobuf message, `PreloadedUserSettings`, in base64;
//! Yapfast reads its `status` and `guild_folders` fields and writes only `status`,
//! by hand. `status` also says whether the account shares the game it plays.
//!
//! Each session tells the gateway its presence, so Yapfast sends the status the
//! settings hold once connected, and again when they change.

use base64::Engine;
use serde_json::{Value, json};

/// `PreloadedUserSettings.status`.
const STATUS: u64 = 11;
/// Voice and video settings, and in them the AFK timeout.
const VOICE_AND_VIDEO: u64 = 5;
const VOICE_AFK_TIMEOUT: u64 = 6;
/// `StatusSettings.status`, a `StringValue`, and `StatusSettings.custom_status`.
const STATUS_STATUS: u64 = 1;
const STATUS_CUSTOM: u64 = 2;
/// `StatusSettings.show_current_game`, a `BoolValue`: whether the account
/// shares the game it plays and the Rich Presence games send.
const STATUS_SHOW_GAME: u64 = 3;
/// `CustomStatus`'s fields.
const CUSTOM_TEXT: u64 = 1;
const CUSTOM_EMOJI_ID: u64 = 2;
const CUSTOM_EMOJI_NAME: u64 = 3;
const CUSTOM_EXPIRES: u64 = 4;
/// `PreloadedUserSettings.guild_folders`, a `GuildFolders`: its `folders`, in
/// the order the server list shows them, and `guild_positions`, the order
/// older clients kept.
const GUILD_FOLDERS: u64 = 14;
const FOLDERS: u64 = 1;
const GUILD_POSITIONS: u64 = 2;
/// `GuildFolder.guild_ids`: one server outside a folder, or a folder's.
const FOLDER_GUILDS: u64 = 1;
/// `GuildFolder`'s `id`, an `Int64Value` that only a folder has; its `name`,
/// a `StringValue`; and its `color`, a `UInt64Value` holding RGB.
const FOLDER_ID: u64 = 2;
const FOLDER_NAME: u64 = 3;
const FOLDER_COLOR: u64 = 4;

pub(crate) const VARINT: u64 = 0;
const FIXED64: u64 = 1;
const BYTES: u64 = 2;
const FIXED32: u64 = 5;

/// A custom status: its text, its emoji, and when it ends.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CustomStatus {
    pub text: String,
    /// A Unicode emoji, or a custom emoji's name.
    pub emoji_name: Option<String>,
    /// A custom emoji's id.
    pub emoji_id: Option<String>,
    /// Unix milliseconds when it clears itself; none when it stays.
    pub expires_at_ms: Option<u64>,
}

impl CustomStatus {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.emoji_name.is_none() && self.emoji_id.is_none()
    }

    /// Whether it has ended by `now_ms`.
    pub fn expired(&self, now_ms: u64) -> bool {
        self.expires_at_ms.is_some_and(|expires| expires <= now_ms)
    }
}

/// What the settings say of the status. `None` where they don't say: a
/// partial update leaves the rest as it was.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusSettings {
    /// `online`, `idle`, `dnd` or `invisible`.
    pub status: Option<String>,
    /// The custom status, `Some(None)` when it's cleared.
    pub custom: Option<Option<CustomStatus>>,
    /// How long, in seconds, nobody uses a client before it tells Discord
    /// it's away from the keyboard.
    pub afk_timeout: Option<u64>,
    /// Whether the account shares the game it plays.
    pub show_game: Option<bool>,
}

/// The status in base64 settings, as READY's `user_settings_proto` or a
/// USER_SETTINGS_PROTO_UPDATE carries them.
pub fn read(encoded: &str) -> StatusSettings {
    let Ok(data) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
        return StatusSettings::default();
    };
    // A UInt32Value: the number in its field 1.
    let afk_timeout = message(&data, VOICE_AND_VIDEO)
        .and_then(|voice| message(voice, VOICE_AFK_TIMEOUT))
        .map(|wrapper| {
            fields(wrapper)
                .find_map(|(number, value)| match value {
                    Field::Varint(seconds) if number == 1 => Some(seconds),
                    _ => None,
                })
                .unwrap_or(0)
        });
    let mut settings = StatusSettings {
        afk_timeout,
        ..StatusSettings::default()
    };
    let Some(status) = message(&data, STATUS) else {
        return settings;
    };
    for (number, value) in fields(status) {
        match (number, value) {
            (STATUS_STATUS, Field::Bytes(wrapper)) => {
                settings.status = fields(wrapper).find_map(|(number, value)| match value {
                    Field::Bytes(text) if number == 1 => {
                        Some(String::from_utf8_lossy(text).into_owned())
                    }
                    _ => None,
                });
            }
            (STATUS_CUSTOM, Field::Bytes(custom)) => {
                let mut status = CustomStatus::default();
                for (number, value) in fields(custom) {
                    match (number, value) {
                        (CUSTOM_TEXT, Field::Bytes(text)) => {
                            status.text = String::from_utf8_lossy(text).into_owned();
                        }
                        (CUSTOM_EMOJI_ID, Field::Fixed64(id)) if id != 0 => {
                            status.emoji_id = Some(id.to_string());
                        }
                        (CUSTOM_EMOJI_NAME, Field::Bytes(name)) if !name.is_empty() => {
                            status.emoji_name = Some(String::from_utf8_lossy(name).into_owned());
                        }
                        (CUSTOM_EXPIRES, Field::Fixed64(at)) if at != 0 => {
                            status.expires_at_ms = Some(at);
                        }
                        _ => {}
                    }
                }
                settings.custom = Some((!status.is_empty()).then_some(status));
            }
            // A BoolValue leaves out a false value.
            (STATUS_SHOW_GAME, Field::Bytes(wrapper)) => {
                settings.show_game = Some(fields(wrapper).any(|(number, value)| {
                    number == 1 && matches!(value, Field::Varint(shown) if shown != 0)
                }));
            }
            _ => {}
        }
    }
    settings
}

/// The last field `number` of `data` that holds a message.
pub(crate) fn message(data: &[u8], number: u64) -> Option<&[u8]> {
    fields(data)
        .filter_map(|(field, value)| match value {
            Field::Bytes(bytes) if field == number => Some(bytes),
            _ => None,
        })
        .last()
}

/// A folder in the account's server list.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Folder {
    /// Discord's id for it.
    pub id: String,
    /// Its name; none when it has none, and the rail names it by its servers.
    pub name: Option<String>,
    /// Its colour as RGB; none for Discord's default.
    pub color: Option<u32>,
    /// Its servers' ids, in order.
    pub servers: Vec<String>,
}

/// The account's server list, as its settings keep it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerList {
    /// Every server it places, in order, each folder's in its place.
    pub order: Vec<String>,
    pub folders: Vec<Folder>,
}

/// The account's server list. `None` when the settings don't say: a partial
/// update leaves it as it was.
pub fn server_list(encoded: &str) -> Option<ServerList> {
    let data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let folders = fields(&data).find_map(|(number, value)| match value {
        Field::Bytes(bytes) if number == GUILD_FOLDERS => Some(bytes),
        _ => None,
    })?;
    let mut list = ServerList::default();
    let mut positions = Vec::new();
    for (number, value) in fields(folders) {
        match (number, value) {
            (FOLDERS, Field::Bytes(entry)) => {
                let mut folder = Folder::default();
                let mut id = None;
                for (number, value) in fields(entry) {
                    match (number, value) {
                        (FOLDER_GUILDS, value) => folder
                            .servers
                            .extend(fixed64s(value).into_iter().map(|id| id.to_string())),
                        (FOLDER_ID, Field::Bytes(wrapper)) => id = Some(wrapped_varint(wrapper)),
                        (FOLDER_NAME, Field::Bytes(wrapper)) => {
                            folder.name = fields(wrapper)
                                .find_map(|(number, value)| match value {
                                    Field::Bytes(text) if number == 1 => {
                                        Some(String::from_utf8_lossy(text).into_owned())
                                    }
                                    _ => None,
                                })
                                .filter(|name| !name.trim().is_empty());
                        }
                        (FOLDER_COLOR, Field::Bytes(wrapper)) => {
                            folder.color = u32::try_from(wrapped_varint(wrapper)).ok();
                        }
                        _ => {}
                    }
                }
                list.order.extend(folder.servers.iter().cloned());
                // An entry without an id is one server outside any folder.
                if let Some(id) = id
                    && !folder.servers.is_empty()
                {
                    folder.id = id.to_string();
                    list.folders.push(folder);
                }
            }
            (GUILD_POSITIONS, value) => positions.extend(fixed64s(value)),
            _ => {}
        }
    }
    if list.order.is_empty() {
        list.order = positions.into_iter().map(|id| id.to_string()).collect();
    }
    Some(list)
}

/// The value of a wrapper such as `Int64Value`, whose field 1 is a varint;
/// 0, protobuf's default, when it's left out.
fn wrapped_varint(wrapper: &[u8]) -> u64 {
    fields(wrapper)
        .find_map(|(number, value)| match value {
            Field::Varint(value) if number == 1 => Some(value),
            _ => None,
        })
        .unwrap_or_default()
}

/// Base64 settings holding only a server order: each folder's servers, a
/// server outside a folder as a folder of one.
#[cfg(test)]
pub(crate) fn write_server_order(folders: &[&[u64]]) -> String {
    let entries: Vec<_> = folders
        .iter()
        .map(|servers| Folder {
            servers: servers.iter().map(u64::to_string).collect(),
            ..Folder::default()
        })
        .collect();
    write_server_list(&entries)
}

/// Base64 settings holding only a server list. An entry with an empty `id`
/// is a server outside any folder.
#[cfg(test)]
pub(crate) fn write_server_list(entries: &[Folder]) -> String {
    let mut message = Vec::new();
    for entry in entries {
        let packed: Vec<u8> = entry
            .servers
            .iter()
            .flat_map(|id| id.parse::<u64>().unwrap().to_le_bytes())
            .collect();
        let mut folder = Vec::new();
        put_bytes(&mut folder, FOLDER_GUILDS, &packed);
        if let Ok(id) = entry.id.parse::<u64>() {
            let mut wrapper = Vec::new();
            put_varint(&mut wrapper, 1 << 3 | VARINT);
            put_varint(&mut wrapper, id);
            put_bytes(&mut folder, FOLDER_ID, &wrapper);
        }
        if let Some(name) = &entry.name {
            let mut wrapper = Vec::new();
            put_bytes(&mut wrapper, 1, name.as_bytes());
            put_bytes(&mut folder, FOLDER_NAME, &wrapper);
        }
        if let Some(color) = entry.color {
            let mut wrapper = Vec::new();
            put_varint(&mut wrapper, 1 << 3 | VARINT);
            put_varint(&mut wrapper, u64::from(color));
            put_bytes(&mut folder, FOLDER_COLOR, &wrapper);
        }
        put_bytes(&mut message, FOLDERS, &folder);
    }
    let mut settings = Vec::new();
    put_bytes(&mut settings, GUILD_FOLDERS, &message);
    base64::engine::general_purpose::STANDARD.encode(settings)
}

/// The values of a repeated `fixed64` field: packed, or one by one.
fn fixed64s(value: Field<'_>) -> Vec<u64> {
    match value {
        Field::Fixed64(value) => vec![value],
        Field::Bytes(packed) => packed
            .chunks_exact(8)
            .map(|bytes| u64::from_le_bytes(bytes.try_into().expect("eight bytes")))
            .collect(),
        _ => Vec::new(),
    }
}

/// Base64 settings that set the custom status, or clear it with `None`.
pub fn write_custom(custom: Option<&CustomStatus>) -> String {
    let mut message = Vec::new();
    if let Some(custom) = custom {
        put_bytes(&mut message, CUSTOM_TEXT, custom.text.as_bytes());
        if let Some(id) = custom
            .emoji_id
            .as_deref()
            .and_then(|id| id.parse::<u64>().ok())
        {
            put_fixed64(&mut message, CUSTOM_EMOJI_ID, id);
        }
        if let Some(name) = &custom.emoji_name {
            put_bytes(&mut message, CUSTOM_EMOJI_NAME, name.as_bytes());
        }
        if let Some(at) = custom.expires_at_ms {
            put_fixed64(&mut message, CUSTOM_EXPIRES, at);
        }
    }
    let mut status = Vec::new();
    put_bytes(&mut status, STATUS_CUSTOM, &message);
    let mut settings = Vec::new();
    put_bytes(&mut settings, STATUS, &status);
    base64::engine::general_purpose::STANDARD.encode(settings)
}

/// Base64 settings that set the status: `online`, `idle`, `dnd` or `invisible`.
pub fn write_status(status: &str) -> String {
    let mut wrapper = Vec::new();
    put_bytes(&mut wrapper, 1, status.as_bytes());
    let mut inner = Vec::new();
    put_bytes(&mut inner, STATUS_STATUS, &wrapper);
    let mut settings = Vec::new();
    put_bytes(&mut settings, STATUS, &inner);
    base64::engine::general_purpose::STANDARD.encode(settings)
}

/// Base64 settings that share the game the account plays, or stop sharing it.
pub fn write_show_game(shown: bool) -> String {
    let mut wrapper = Vec::new();
    if shown {
        put_varint(&mut wrapper, 1 << 3 | VARINT);
        put_varint(&mut wrapper, 1);
    }
    let mut inner = Vec::new();
    put_bytes(&mut inner, STATUS_SHOW_GAME, &wrapper);
    let mut settings = Vec::new();
    put_bytes(&mut settings, STATUS, &inner);
    base64::engine::general_purpose::STANDARD.encode(settings)
}

/// The presence a session sends the gateway, in IDENTIFY or opcode 3: the
/// status, and the custom status as an activity unless it has ended.
/// `away` marks the session AFK, so Discord sends this account's
/// notifications to its phone, and may show an online status as idle.
pub fn presence(status: &str, custom: Option<&CustomStatus>, now_ms: u64, away: Away) -> Value {
    let activities: Vec<Value> = custom
        .filter(|custom| !custom.is_empty() && !custom.expired(now_ms))
        .map(|custom| {
            let mut activity = json!({"type": 4, "name": "Custom Status"});
            if !custom.text.is_empty() {
                activity["state"] = json!(custom.text);
            }
            if custom.emoji_name.is_some() || custom.emoji_id.is_some() {
                activity["emoji"] = json!({
                    "name": custom.emoji_name,
                    "id": custom.emoji_id,
                    "animated": false,
                });
            }
            activity
        })
        .into_iter()
        .collect();
    let status = match away.idle_since {
        Some(_) if status == "online" => "idle",
        _ => status,
    };
    json!({
        "status": status,
        "since": away.idle_since.unwrap_or(0),
        "activities": activities,
        "afk": away.afk,
    })
}

/// Nobody has used Yapfast for a while: as the web client decides it, from
/// [`Away::after`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Away {
    /// Away from the keyboard: Discord sends notifications to the phone.
    pub afk: bool,
    /// Shown idle since then, in Unix milliseconds.
    pub idle_since: Option<u64>,
}

/// The web client shows its account idle after 10 minutes without input.
const IDLE_AFTER_MS: u64 = 10 * 60 * 1000;
/// The AFK timeout the settings hold when they say none, in seconds.
pub const AFK_TIMEOUT: u64 = 60;

impl Away {
    /// How a session is away when nobody has used Yapfast since
    /// `inactive_since`, in Unix milliseconds: AFK after the account's AFK
    /// timeout, at most 10 minutes, and idle after 10 minutes unless
    /// `show_idle` is off. Also returns when that next changes.
    pub fn after(
        inactive_since: Option<u64>,
        afk_timeout: u64,
        show_idle: bool,
        now_ms: u64,
    ) -> (Self, Option<u64>) {
        let Some(since) = inactive_since else {
            return (Self::default(), None);
        };
        let afk_at = since + afk_timeout.saturating_mul(1000).min(IDLE_AFTER_MS);
        let idle_at = since + IDLE_AFTER_MS;
        let away = Self {
            afk: now_ms >= afk_at,
            idle_since: (show_idle && now_ms >= idle_at).then_some(since),
        };
        let next = [afk_at, idle_at]
            .into_iter()
            .filter(|&at| at > now_ms && (show_idle || at == afk_at))
            .min();
        (away, next)
    }
}

pub(crate) enum Field<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
}

/// A protobuf message's fields, as far as they parse.
pub(crate) fn fields(mut data: &[u8]) -> impl Iterator<Item = (u64, Field<'_>)> {
    std::iter::from_fn(move || {
        let key = varint(&mut data)?;
        let value = match key & 7 {
            VARINT => Field::Varint(varint(&mut data)?),
            FIXED64 => {
                let (bytes, rest) = data.split_first_chunk::<8>()?;
                data = rest;
                Field::Fixed64(u64::from_le_bytes(*bytes))
            }
            BYTES => {
                let length = usize::try_from(varint(&mut data)?).ok()?;
                let bytes = data.get(..length)?;
                data = &data[length..];
                Field::Bytes(bytes)
            }
            FIXED32 => {
                let (bytes, rest) = data.split_first_chunk::<4>()?;
                data = rest;
                Field::Fixed32(u32::from_le_bytes(*bytes))
            }
            _ => return None,
        };
        Some((key >> 3, value))
    })
}

pub(crate) fn varint(data: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = data.split_first()?;
        *data = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

pub(crate) fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

pub(crate) fn put_bytes(out: &mut Vec<u8>, number: u64, bytes: &[u8]) {
    put_varint(out, number << 3 | BYTES);
    put_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

pub(crate) fn put_fixed64(out: &mut Vec<u8>, number: u64, value: u64) {
    put_varint(out, number << 3 | FIXED64);
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_fixed32(out: &mut Vec<u8>, number: u64, value: u32) {
    put_varint(out, number << 3 | FIXED32);
    out.extend_from_slice(&value.to_le_bytes());
}

/// Writes a field as [`fields`] read it, to keep it as it was.
pub(crate) fn put_field(out: &mut Vec<u8>, number: u64, field: &Field<'_>) {
    match *field {
        Field::Varint(value) => {
            put_varint(out, number << 3 | VARINT);
            put_varint(out, value);
        }
        Field::Fixed64(value) => put_fixed64(out, number, value),
        Field::Bytes(bytes) => put_bytes(out, number, bytes),
        Field::Fixed32(value) => put_fixed32(out, number, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_server_list_with_its_folders() {
        let folder = |id: &str, name: Option<&str>, color, servers: &[&str]| Folder {
            id: id.into(),
            name: name.map(Into::into),
            color,
            servers: servers.iter().map(|&id| id.into()).collect(),
        };
        let written = write_server_list(&[
            folder("", None, None, &["30"]),
            folder("7", Some("Games"), Some(0x5865f2), &["10", "40"]),
            folder("8", Some(" "), None, &["20"]),
        ]);
        let list = server_list(&written).unwrap();
        assert_eq!(list.order, ["30", "10", "40", "20"]);
        assert_eq!(
            list.folders,
            [
                folder("7", Some("Games"), Some(0x5865f2), &["10", "40"]),
                // A blank name counts as none; a folder of one is still a folder.
                folder("8", None, None, &["20"]),
            ]
        );
        // Settings that leave the list out don't empty it.
        assert_eq!(server_list(&write_status("idle")), None);
    }

    #[test]
    fn reads_and_writes_the_custom_status() {
        let status = CustomStatus {
            text: "Fixing the bridge".into(),
            emoji_name: Some("pog".into()),
            emoji_id: Some("1400000000000000300".into()),
            expires_at_ms: Some(1_791_300_000_000),
        };
        let written = write_custom(Some(&status));
        assert_eq!(
            read(&written),
            StatusSettings {
                status: None,
                custom: Some(Some(status.clone())),
                afk_timeout: None,
                show_game: None,
            }
        );
        assert_eq!(read(&write_custom(None)).custom, Some(None));
        assert_eq!(
            read(&write_status("idle")),
            StatusSettings {
                status: Some("idle".into()),
                custom: None,
                afk_timeout: None,
                show_game: None,
            },
            "setting the status leaves the custom status as it was"
        );
        // Other settings around it, and a status, are read past and read.
        let mut settings = Vec::new();
        put_bytes(&mut settings, 1, b"\x08\x01");
        put_varint(&mut settings, 6 << 3 | VARINT);
        put_varint(&mut settings, 300);
        let mut inner = Vec::new();
        let mut wrapper = Vec::new();
        put_bytes(&mut wrapper, 1, b"dnd");
        put_bytes(&mut inner, STATUS_STATUS, &wrapper);
        put_fixed64(&mut inner, 4, 9);
        put_bytes(&mut settings, STATUS, &inner);
        let read = read(&base64::engine::general_purpose::STANDARD.encode(settings));
        assert_eq!(read.status.as_deref(), Some("dnd"));
        assert_eq!(read.custom, None, "a partial update leaves it as it was");
        assert_eq!(super::read("not base64!"), StatusSettings::default());
    }

    #[test]
    fn reads_and_writes_whether_the_game_shows() {
        assert_eq!(read(&write_show_game(false)).show_game, Some(false));
        assert_eq!(read(&write_show_game(true)).show_game, Some(true));
        assert_eq!(read(&write_status("idle")).show_game, None);
    }

    #[test]
    fn reads_the_server_order_with_folders_in_place() {
        let packed = |ids: &[u64]| {
            ids.iter()
                .flat_map(|id| id.to_le_bytes())
                .collect::<Vec<_>>()
        };
        let mut folders = Vec::new();
        // A server on its own, a folder of two, then one written unpacked.
        let mut folder = Vec::new();
        put_bytes(&mut folder, FOLDER_GUILDS, &packed(&[30]));
        put_bytes(&mut folders, FOLDERS, &folder);
        let mut folder = Vec::new();
        put_bytes(&mut folder, FOLDER_GUILDS, &packed(&[10, 20]));
        put_bytes(&mut folder, 3, b"\x0a\x05Games");
        put_bytes(&mut folders, FOLDERS, &folder);
        let mut folder = Vec::new();
        put_fixed64(&mut folder, FOLDER_GUILDS, 5);
        put_bytes(&mut folders, FOLDERS, &folder);
        put_bytes(&mut folders, GUILD_POSITIONS, &packed(&[1, 2]));
        let mut settings = Vec::new();
        put_bytes(&mut settings, STATUS, b"");
        put_bytes(&mut settings, GUILD_FOLDERS, &folders);
        let encoded = base64::engine::general_purpose::STANDARD.encode(&settings);
        assert_eq!(
            server_list(&encoded).unwrap().order,
            ["30", "10", "20", "5"]
        );

        // Older settings keep only the flat order.
        let mut folders = Vec::new();
        put_bytes(&mut folders, GUILD_POSITIONS, &packed(&[2, 1]));
        let mut settings = Vec::new();
        put_bytes(&mut settings, GUILD_FOLDERS, &folders);
        let encoded = base64::engine::general_purpose::STANDARD.encode(&settings);
        assert_eq!(server_list(&encoded).unwrap().order, ["2", "1"]);
        assert_eq!(server_list(&write_custom(None)), None);
    }

    #[test]
    fn presence_carries_the_custom_status_until_it_ends() {
        let status = CustomStatus {
            text: "Away".into(),
            emoji_name: Some("🌙".into()),
            emoji_id: None,
            expires_at_ms: Some(2_000),
        };
        let shown = presence("idle", Some(&status), 1_000, Away::default());
        assert_eq!(shown["status"], "idle");
        assert_eq!(
            shown["activities"],
            json!([{"type": 4, "name": "Custom Status", "state": "Away",
                "emoji": {"name": "🌙", "id": null, "animated": false}}])
        );
        assert_eq!(
            presence("online", Some(&status), 2_000, Away::default())["activities"],
            json!([])
        );
    }

    #[test]
    fn a_session_nobody_uses_goes_afk_then_idle_as_the_web_client_does() {
        const MINUTE: u64 = 60_000;
        let since = 1_000_000;
        let at = |minutes: u64, show_idle| {
            Away::after(Some(since), 60, show_idle, since + minutes * MINUTE)
        };
        assert_eq!(Away::after(None, 60, true, since), (Away::default(), None));
        assert_eq!(at(0, true), (Away::default(), Some(since + MINUTE)));
        let afk = Away {
            afk: true,
            idle_since: None,
        };
        assert_eq!(at(1, true), (afk, Some(since + 10 * MINUTE)));
        let idle = Away {
            afk: true,
            idle_since: Some(since),
        };
        assert_eq!(at(10, true), (idle, None));
        assert_eq!(at(10, false), (afk, None), "never idle when that's off");
        // A long AFK timeout still goes AFK at 10 minutes.
        assert_eq!(
            Away::after(Some(since), 3600, true, since + 10 * MINUTE).0,
            idle
        );

        let shown = presence("online", None, 0, idle);
        assert_eq!(
            (&shown["status"], &shown["since"], &shown["afk"]),
            (&json!("idle"), &json!(since), &json!(true))
        );
        let shown = presence("online", None, 0, afk);
        assert_eq!(
            (&shown["status"], &shown["since"]),
            (&json!("online"), &json!(0))
        );
        assert_eq!(presence("dnd", None, 0, idle)["status"], "dnd");
    }

    #[test]
    fn reads_the_afk_timeout() {
        // voice_and_video { afk_timeout { value: 300 } }
        let mut wrapper = Vec::new();
        wrapper.extend([1 << 3, 0xac, 0x02]);
        let mut voice = Vec::new();
        put_bytes(&mut voice, VOICE_AFK_TIMEOUT, &wrapper);
        let mut settings = Vec::new();
        put_bytes(&mut settings, VOICE_AND_VIDEO, &voice);
        let encoded = base64::engine::general_purpose::STANDARD.encode(settings);
        assert_eq!(read(&encoded).afk_timeout, Some(300));
        assert_eq!(read(&write_status("dnd")).afk_timeout, None);
    }
}
