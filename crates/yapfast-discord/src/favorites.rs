//! Discord's account-wide Favorites pseudo-guild (PreloadedUserSettings field 15).
//! Writes replace that field only, retaining every untouched entry and setting.

use crate::settings::{Field, fields, message, put_bytes, put_field, put_fixed64, put_varint};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};

const FAVORITES: u64 = 15;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Favorite {
    pub id: u64,
    pub nickname: String,
    /// 1 references a channel; 2 is a category. Unknown kinds stay untouched.
    pub kind: u32,
    pub position: u32,
    pub parent_id: u64,
    pub channel_type: Option<u32>,
    pub collapsed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Favorites {
    pub entries: Vec<Favorite>,
    pub muted: bool,
    pub visible: Option<bool>,
}

impl Favorites {
    pub fn contains(&self, id: u64) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.id == id && entry.kind == 1)
    }

    pub fn shown(&self) -> bool {
        self.visible.unwrap_or(!self.entries.is_empty())
    }
}

// Unlike the general settings reader, a read used for a write must reject a
// truncated protobuf, rather than silently replacing its successfully read prefix.
fn validate(mut data: &[u8]) -> Result<()> {
    while !data.is_empty() {
        let key = take_varint(&mut data)?;
        ensure!(key >> 3 != 0, "Invalid Favorites settings");
        let length = match key & 7 {
            0 => {
                take_varint(&mut data)?;
                0
            }
            1 => 8,
            2 => usize::try_from(take_varint(&mut data)?)?,
            5 => 4,
            _ => anyhow::bail!("Invalid Favorites settings"),
        };
        data = data.get(length..).context("Truncated Favorites settings")?;
    }
    Ok(())
}

fn take_varint(data: &mut &[u8]) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..=63).step_by(7) {
        let (&byte, rest) = data.split_first().context("Truncated Favorites settings")?;
        *data = rest;
        ensure!(shift != 63 || byte <= 1, "Invalid Favorites settings");
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    anyhow::bail!("Invalid Favorites settings")
}

fn entry_id(data: &[u8]) -> u64 {
    fields(data)
        .filter_map(|(n, v)| match (n, v) {
            (1, Field::Fixed64(id)) => Some(id),
            _ => None,
        })
        .last()
        .unwrap_or(0)
}

fn decode(data: &[u8]) -> Result<Favorites> {
    validate(data)?;
    let mut list = Favorites::default();
    for (number, value) in fields(data) {
        match (number, value) {
            (1, Field::Bytes(map)) => {
                validate(map)?;
                let value = message(map, 2).unwrap_or_default();
                validate(value)?;
                let mut entry = Favorite {
                    id: entry_id(map),
                    ..Default::default()
                };
                for (n, v) in fields(value) {
                    match (n, v) {
                        (1, Field::Bytes(name)) => {
                            entry.nickname = String::from_utf8(name.to_vec())?;
                        }
                        (2, Field::Varint(kind)) => entry.kind = u32::try_from(kind)?,
                        (3, Field::Varint(position)) => entry.position = u32::try_from(position)?,
                        (4, Field::Fixed64(parent)) => entry.parent_id = parent,
                        (5, Field::Bytes(wrapper)) => {
                            validate(wrapper)?;
                            entry.channel_type = Some(
                                fields(wrapper)
                                    .find_map(|(n, v)| match (n, v) {
                                        (1, Field::Varint(kind)) => u32::try_from(kind).ok(),
                                        _ => None,
                                    })
                                    .unwrap_or(0),
                            );
                        }
                        (6, Field::Varint(on)) => entry.collapsed = on != 0,
                        _ => {}
                    }
                }
                list.entries.retain(|old| old.id != entry.id);
                list.entries.push(entry);
            }
            (2, Field::Varint(on)) => list.muted = on != 0,
            (3, Field::Bytes(wrapper)) => {
                validate(wrapper)?;
                list.visible = Some(
                    fields(wrapper)
                        .any(|(n, v)| n == 1 && matches!(v, Field::Varint(on) if on != 0)),
                );
            }
            _ => {}
        }
    }
    list.entries.sort_by_key(|entry| (entry.position, entry.id));
    Ok(list)
}

/// None is a partial update that leaves Favorites alone; Some(empty) clears it.
pub fn read(encoded: &str) -> Result<Option<Favorites>> {
    let data = STANDARD.decode(encoded)?;
    validate(&data)?;
    message(&data, FAVORITES).map(decode).transpose()
}

/// Add/remove one reference using a fresh GET of the account settings. Unknown
/// fields, categories, order, mute and visibility survive. No-op writes send nothing.
pub fn change(encoded: &str, id: u64, channel_type: u32, favorite: bool) -> Result<Option<String>> {
    ensure!(id != 0, "Invalid channel");
    let data = STANDARD.decode(encoded)?;
    validate(&data)?;
    let current = message(&data, FAVORITES).unwrap_or_default();
    let list = decode(current)?;
    if list.contains(id) == favorite {
        return Ok(None);
    }
    ensure!(
        !favorite || !list.entries.iter().any(|entry| entry.id == id),
        "That Favorites ID is already in use"
    );
    let mut changed = Vec::new();
    for (n, value) in fields(current) {
        if n == 1 && matches!(&value, Field::Bytes(map) if entry_id(map) == id) {
            continue;
        }
        put_field(&mut changed, n, &value);
    }
    if favorite {
        let position = list
            .entries
            .iter()
            .filter(|entry| entry.parent_id == 0)
            .map(|entry| entry.position)
            .max()
            .map_or(Some(0), |n| n.checked_add(1))
            .context("Favorites order is full")?;
        let mut value = Vec::new();
        put_varint(&mut value, 2 << 3);
        put_varint(&mut value, 1);
        put_varint(&mut value, 3 << 3);
        put_varint(&mut value, u64::from(position));
        let mut wrapper = Vec::new();
        put_varint(&mut wrapper, 1 << 3);
        put_varint(&mut wrapper, u64::from(channel_type));
        put_bytes(&mut value, 5, &wrapper);
        let mut map = Vec::new();
        put_fixed64(&mut map, 1, id);
        put_bytes(&mut map, 2, &value);
        put_bytes(&mut changed, 1, &map);
    }
    let mut patch = Vec::new();
    put_bytes(&mut patch, FAVORITES, &changed);
    Ok(Some(STANDARD.encode(patch)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(raw: &[u8]) -> String {
        let mut out = Vec::new();
        put_bytes(&mut out, FAVORITES, raw);
        STANDARD.encode(out)
    }

    #[test]
    fn changes_preserve_categories_unknown_fields_and_visibility() {
        let mut category = Vec::new();
        put_bytes(&mut category, 1, b"Friends");
        put_varint(&mut category, 2 << 3);
        put_varint(&mut category, 2);
        put_varint(&mut category, 6 << 3);
        put_varint(&mut category, 1);
        put_bytes(&mut category, 99, b"future category");
        let mut map = Vec::new();
        put_fixed64(&mut map, 1, 900);
        put_bytes(&mut map, 2, &category);
        let mut raw = Vec::new();
        put_bytes(&mut raw, 1, &map);
        put_varint(&mut raw, 2 << 3);
        put_varint(&mut raw, 1);
        put_bytes(&mut raw, 3, &[]); // explicit false
        put_bytes(&mut raw, 100, b"future setting");
        let original = settings(&raw);
        let added = change(&original, 42, 3, true).unwrap().unwrap();
        let list = read(&added).unwrap().unwrap();
        assert!(list.contains(42));
        assert!(list.muted);
        assert_eq!(list.visible, Some(false));
        assert_eq!(list.entries[0].nickname, "Friends");
        assert!(list.entries[0].collapsed);
        assert!(change(&added, 42, 3, true).unwrap().is_none());
        let removed = change(&added, 42, 3, false).unwrap().unwrap();
        assert_eq!(removed, original);
    }

    #[test]
    fn partial_empty_invalid_and_duplicate_map_entries() {
        assert_eq!(read("").unwrap(), None);
        assert_eq!(read(&settings(&[])).unwrap(), Some(Favorites::default()));
        assert!(change("not base64", 42, 0, true).is_err());
        assert!(change(&STANDARD.encode([0x7a, 5, 0x0a]), 42, 0, true).is_err());
        let first = change("", 42, 1, true).unwrap().unwrap();
        let second = change(&first, 43, 3, true).unwrap().unwrap();
        assert_eq!(
            read(&second)
                .unwrap()
                .unwrap()
                .entries
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            [42, 43]
        );
    }
}
