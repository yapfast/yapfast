//! The picker's stickers: a server's own and Discord's default packs, as
//! Discord lists them and as the picker sends them.

use serde_json::Value;

/// The size the picker's tiles show a sticker at, which is a sticker's own
/// size as Discord draws it there.
pub const TILE_SIZE: u32 = 96;

/// The size Discord shows a sticker at in a message.
pub const MESSAGE_SIZE: u32 = 160;

/// A sticker the picker offers.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Sticker {
    pub id: String,
    pub name: String,
    /// Discord's `format_type`: 1 PNG, 2 APNG, 3 Lottie, 4 GIF. A missing
    /// one is a PNG.
    pub format: u64,
    /// A server's sticker its boosts no longer cover, `available: false`:
    /// Discord's client leaves it out of the picker.
    #[serde(default)]
    pub unavailable: bool,
}

impl Sticker {
    /// Its picture's address, at the size the picker's tiles show it.
    pub fn picture(&self) -> String {
        crate::ready::sticker_at(&self.id, self.format, TILE_SIZE)
    }
}

/// Stickers the picker lists under one heading: a server's own, or one of
/// Discord's default packs.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Collection {
    /// The pack's id, or a server's for its own stickers.
    pub id: String,
    pub name: String,
    pub stickers: Vec<Sticker>,
}

/// One sticker, as a sticker pack lists it.
pub fn parse(sticker: &Value) -> Option<Sticker> {
    Some(Sticker {
        id: sticker["id"].as_str()?.to_owned(),
        name: sticker["name"].as_str()?.to_owned(),
        format: sticker["format_type"]
            .as_u64()
            .or_else(|| sticker["format_type"].as_str()?.parse().ok())
            .unwrap_or(1),
        unavailable: sticker["available"].as_bool() == Some(false),
    })
}

/// A list of stickers, as a guild object's or GUILD_STICKERS_UPDATE's
/// `stickers`.
pub fn parse_all(list: &Value) -> Vec<Sticker> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(parse)
        .collect()
}

/// Discord's default packs, as `GET /sticker-packs` answers them, in its
/// order; a pack with no sticker is left out.
pub fn packs(answer: &Value) -> Vec<Collection> {
    answer["sticker_packs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|pack| {
            let stickers = parse_all(&pack["stickers"]);
            Some(Collection {
                id: pack["id"].as_str()?.to_owned(),
                name: pack["name"].as_str()?.to_owned(),
                stickers,
            })
            .filter(|pack| !pack.stickers.is_empty())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::ready::sticker_at;

    #[test]
    fn reads_a_packs_stickers_and_their_pictures() {
        let answer = json!({"sticker_packs": [
            {"id": "1", "name": "Wumpus", "stickers": [
                {"id": "10", "name": "Wave", "format_type": 1},
                {"id": "11", "name": "Bounce", "format_type": 4},
            ]},
            {"id": "2", "name": "Empty", "stickers": []},
        ]});
        let packs = packs(&answer);
        assert_eq!(packs.len(), 1, "a pack with no sticker isn't listed");
        assert_eq!(packs[0].name, "Wumpus");
        assert_eq!(
            packs[0].stickers[0].picture(),
            sticker_at("10", 1, TILE_SIZE)
        );
        assert!(
            packs[0].stickers[1].picture().contains("/stickers/11.gif"),
            "a GIF sticker comes from the media proxy"
        );
    }

    #[test]
    fn a_sticker_without_a_format_is_a_png() {
        let sticker = parse(&json!({"id": "7", "name": "Wave"})).unwrap();
        assert_eq!(sticker.format, 1);
        assert!(sticker.picture().ends_with("/stickers/7.png?size=96"));
    }

    #[test]
    fn a_sticker_the_boosts_no_longer_cover_says_so() {
        let sticker =
            |available| parse(&json!({"id": "7", "name": "Wave", "available": available}));
        assert!(sticker(json!(false)).unwrap().unavailable);
        assert!(!sticker(json!(true)).unwrap().unavailable);
        // A pack's sticker says nothing, and is there.
        assert!(
            !parse(&json!({"id": "7", "name": "Wave"}))
                .unwrap()
                .unavailable
        );
    }

    #[test]
    fn a_guilds_stickers_read_in_order() {
        let list = json!([
            {"id": "5", "name": "Spin", "format_type": 3},
            {"id": "6", "name": "Hi", "format_type": 2},
        ]);
        assert_eq!(
            parse_all(&list)
                .iter()
                .map(|sticker| sticker.id.as_str())
                .collect::<Vec<_>>(),
            ["5", "6"]
        );
    }
}
