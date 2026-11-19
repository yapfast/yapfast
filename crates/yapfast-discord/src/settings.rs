//! The account's status as Discord's user settings keep it: online, idle, do not
//! disturb or invisible, and a custom status; and its server list's order and
//! folders.
//! The settings are a protobuf message, `PreloadedUserSettings`, in base64;
//! Yapfast reads its `status` and `guild_folders` fields and writes those two,
//! by hand, each always whole: Discord resets what a write of a field leaves
//! out. `status` also says whether the account shares the game it plays.
//! `guild_folders` goes as the whole server list: each folder in order, a
//! server outside any as a folder of one without an id, never the
//! deprecated `guild_positions`. It's written over the list Discord last
//! sent, so the servers this client doesn't show, and the fields Yapfast
//! doesn't read, stay as they were ([`ServerList::arranged`]).
//!
//! Each session tells the gateway its presence, so Yapfast sends the status the
//! settings hold once connected, and again when they change.

use std::collections::HashSet;
use std::sync::Arc;

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
/// `PreloadedUserSettings.text_and_images`, and in it `view_nsfw_commands`,
/// a `BoolValue`: age-restricted commands are offered in DMs.
const TEXT_AND_IMAGES: u64 = 6;
const VIEW_NSFW_COMMANDS: u64 = 23;
/// `StatusSettings.status_expires_at_ms`, a `fixed64`, and
/// `status_created_at_ms`, a `UInt64Value`: when a status set for a while
/// ends, and when it was set.
const STATUS_EXPIRES: u64 = 4;
const STATUS_CREATED: u64 = 5;
/// `CustomStatus`'s fields.
const CUSTOM_TEXT: u64 = 1;
const CUSTOM_EMOJI_ID: u64 = 2;
const CUSTOM_EMOJI_NAME: u64 = 3;
const CUSTOM_EXPIRES: u64 = 4;
/// `CustomStatus.created_at_ms`, a `fixed64`: when it was set.
const CUSTOM_CREATED: u64 = 5;
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
    /// Whether apps' age-restricted commands are offered in DMs.
    pub nsfw_commands_in_dms: Option<bool>,
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
    let nsfw_commands_in_dms = message(&data, TEXT_AND_IMAGES)
        .and_then(|text| message(text, VIEW_NSFW_COMMANDS))
        .map(bool_value);
    let mut settings = StatusSettings {
        afk_timeout,
        nsfw_commands_in_dms,
        ..StatusSettings::default()
    };
    let Some(status) = message(&data, STATUS) else {
        return settings;
    };
    // `status` comes whole: one without a custom status has none, as
    // when another device clears it or it runs out there.
    settings.custom = Some(None);
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
            (STATUS_SHOW_GAME, Field::Bytes(wrapper)) => {
                settings.show_game = Some(bool_value(wrapper));
            }
            _ => {}
        }
    }
    settings
}

/// A `BoolValue`'s value, which leaves out a false one.
fn bool_value(wrapper: &[u8]) -> bool {
    fields(wrapper)
        .any(|(number, value)| number == 1 && matches!(value, Field::Varint(on) if on != 0))
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
    /// `guild_folders` itself, its `GuildFolders`' protobuf bytes, as it
    /// came or went: what the next write starts from, holding the servers
    /// this client doesn't show and the fields Yapfast doesn't read.
    pub encoded: Arc<[u8]>,
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
    Some(ServerList::read(folders.into()))
}

impl ServerList {
    /// The list `encoded`, a `GuildFolders`' protobuf bytes, holds.
    pub fn read(encoded: Arc<[u8]>) -> Self {
        let mut list = Self::default();
        let mut positions = Vec::new();
        for (number, value) in fields(&encoded) {
            match (number, value) {
                (FOLDERS, Field::Bytes(entry)) => {
                    let (folder, _) = read_folder(entry);
                    list.order.extend(folder.servers.iter().cloned());
                    // An entry without an id is one server outside any folder.
                    if !folder.id.is_empty() && !folder.servers.is_empty() {
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
        list.encoded = encoded;
        list
    }

    /// The list a write of `entries` makes over this one, the list Discord
    /// last sent or this session last wrote. `entries` are the servers the
    /// client shows, in order, as [`write_server_list`] takes them; what
    /// they leave out of this list is a server the client doesn't know (one
    /// unavailable, not arrived yet, left on another device a moment ago, or
    /// not shown), and it stays as it was: in its folder after the server it
    /// followed there, the folder kept for it, with its name and colour, when
    /// the servers shown all left it; alone after the entry it followed. A
    /// server goes only when Discord's list stops naming it. The fields of
    /// the list and of each folder that Yapfast doesn't read stay as they
    /// came; the deprecated `guild_positions` goes, its order now in the
    /// folders.
    pub fn arranged(&self, entries: &[Folder]) -> Self {
        let mut was: Vec<(Folder, Vec<u8>)> = Vec::new();
        let mut positions = Vec::new();
        let mut own = Vec::new();
        for (number, value) in fields(&self.encoded) {
            match (number, value) {
                (FOLDERS, Field::Bytes(entry)) => was.push(read_folder(entry)),
                (GUILD_POSITIONS, value) => positions.extend(fixed64s(value)),
                (FOLDERS, _) => {}
                (number, value) => put_field(&mut own, number, &value),
            }
        }
        // Older settings keep only the flat order.
        if was.iter().all(|(folder, _)| folder.servers.is_empty()) {
            for id in positions {
                let alone = Folder {
                    servers: vec![id.to_string()],
                    ..Folder::default()
                };
                was.push((alone, Vec::new()));
            }
        }
        let shown: HashSet<&str> = entries
            .iter()
            .flat_map(|entry| entry.servers.iter().map(String::as_str))
            .collect();
        let mut list: Vec<(Folder, Vec<u8>)> = entries
            .iter()
            .filter(|entry| !entry.servers.is_empty())
            .map(|entry| (entry.clone(), kept_fields(&was, entry)))
            .collect();
        let mut placed: HashSet<&str> = HashSet::new();
        for (at, (old, rest)) in was.iter().enumerate() {
            for (index, guild) in old.servers.iter().enumerate() {
                if shown.contains(guild.as_str()) || !placed.insert(guild.as_str()) {
                    continue;
                }
                // In its folder, after the server it followed there.
                let kept = list
                    .iter_mut()
                    .find(|(folder, _)| !old.id.is_empty() && folder.id == old.id);
                if let Some((folder, _)) = kept {
                    let after = old.servers[..index]
                        .iter()
                        .rev()
                        .find_map(|before| folder.servers.iter().position(|id| id == before));
                    let slot = after.map_or(0, |before| before + 1);
                    folder.servers.insert(slot, guild.clone());
                    continue;
                }
                // Alone, or in its folder kept for it, after the entry it
                // followed.
                let place = was[..at]
                    .iter()
                    .rev()
                    .find_map(|(before, _)| place_of(&list, before))
                    .map_or(0, |before| before + 1);
                let entry = Folder {
                    id: old.id.clone(),
                    name: old.name.clone(),
                    color: old.color,
                    servers: vec![guild.clone()],
                };
                list.insert(place, (entry, rest.clone()));
            }
        }
        let mut message = Vec::new();
        for (folder, rest) in &list {
            put_folder(&mut message, folder, rest);
        }
        message.extend_from_slice(&own);
        Self::read(message.into())
    }

    /// Base64 settings holding this list alone, `guild_folders`, as a PATCH
    /// sends it.
    pub fn write(&self) -> String {
        let mut settings = Vec::new();
        put_bytes(&mut settings, GUILD_FOLDERS, &self.encoded);
        base64::engine::general_purpose::STANDARD.encode(settings)
    }
}

/// A `GuildFolder`: what Yapfast reads of it, and its other fields as they
/// came, for a write to keep them as Discord's client does.
fn read_folder(entry: &[u8]) -> (Folder, Vec<u8>) {
    let mut folder = Folder::default();
    let mut rest = Vec::new();
    for (number, value) in fields(entry) {
        match (number, value) {
            (FOLDER_GUILDS, value) => folder
                .servers
                .extend(fixed64s(value).into_iter().map(|id| id.to_string())),
            (FOLDER_ID, Field::Bytes(wrapper)) => {
                folder.id = wrapped_varint(wrapper).to_string();
            }
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
            (FOLDER_ID | FOLDER_NAME | FOLDER_COLOR, _) => {}
            (number, value) => put_field(&mut rest, number, &value),
        }
    }
    (folder, rest)
}

/// The fields Yapfast doesn't read of the entry of `was`, the list written
/// over, that `entry` is: the same folder, or the same server alone.
fn kept_fields(was: &[(Folder, Vec<u8>)], entry: &Folder) -> Vec<u8> {
    let same = |(old, _): &&(Folder, Vec<u8>)| {
        if entry.id.is_empty() {
            old.id.is_empty() && old.servers == entry.servers
        } else {
            old.id == entry.id
        }
    };
    was.iter()
        .find(same)
        .map(|(_, rest)| rest.clone())
        .unwrap_or_default()
}

/// Where in `list` the entry `old` of the list written over is now: the
/// folder by its id, else the entry holding the last of its servers that's
/// there.
fn place_of(list: &[(Folder, Vec<u8>)], old: &Folder) -> Option<usize> {
    let by_id = list.iter().position(|(folder, _)| folder.id == old.id);
    match by_id {
        Some(at) if !old.id.is_empty() => Some(at),
        _ => old.servers.iter().rev().find_map(|id| holding(list, id)),
    }
}

/// Where in `list` the entry holding `guild` is.
fn holding(list: &[(Folder, Vec<u8>)], guild: &str) -> Option<usize> {
    let holds = |(folder, _): &(Folder, Vec<u8>)| folder.servers.iter().any(|id| id == guild);
    list.iter().position(holds)
}

/// This session's writes of the server list on their way, and what a
/// refused one puts back. Each carries the whole list, so a refusal puts
/// back nothing while a later write is on its way, which settles it; the
/// last one refused puts back the list Discord last took.
#[derive(Default)]
pub(crate) struct ServerWrites {
    writing: usize,
    /// The list Discord last took, or the one shown before these writes.
    taken: ServerList,
}

impl ServerWrites {
    /// A write starts; `shown` is the list shown before it.
    pub(crate) fn start(&mut self, shown: impl FnOnce() -> ServerList) {
        if self.writing == 0 {
            self.taken = shown();
        }
        self.writing += 1;
    }

    /// The write of `list` went through, or was refused: the list to show
    /// again, if any.
    pub(crate) fn end(&mut self, list: ServerList, took: bool) -> Option<ServerList> {
        self.writing = self.writing.saturating_sub(1);
        if took {
            self.taken = list;
            None
        } else {
            (self.writing == 0).then(|| self.taken.clone())
        }
    }
}

/// An id for a folder made now: a random whole number of 32 bits, as
/// Discord's client gives a new folder, and none of `taken`.
pub fn new_folder_id(taken: &[&str]) -> String {
    loop {
        let id = rand::random_range(1..=u64::from(u32::MAX)).to_string();
        if !taken.contains(&id.as_str()) {
            return id;
        }
    }
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

/// Base64 settings holding only the server list, `guild_folders`, as a PATCH
/// sends it (`GuildFolders.folders`): each entry a folder's servers, its
/// `id`, `name` and `color`; an entry with an empty `id` is a server
/// outside any folder. A list written over none: a write of the account's
/// goes over the one Discord sent, [`ServerList::arranged`].
pub fn write_server_list(entries: &[Folder]) -> String {
    ServerList::default().arranged(entries).write()
}

/// Puts `entry` in `message`, a `GuildFolders`, as one of its `folders`:
/// its servers, and its `id`, `name` and `color` when it has them, then
/// `rest`, the fields Yapfast doesn't read, as they came. Nothing for an
/// entry without servers.
fn put_folder(message: &mut Vec<u8>, entry: &Folder, rest: &[u8]) {
    let packed: Vec<u8> = entry
        .servers
        .iter()
        .filter_map(|id| id.parse::<u64>().ok())
        .flat_map(u64::to_le_bytes)
        .collect();
    if packed.is_empty() {
        return;
    }
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
    folder.extend_from_slice(rest);
    put_bytes(message, FOLDERS, &folder);
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

/// The settings' `status` as they hold it, its bytes, when they have one:
/// what a write of it starts from.
pub fn status_field(encoded: &str) -> Option<Vec<u8>> {
    let data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    message(&data, STATUS).map(<[u8]>::to_vec)
}

/// Base64 settings holding `status` alone, as a PATCH sends them: Discord
/// puts each top-level field it carries in place of the old one, and resets
/// what that field leaves out, so `status` always goes whole.
pub fn encode_status(status: &[u8]) -> String {
    let mut settings = Vec::new();
    put_bytes(&mut settings, STATUS, status);
    base64::engine::general_purpose::STANDARD.encode(settings)
}

/// `current`, the settings' `status`, with its field `number` set to
/// `value`, or left out with `None`, and the fields in `stale` taken out;
/// the rest as it was, unknown fields too, as Discord's client keeps them.
fn with_status_field(current: &[u8], number: u64, value: Option<&[u8]>, stale: &[u64]) -> Vec<u8> {
    let mut status = Vec::new();
    for (field, kept) in fields(current) {
        if field != number && !stale.contains(&field) {
            put_field(&mut status, field, &kept);
        }
    }
    if let Some(value) = value {
        put_bytes(&mut status, number, value);
    }
    status
}

/// `current`, the settings' `status`, with the custom status set, or left
/// out to clear it with `None`, as Discord's client clears it. A new one
/// says when it was set, `now_ms`, as the client's does.
pub fn write_custom(current: &[u8], custom: Option<&CustomStatus>, now_ms: u64) -> Vec<u8> {
    let message = custom.map(|custom| {
        let mut message = Vec::new();
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
        put_fixed64(&mut message, CUSTOM_CREATED, now_ms);
        message
    });
    with_status_field(current, STATUS_CUSTOM, message.as_deref(), &[])
}

/// `current`, the settings' `status`, with the status set: `online`,
/// `idle`, `dnd` or `invisible`. As in Discord's client it stays until
/// changed, the old one's end going with it, and says when it was set,
/// `now_ms`, or keeps when the same status was.
pub fn write_status(current: &[u8], status: &str, now_ms: u64) -> Vec<u8> {
    let mut wrapper = Vec::new();
    put_bytes(&mut wrapper, 1, status.as_bytes());
    let created = fields(current).find_map(|(number, value)| match value {
        Field::Bytes(created) if number == STATUS_CREATED => Some(created.to_vec()),
        _ => None,
    });
    let same = status_of(current).as_deref() == Some(status);
    let created = match created.filter(|_| same) {
        Some(created) => created,
        None => {
            let mut created = Vec::new();
            put_varint(&mut created, 1 << 3 | VARINT);
            put_varint(&mut created, now_ms);
            created
        }
    };
    let status = with_status_field(
        current,
        STATUS_STATUS,
        Some(&wrapper),
        &[STATUS_EXPIRES, STATUS_CREATED],
    );
    with_status_field(&status, STATUS_CREATED, Some(&created), &[])
}

/// The status `status`, the settings' field, holds, if any.
fn status_of(status: &[u8]) -> Option<String> {
    fields(status).find_map(|(number, value)| match value {
        Field::Bytes(wrapper) if number == STATUS_STATUS => {
            fields(wrapper).find_map(|(number, value)| match value {
                Field::Bytes(text) if number == 1 => {
                    Some(String::from_utf8_lossy(text).into_owned())
                }
                _ => None,
            })
        }
        _ => None,
    })
}

/// `current`, the settings' `status`, sharing the game the account plays,
/// or not.
pub fn write_show_game(current: &[u8], shown: bool) -> Vec<u8> {
    let mut wrapper = Vec::new();
    if shown {
        put_varint(&mut wrapper, 1 << 3 | VARINT);
        put_varint(&mut wrapper, 1);
    }
    with_status_field(current, STATUS_SHOW_GAME, Some(&wrapper), &[])
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
        assert_eq!(server_list(&status_only("idle")), None);
    }

    #[test]
    fn writes_the_whole_server_list_as_a_patch_sends_it() {
        let entries = [
            Folder {
                servers: vec!["30".into()],
                ..Folder::default()
            },
            Folder {
                id: new_folder_id(&[]),
                name: Some("Games".into()),
                color: Some(0xed4245),
                servers: vec!["10".into(), "40".into()],
            },
        ];
        let written = write_server_list(&entries);
        let read = server_list(&written).unwrap();
        assert_eq!(read.order, ["30", "10", "40"]);
        assert_eq!(read.folders, &entries[1..]);
        assert_eq!(read.write(), written, "the list writes as it reads");
        // `guild_folders` alone, holding `folders` alone: a server alone
        // has its servers and nothing else, a folder its id, name and colour.
        let data = base64::engine::general_purpose::STANDARD
            .decode(&written)
            .unwrap();
        let top: Vec<u64> = fields(&data).map(|(number, _)| number).collect();
        assert_eq!(top, [GUILD_FOLDERS]);
        let list = message(&data, GUILD_FOLDERS).unwrap();
        let entries: Vec<Vec<u64>> = fields(list)
            .map(|(number, value)| {
                assert_eq!(number, FOLDERS, "never guild_positions");
                let Field::Bytes(entry) = value else {
                    panic!("an entry is a message")
                };
                fields(entry).map(|(number, _)| number).collect()
            })
            .collect();
        assert_eq!(entries, [vec![FOLDER_GUILDS], vec![1, 2, 3, 4]]);
        let id: u64 = new_folder_id(&[]).parse().unwrap();
        assert!((1..=u64::from(u32::MAX)).contains(&id));
    }

    #[test]
    fn a_folder_without_a_name_or_colour_leaves_both_out() {
        let folder = Folder {
            id: "7".into(),
            servers: vec!["10".into(), "20".into()],
            ..Folder::default()
        };
        let written = write_server_list(std::slice::from_ref(&folder));
        let data = base64::engine::general_purpose::STANDARD
            .decode(&written)
            .unwrap();
        let list = message(&data, GUILD_FOLDERS).unwrap();
        let entry = message(list, FOLDERS).unwrap();
        assert_eq!(
            fields(entry).map(|(number, _)| number).collect::<Vec<_>>(),
            [FOLDER_GUILDS, FOLDER_ID],
            "the name and colour are absent, as a folder without them reads"
        );
        let read = server_list(&written).unwrap();
        assert_eq!(read.folders, std::slice::from_ref(&folder));
        // Black is a colour of its own, not the default.
        let black = Folder {
            color: Some(0),
            ..folder
        };
        let read = server_list(&write_server_list(&[black])).unwrap();
        assert_eq!(read.folders[0].color, Some(0));
    }

    #[test]
    fn a_refused_server_list_puts_back_what_discord_took_once_no_write_follows() {
        let list = |guild: &str| ServerList {
            order: vec![guild.to_owned()],
            ..ServerList::default()
        };
        let mut writes = ServerWrites::default();
        // Two on their way: the first refused, the second, which carries
        // the whole list, taken: nothing goes back.
        writes.start(|| list("before"));
        writes.start(|| unreachable!("a write is on its way"));
        assert_eq!(writes.end(list("first"), false), None);
        assert_eq!(writes.end(list("second"), true), None);
        // Both refused: the list before them, once the last ends.
        writes.start(|| list("second"));
        writes.start(|| unreachable!("a write is on its way"));
        assert_eq!(writes.end(list("third"), false), None);
        assert_eq!(writes.end(list("fourth"), false), Some(list("second")));
        // The first taken, the second refused: the first's.
        writes.start(|| list("second"));
        writes.start(|| unreachable!("a write is on its way"));
        assert_eq!(writes.end(list("fifth"), true), None);
        assert_eq!(writes.end(list("sixth"), false), Some(list("fifth")));
    }

    /// A `GuildFolders` of `entries`, each with fields Yapfast doesn't read,
    /// then `own`, fields of the list's own.
    fn guild_folders(entries: &[(Folder, &[u8])], own: &[u8]) -> ServerList {
        let mut message = Vec::new();
        for (entry, rest) in entries {
            put_folder(&mut message, entry, rest);
        }
        message.extend_from_slice(own);
        ServerList::read(message.into())
    }

    /// The fields Yapfast doesn't read of `list`'s folder `id`.
    fn unread_of(list: &ServerList, id: &str) -> Vec<u8> {
        fields(&list.encoded)
            .find_map(|(number, value)| match value {
                Field::Bytes(entry) if number == FOLDERS => {
                    let (folder, rest) = read_folder(entry);
                    (folder.id == id).then_some(rest)
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_write_keeps_what_the_client_doesnt_show_where_it_was() {
        let folder = |id: &str, name: &str, servers: &[&str]| Folder {
            id: id.into(),
            name: (!name.is_empty()).then(|| name.into()),
            servers: servers.iter().map(|&id| id.into()).collect(),
            ..Folder::default()
        };
        let alone = |id: &str| folder("", "", &[id]);
        // Discord's list: 10; 50, which this client doesn't know; the folder
        // "Games" of 20, 40, which it doesn't know either, and 30, with a
        // field Yapfast doesn't read; and a field of the list's own.
        let mut unread = Vec::new();
        put_bytes(&mut unread, 9, b"kept");
        let mut own = Vec::new();
        put_bytes(&mut own, 5, b"list");
        let none: &[u8] = &[];
        let games = folder("7", "Games", &["20", "40", "30"]);
        let sent = guild_folders(
            &[
                (alone("10"), none),
                (alone("50"), none),
                (games.clone(), unread.as_slice()),
            ],
            &own,
        );
        assert_eq!(sent.order, ["10", "50", "20", "40", "30"]);

        // Another order, of the servers shown: 50 after 10, which it
        // followed, 40 after 20 in the folder, the fields as they came.
        let shown = folder("7", "Games", &["20", "30"]);
        let list = sent.arranged(&[shown, alone("10")]);
        assert_eq!(list.order, ["20", "40", "30", "10", "50"]);
        assert_eq!(list.folders, [games]);
        assert_eq!(unread_of(&list, "7"), unread);
        assert!(fields(&list.encoded).any(|(number, _)| number == 5));
        assert_eq!(server_list(&list.write()), Some(list));

        // A folder made of 10 and 20: 40 stays in "Games", before 30.
        let made = folder("99", "", &["10", "20"]);
        let list = sent.arranged(&[made, folder("7", "Games", &["30"])]);
        assert_eq!(list.order, ["10", "20", "50", "40", "30"]);
        assert_eq!(list.folders[1], folder("7", "Games", &["40", "30"]));

        // 30 out of the folder, after it: 40 stays in it, after 20.
        let list = sent.arranged(&[alone("10"), folder("7", "Games", &["20"]), alone("30")]);
        assert_eq!(list.order, ["10", "50", "20", "40", "30"]);
        assert_eq!(list.folders, [folder("7", "Games", &["20", "40"])]);

        // Renamed: the list as it was, the folder with its new name.
        let list = sent.arranged(&[alone("10"), folder("7", "Raids", &["20", "30"])]);
        assert_eq!(list.order, sent.order);
        assert_eq!(list.folders, [folder("7", "Raids", &["20", "40", "30"])]);
        assert_eq!(unread_of(&list, "7"), unread);

        // Every server shown out of "Games": it stays where it was, with 40.
        let list = sent.arranged(&[alone("10"), alone("20"), alone("30")]);
        assert_eq!(list.order, ["10", "50", "40", "20", "30"]);
        assert_eq!(list.folders, [folder("7", "Games", &["40"])]);
        assert_eq!(unread_of(&list, "7"), unread);

        // A server goes once Discord's list stops naming it.
        let left = guild_folders(
            &[
                (alone("10"), none),
                (folder("7", "Games", &["20", "30"]), none),
            ],
            none,
        );
        let list = left.arranged(&[folder("7", "Games", &["20", "30"]), alone("10")]);
        assert_eq!(list.order, ["20", "30", "10"]);
    }

    #[test]
    fn a_write_over_the_legacy_order_keeps_it_in_the_folders() {
        let mut legacy = Vec::new();
        let packed: Vec<u8> = [2u64, 1].iter().flat_map(|id| id.to_le_bytes()).collect();
        put_bytes(&mut legacy, GUILD_POSITIONS, &packed);
        let sent = ServerList::read(legacy.into());
        assert_eq!(sent.order, ["2", "1"]);
        let alone = Folder {
            servers: vec!["1".into()],
            ..Folder::default()
        };
        let list = sent.arranged(&[alone]);
        assert_eq!(list.order, ["2", "1"], "2, not shown, stays first");
        assert!(
            fields(&list.encoded).all(|(number, _)| number == FOLDERS),
            "never guild_positions"
        );
    }

    /// One `GuildFolder` as the written bytes hold it, read here by hand
    /// rather than by the code under test: its servers, its id (empty for
    /// a server alone), name and colour, and its other fields' bytes.
    #[derive(Debug, Clone, PartialEq)]
    struct Decoded {
        servers: Vec<String>,
        id: String,
        name: Option<String>,
        color: Option<u64>,
        rest: Vec<u8>,
    }

    /// `GuildFolders`' entries, and its own other fields, from the base64
    /// settings a PATCH sends, which must hold `guild_folders` alone.
    fn decode_patch(patch: &str) -> (Vec<Decoded>, Vec<u8>) {
        let data = base64::engine::general_purpose::STANDARD
            .decode(patch)
            .unwrap();
        let top: Vec<_> = fields(&data).collect();
        assert_eq!(top.len(), 1, "guild_folders alone");
        let (GUILD_FOLDERS, Field::Bytes(list)) = top[0] else {
            panic!("guild_folders alone, a message");
        };
        let wrapped = |bytes: &[u8]| -> Vec<(u64, u64, Vec<u8>)> {
            let mut out = Vec::new();
            let mut data = bytes;
            while let Some(key) = varint(&mut data) {
                match key & 7 {
                    0 => out.push((key >> 3, varint(&mut data).unwrap(), Vec::new())),
                    2 => {
                        let len = varint(&mut data).unwrap() as usize;
                        out.push((key >> 3, 0, data[..len].to_vec()));
                        data = &data[len..];
                    }
                    other => panic!("wire type {other} in a wrapper"),
                }
            }
            out
        };
        let mut entries = Vec::new();
        let mut own = Vec::new();
        for (number, value) in fields(list) {
            let entry = match (number, &value) {
                (1, Field::Bytes(entry)) => *entry,
                (2, _) => panic!("never guild_positions"),
                _ => {
                    put_field(&mut own, number, &value);
                    continue;
                }
            };
            let mut decoded = Decoded {
                servers: Vec::new(),
                id: String::new(),
                name: None,
                color: None,
                rest: Vec::new(),
            };
            for (number, value) in fields(entry) {
                match (number, value) {
                    (1, Field::Bytes(packed)) => {
                        assert_eq!(packed.len() % 8, 0, "packed fixed64");
                        decoded.servers.extend(packed.chunks(8).map(|bytes| {
                            u64::from_le_bytes(bytes.try_into().unwrap()).to_string()
                        }));
                    }
                    (2, Field::Bytes(wrapper)) => {
                        let inner = wrapped(wrapper);
                        assert_eq!(inner.len(), 1);
                        decoded.id = inner[0].1.to_string();
                    }
                    (3, Field::Bytes(wrapper)) => {
                        let inner = wrapped(wrapper);
                        decoded.name = Some(String::from_utf8(inner[0].2.clone()).unwrap());
                    }
                    (4, Field::Bytes(wrapper)) => decoded.color = Some(wrapped(wrapper)[0].1),
                    (1..=4, _) => panic!("a known field in the wrong shape"),
                    (number, value) => put_field(&mut decoded.rest, number, &value),
                }
            }
            assert!(!decoded.servers.is_empty(), "never an empty folder");
            if decoded.id.is_empty() {
                assert_eq!(decoded.servers.len(), 1, "a server alone is one");
            }
            entries.push(decoded);
        }
        (entries, own)
    }

    /// Random lists Discord sent, each with servers this client doesn't
    /// show (alone, in folders with shown ones, in folders of their own),
    /// and fields Yapfast doesn't read, then a random arrangement of the
    /// servers shown: what goes holds every server once, the shown ones as
    /// arranged, each one not shown in its folder, after what it followed,
    /// and every field it doesn't read where it was.
    #[test]
    fn every_write_keeps_each_server_not_shown_in_its_folder_after_what_it_followed() {
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut random = move |below: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % below as u64) as usize
        };
        for _ in 0..3000 {
            // Discord's list: 1001..=1012 in random entries.
            let mut ids: Vec<String> = (1001..=1012).map(|id: u32| id.to_string()).collect();
            for at in (1..ids.len()).rev() {
                ids.swap(at, random(at + 1));
            }
            let mut sent: Vec<(Folder, Vec<u8>)> = Vec::new();
            let mut next_folder = 7;
            let mut left = ids.as_slice();
            while !left.is_empty() {
                let take = if random(2) == 0 {
                    1
                } else {
                    1 + random(4.min(left.len()))
                };
                let (servers, rest) = left.split_at(take);
                left = rest;
                let mut unread = Vec::new();
                if random(3) == 0 {
                    put_bytes(&mut unread, 9, format!("u{}", sent.len()).as_bytes());
                }
                let folder = if take > 1 || random(3) == 0 {
                    next_folder += 1;
                    Folder {
                        id: next_folder.to_string(),
                        name: (random(2) == 0).then(|| format!("F{next_folder}")),
                        color: (random(2) == 0).then_some(next_folder),
                        servers: servers.to_vec(),
                    }
                } else {
                    Folder {
                        servers: servers.to_vec(),
                        ..Folder::default()
                    }
                };
                sent.push((folder, unread));
            }
            let mut own = Vec::new();
            if random(2) == 0 {
                put_bytes(&mut own, 5, b"own");
            }
            let refs: Vec<(Folder, &[u8])> = sent
                .iter()
                .map(|(folder, rest)| (folder.clone(), rest.as_slice()))
                .collect();
            let base = guild_folders(&refs, &own);

            // Shown: some of them, and one joined since, not in the list.
            let mut shown: Vec<String> = ids.iter().filter(|_| random(5) < 3).cloned().collect();
            if random(3) == 0 {
                shown.push("2001".into());
            }
            for at in (1..shown.len()).rev() {
                shown.swap(at, random(at + 1));
            }
            let sent_ids: Vec<String> = sent
                .iter()
                .map(|(folder, _)| folder.id.clone())
                .filter(|id| !id.is_empty())
                .collect();
            let mut free_ids: Vec<String> = sent_ids.clone();
            let mut entries: Vec<Folder> = Vec::new();
            let mut left = shown.as_slice();
            let mut made = 100;
            while !left.is_empty() {
                let take = 1 + random(3.min(left.len()));
                let (servers, rest) = left.split_at(take);
                left = rest;
                let entry = match random(3) {
                    0 if take == 1 => Folder {
                        servers: servers.to_vec(),
                        ..Folder::default()
                    },
                    1 if !free_ids.is_empty() => {
                        let id = free_ids.remove(random(free_ids.len()));
                        Folder {
                            id,
                            name: (random(2) == 0).then(|| "Renamed".into()),
                            color: (random(2) == 0).then_some(0xabcdef),
                            servers: servers.to_vec(),
                        }
                    }
                    _ if take == 1 && random(2) == 0 => Folder {
                        servers: servers.to_vec(),
                        ..Folder::default()
                    },
                    _ => {
                        made += 1;
                        Folder {
                            id: made.to_string(),
                            servers: servers.to_vec(),
                            ..Folder::default()
                        }
                    }
                };
                entries.push(entry);
            }

            let list = base.arranged(&entries);
            let (out, out_own) = decode_patch(&list.write());
            let case = format!("sent {sent:?}\nentries {entries:?}\nwrote {out:?}");

            // Every server once: Discord's and the shown.
            let mut all: Vec<&String> = out.iter().flat_map(|entry| &entry.servers).collect();
            let order: Vec<String> = all.iter().map(|id| (*id).clone()).collect();
            all.sort();
            all.dedup();
            assert_eq!(all.len(), order.len(), "a server twice\n{case}");
            let mut expected: Vec<&String> = ids.iter().chain(&shown).collect();
            expected.sort();
            expected.dedup();
            assert_eq!(all, expected, "{case}");
            assert_eq!(list.order, order, "the list reads as it went\n{case}");

            // The shown ones as arranged, in their entries.
            let shown_order: Vec<&String> = order.iter().filter(|id| shown.contains(id)).collect();
            let arranged: Vec<&String> = entries.iter().flat_map(|entry| &entry.servers).collect();
            assert_eq!(shown_order, arranged, "{case}");
            let entry_of = |id: &str| {
                out.iter()
                    .position(|entry| entry.servers.iter().any(|s| s == id))
                    .unwrap()
            };
            for entry in &entries {
                for id in &entry.servers {
                    let written = &out[entry_of(id)];
                    assert_eq!(written.id, entry.id, "{id}\n{case}");
                    if !entry.id.is_empty() {
                        assert_eq!(written.name, entry.name, "{case}");
                        assert_eq!(written.color, entry.color.map(u64::from), "{case}");
                    }
                }
            }

            // Each not shown in its folder, or alone, after what it followed.
            for (at, (folder, rest)) in sent.iter().enumerate() {
                for (index, id) in folder.servers.iter().enumerate() {
                    if shown.contains(id) {
                        continue;
                    }
                    let written = &out[entry_of(id)];
                    assert_eq!(written.id, folder.id, "{id} keeps its folder\n{case}");
                    if folder.id.is_empty() {
                        assert_eq!(&written.rest, rest, "{case}");
                        // After the entry it followed, when that's there.
                        if let Some((before, _)) = at.checked_sub(1).map(|before| &sent[before]) {
                            let there = if before.id.is_empty() {
                                Some(entry_of(&before.servers[0]))
                            } else {
                                out.iter().position(|entry| entry.id == before.id)
                            };
                            if let Some(there) = there {
                                assert!(there < entry_of(id), "{id} after its neighbour\n{case}");
                            }
                        }
                    } else {
                        let within = |of: &str| written.servers.iter().position(|s| s == of);
                        if let Some(before) = index.checked_sub(1).map(|i| &folder.servers[i])
                            && let Some(there) = within(before)
                        {
                            assert!(there < within(id).unwrap(), "{id} after {before}\n{case}");
                        }
                        if !entries.iter().any(|entry| entry.id == folder.id) {
                            assert_eq!(written.name, folder.name, "{case}");
                            assert_eq!(written.color, folder.color.map(u64::from), "{case}");
                        }
                    }
                }
            }

            // Each folder once, with the fields it came with.
            let mut folder_ids: Vec<&String> = out
                .iter()
                .map(|entry| &entry.id)
                .filter(|id| !id.is_empty())
                .collect();
            let count = folder_ids.len();
            folder_ids.sort();
            folder_ids.dedup();
            assert_eq!(folder_ids.len(), count, "a folder twice\n{case}");
            for written in out.iter().filter(|entry| !entry.id.is_empty()) {
                let came = sent
                    .iter()
                    .find(|(folder, _)| folder.id == written.id)
                    .map(|(_, rest)| rest.clone())
                    .unwrap_or_default();
                assert_eq!(written.rest, came, "{case}");
            }
            assert_eq!(out_own, own, "the list's own fields\n{case}");
        }
    }

    /// Discord's list: 50 alone, not shown; A (10); "Raids" (8) of 60 and
    /// 70, none shown, with a field Yapfast doesn't read; "Games" (7) of
    /// B (20), 40 not shown, C (30). Each write, as the bytes hold it, keeps
    /// 50, 60, 70 and 40 where they were, through a move to the top and to
    /// the bottom, writes queued, a refusal and another device's list.
    #[test]
    fn servers_not_shown_stay_in_the_bytes_through_moves_refusals_and_other_devices() {
        let folder = |id: &str, servers: &[&str]| Folder {
            id: id.into(),
            name: (!id.is_empty()).then(|| format!("F{id}")),
            servers: servers.iter().map(|&id| id.into()).collect(),
            ..Folder::default()
        };
        let alone = |id: &str| folder("", &[id]);
        let mut unread = Vec::new();
        put_bytes(&mut unread, 9, b"raids");
        let none: &[u8] = &[];
        let sent = guild_folders(
            &[
                (alone("50"), none),
                (alone("10"), none),
                (folder("8", &["60", "70"]), unread.as_slice()),
                (folder("7", &["20", "40", "30"]), none),
            ],
            none,
        );
        // The bytes as entries: (folder id, servers).
        let shape = |list: &ServerList| -> Vec<(String, Vec<String>)> {
            decode_patch(&list.write())
                .0
                .into_iter()
                .map(|entry| (entry.id, entry.servers))
                .collect()
        };
        let entry = |id: &str, servers: &[&str]| {
            (
                id.to_owned(),
                servers.iter().map(|&id| id.to_owned()).collect::<Vec<_>>(),
            )
        };

        // "Games" to the top: 50, first, stays first; "Raids" after A.
        let top = sent.arranged(&[folder("7", &["20", "30"]), alone("10")]);
        assert_eq!(
            shape(&top),
            [
                entry("", &["50"]),
                entry("7", &["20", "40", "30"]),
                entry("", &["10"]),
                entry("8", &["60", "70"]),
            ]
        );
        assert_eq!(decode_patch(&top.write()).0[3].rest, unread);
        // To the bottom.
        let bottom = sent.arranged(&[alone("10"), folder("7", &["20", "30"])]);
        assert_eq!(
            shape(&bottom),
            [
                entry("", &["50"]),
                entry("", &["10"]),
                entry("8", &["60", "70"]),
                entry("7", &["20", "40", "30"]),
            ]
        );

        // A write queued behind another goes over the first, taken or not;
        // both refused, the list before them comes back, every server in it.
        let mut writes = ServerWrites::default();
        writes.start(|| sent.clone());
        let first = sent.arranged(&[folder("7", &["30", "20"]), alone("10")]);
        writes.start(|| unreachable!("a write is on its way"));
        let second = first.arranged(&[alone("30"), folder("7", &["20"]), alone("10")]);
        assert_eq!(
            shape(&second),
            [
                entry("", &["50"]),
                entry("", &["30"]),
                entry("7", &["20", "40"]),
                entry("", &["10"]),
                entry("8", &["60", "70"]),
            ]
        );
        assert_eq!(writes.end(first, false), None);
        let back = writes.end(second, false).expect("the list before");
        assert_eq!(shape(&back), shape(&sent));

        // Another device puts 40 alone first and A in "Raids": the next
        // write goes over that, and keeps it.
        let other = guild_folders(
            &[
                (alone("40"), none),
                (alone("50"), none),
                (folder("8", &["60", "10", "70"]), unread.as_slice()),
                (folder("7", &["20", "30"]), none),
            ],
            none,
        );
        let next = other.arranged(&[folder("7", &["30", "20"]), folder("8", &["10"])]);
        assert_eq!(
            shape(&next),
            [
                entry("", &["40"]),
                entry("", &["50"]),
                entry("7", &["30", "20"]),
                entry("8", &["60", "10", "70"]),
            ]
        );
    }

    /// Settings holding a status written over none.
    fn status_only(status: &str) -> String {
        encode_status(&write_status(&[], status, 1))
    }

    fn custom_only(custom: Option<&CustomStatus>) -> String {
        encode_status(&write_custom(&[], custom, 1))
    }

    #[test]
    fn reads_and_writes_the_custom_status() {
        let status = CustomStatus {
            text: "Fixing the bridge".into(),
            emoji_name: Some("pog".into()),
            emoji_id: Some("1400000000000000300".into()),
            expires_at_ms: Some(1_791_300_000_000),
        };
        let written = custom_only(Some(&status));
        assert_eq!(
            read(&written),
            StatusSettings {
                status: None,
                custom: Some(Some(status.clone())),
                afk_timeout: None,
                show_game: None,
                nsfw_commands_in_dms: None,
            }
        );
        assert_eq!(read(&custom_only(None)).custom, Some(None));
        assert_eq!(
            read(&status_only("idle")),
            StatusSettings {
                status: Some("idle".into()),
                custom: Some(None),
                afk_timeout: None,
                show_game: None,
                nsfw_commands_in_dms: None,
            },
            "the status comes whole: none in it is none"
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
        assert_eq!(
            read.custom,
            Some(None),
            "a status without a custom status clears it, as another device's clearing does"
        );
        assert_eq!(super::read("not base64!"), StatusSettings::default());
    }

    #[test]
    fn reads_and_writes_whether_the_game_shows() {
        let game = |shown| encode_status(&write_show_game(&[], shown));
        assert_eq!(read(&game(false)).show_game, Some(false));
        assert_eq!(read(&game(true)).show_game, Some(true));
        assert_eq!(read(&status_only("idle")).show_game, None);
    }

    #[test]
    fn each_write_keeps_the_rest_of_the_status() {
        let custom = CustomStatus {
            text: "away for lunch".into(),
            ..CustomStatus::default()
        };
        // The account's status: Do Not Disturb, set at 5 for a while, and a
        // field Yapfast doesn't know.
        let mut current = Vec::new();
        put_bytes(&mut current, STATUS_STATUS, b"\x0a\x03dnd");
        put_fixed64(&mut current, STATUS_EXPIRES, 9);
        put_bytes(&mut current, STATUS_CREATED, b"\x08\x05");
        put_bytes(&mut current, 9, b"kept");
        let has = |status: &[u8], number: u64| fields(status).any(|(field, _)| field == number);

        let written = write_custom(&current, Some(&custom), 100);
        let expected = StatusSettings {
            status: Some("dnd".into()),
            custom: Some(Some(custom.clone())),
            afk_timeout: None,
            show_game: None,
            nsfw_commands_in_dms: None,
        };
        assert_eq!(read(&encode_status(&written)), expected);
        let data = base64::engine::general_purpose::STANDARD
            .decode(encode_status(&written))
            .unwrap();
        assert_eq!(
            fields(&data).map(|(number, _)| number).collect::<Vec<_>>(),
            [STATUS],
            "only the status goes, the rest stay as they are"
        );
        assert!(
            fields(&written)
                .any(|(number, value)| number == STATUS_EXPIRES
                    && matches!(value, Field::Fixed64(9))),
            "the status still ends when it would"
        );
        assert!(
            has(&written, 9),
            "unknown fields stay, as the client keeps them"
        );
        assert!(
            fields(message(&written, STATUS_CUSTOM).unwrap())
                .any(|(number, value)| number == CUSTOM_CREATED
                    && matches!(value, Field::Fixed64(100))),
            "a custom status says when it was set"
        );

        let written = write_show_game(&written, false);
        assert_eq!(
            read(&encode_status(&written)),
            StatusSettings {
                show_game: Some(false),
                ..expected.clone()
            }
        );

        // The same status again keeps when it was set; another is set now
        // and stays until changed.
        let same = write_status(&written, "dnd", 200);
        assert_eq!(message(&same, STATUS_CREATED), Some(&b"\x08\x05"[..]));
        let written = write_status(&written, "idle", 200);
        assert_eq!(
            read(&encode_status(&written)),
            StatusSettings {
                status: Some("idle".into()),
                show_game: Some(false),
                ..expected
            }
        );
        assert!(
            !has(&written, STATUS_EXPIRES),
            "a new status stays until changed"
        );
        assert_eq!(
            message(&written, STATUS_CREATED),
            Some(&b"\x08\xc8\x01"[..]),
            "and says when it was set"
        );

        // A cleared custom status is left out, as the client clears it.
        let cleared = write_custom(&written, None, 300);
        assert!(!has(&cleared, STATUS_CUSTOM));
        assert_eq!(read(&encode_status(&cleared)).custom, Some(None));
    }

    #[test]
    fn reads_whether_age_restricted_commands_are_offered_in_dms() {
        let encoded = |on: bool| {
            let mut wrapper = Vec::new();
            if on {
                put_varint(&mut wrapper, 1 << 3 | VARINT);
                put_varint(&mut wrapper, 1);
            }
            let mut text = Vec::new();
            put_bytes(&mut text, VIEW_NSFW_COMMANDS, &wrapper);
            let mut settings = Vec::new();
            put_bytes(&mut settings, TEXT_AND_IMAGES, &text);
            base64::engine::general_purpose::STANDARD.encode(settings)
        };
        assert_eq!(read(&encoded(true)).nsfw_commands_in_dms, Some(true));
        assert_eq!(read(&encoded(false)).nsfw_commands_in_dms, Some(false));
        assert_eq!(read(&status_only("idle")).nsfw_commands_in_dms, None);
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
        assert_eq!(server_list(&custom_only(None)), None);
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
        assert_eq!(read(&status_only("dnd")).afk_timeout, None);
    }
}
