//! Profile effects: the animation over a profile card that someone equips
//! from Discord's shop. A user's profile names the effect by SKU
//! (`profile_effect: {id, expires_at}`); the layers it plays come from the
//! shop's product, `GET /collectibles-products/{sku}`, whose item Discord's
//! client builds its `ProfileEffect` from. See
//! <https://docs.discord.food/resources/collectibles>.

use serde_json::Value;

use crate::ready;

/// How an effect plays, as Discord's `animationType` has it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    /// Not said; plays as persistent.
    #[default]
    Unspecified,
    /// Plays when the card opens and then holds the effect's static frame.
    Persistent,
    /// Plays only while Discord's window has the focus.
    Intermittent,
}

/// One picture of an effect: an APNG laid over the card at a place, shown from
/// a time for a duration, around again when it loops.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Layer {
    /// The address of its picture.
    pub src: String,
    /// When it starts and how long it plays, in milliseconds after the card
    /// opens (and the half second Discord waits before it starts).
    pub start: u32,
    pub duration: u32,
    /// It plays again after `loop_delay` milliseconds, and again.
    pub looped: bool,
    pub loop_delay: u32,
    /// Where its top left corner lies, in pixels from the card's.
    pub x: i32,
    pub y: i32,
    /// Layers with a higher index lie over those with a lower one.
    pub z: i32,
}

/// A profile effect, as the shop describes it.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Effect {
    /// The SKU it was sold as.
    pub sku: String,
    pub kind: Kind,
    /// What it shows, for screen readers.
    pub label: String,
    /// Its layers from the lowest up.
    pub layers: Vec<Layer>,
    /// The picture it holds when it has played through, for a persistent one.
    pub still: Option<String>,
}

/// The effect a profile response names: the server's, else the user's own.
/// Where the usual `profile_effect` field is null, an effect equipped as a
/// shop collectible is looked for among the `collectibles`.
pub fn sku(profile: &Value, guild: bool) -> Option<String> {
    let named = |value: &Value| {
        sku_of(&value["profile_effect"]).or_else(|| in_collectibles(&value["collectibles"]))
    };
    guild
        .then(|| named(&profile["guild_member_profile"]))
        .flatten()
        .or_else(|| named(&profile["user_profile"]))
        .or_else(|| named(&profile["user"]))
        .or_else(|| named(profile))
}

/// The SKU a profile effect value names: `{id}`, `{sku_id}` or a bare string.
fn sku_of(effect: &Value) -> Option<String> {
    if let Some(sku) = effect.as_str() {
        return Some(sku.to_owned()).filter(|sku| !sku.is_empty());
    }
    ["id", "sku_id", "skuId"]
        .into_iter()
        .find_map(|key| effect[key].as_str().map(str::to_owned))
}

/// The effect among a profile's `collectibles`, which may be an object keyed
/// by kind (`{"nameplate": {..}, "profile_effect": {..}}`) or a list of items
/// that say what they are in `type`.
fn in_collectibles(collectibles: &Value) -> Option<String> {
    let is_effect = |kind: &Value| {
        kind.as_str().is_some_and(|kind| {
            let kind = kind.to_ascii_lowercase();
            kind.contains("effect")
        })
    };
    match collectibles {
        Value::Object(object) => object
            .iter()
            .find(|(key, _)| is_effect(&Value::from(key.as_str())))
            .and_then(|(_, value)| sku_of(value))
            .or_else(|| {
                object
                    .values()
                    .find(|value| is_effect(&value["type"]))
                    .and_then(sku_of)
            }),
        Value::Array(items) => items
            .iter()
            .find(|item| is_effect(&item["type"]) || !item["profile_effect"].is_null())
            .and_then(|item| sku_of(&item["profile_effect"]).or_else(|| sku_of(item))),
        _ => None,
    }
}

/// What a `collectibles` value holds, for the log: its keys, and for each
/// entry the type of its value and the identifying fields it carries. Never
/// the values of anything else.
pub fn shape(collectibles: &Value) -> String {
    let kind = |value: &Value| match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    let entry = |name: &str, value: &Value| {
        let fields = value
            .as_object()
            .map(|object| {
                object
                    .iter()
                    .map(|(key, value)| match key.as_str() {
                        "sku_id" | "id" | "type" | "profile_effect" | "label" => {
                            format!("{key}={value}")
                        }
                        _ => format!("{key}:{}", kind(value)),
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| value.to_string());
        format!("[{name} {} {{{fields}}}]", kind(value))
    };
    match collectibles {
        Value::Object(object) => object
            .iter()
            .map(|(key, value)| entry(key, value))
            .collect(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, value)| entry(&index.to_string(), value))
            .collect(),
        other => entry("-", other),
    }
}

/// An address the shop gives, which is whole or starts at the CDN's root.
fn address(src: &str) -> String {
    if src.starts_with("http://") || src.starts_with("https://") {
        src.to_owned()
    } else {
        format!("{}/{}", ready::cdn(), src.trim_start_matches('/'))
    }
}

/// Reads a field Discord sends in camelCase, or in snake_case where it
/// doesn't.
fn field<'a>(value: &'a Value, camel: &str, snake: &str) -> &'a Value {
    if value.get(camel).is_some() {
        &value[camel]
    } else {
        &value[snake]
    }
}

fn millis(value: &Value) -> u32 {
    value
        .as_u64()
        .map_or(0, |value| u32::try_from(value).unwrap_or(u32::MAX))
}

/// The effect in a collectible product's answer: its first item that has
/// effects.
pub fn parse(sku: &str, product: &Value) -> Option<Effect> {
    let item = product["items"]
        .as_array()?
        .iter()
        .find(|item| item["effects"].is_array())?;
    parse_config(sku, item)
}

/// The effect `sku` names in the answer of `GET /user-profile-effects`, which
/// lists every effect's config as `{"profile_effect_configs": [...]}`. A
/// config is named by its own id or by the SKU it is sold as.
pub fn find(sku: &str, configs: &Value) -> Option<Effect> {
    let list = configs["profile_effect_configs"]
        .as_array()
        .or_else(|| configs.as_array())?;
    let named = |config: &&Value| {
        ["id", "sku_id", "skuId"]
            .into_iter()
            .any(|key| config[key].as_str() == Some(sku))
    };
    parse_config(sku, list.iter().find(named)?)
}

/// An effect from its config, the object that holds `effects`.
pub fn parse_config(sku: &str, item: &Value) -> Option<Effect> {
    let mut layers: Vec<Layer> = item["effects"]
        .as_array()?
        .iter()
        .enumerate()
        .filter_map(|(index, layer)| {
            let position = &layer["position"];
            Some(Layer {
                src: address(layer["src"].as_str()?),
                // Discord's client calls the delay before a layer starts
                // `start`; the API has also sent it as `start_delay`.
                start: millis(if layer.get("start").is_some() {
                    &layer["start"]
                } else {
                    field(layer, "startDelay", "start_delay")
                }),
                duration: millis(&layer["duration"]),
                looped: layer["loop"].as_bool().unwrap_or(false),
                loop_delay: millis(field(layer, "loopDelay", "loop_delay")),
                x: position["x"].as_i64().map_or(0, |x| x as i32),
                y: position["y"].as_i64().map_or(0, |y| y as i32),
                // The client lays layers in the order they come when none says.
                z: field(layer, "zIndex", "z_index")
                    .as_i64()
                    .map_or(index as i32, |z| z as i32),
            })
        })
        .collect();
    layers.sort_by_key(|layer| layer.z);
    let kind = match field(item, "animationType", "animation_type").as_u64() {
        Some(1) => Kind::Persistent,
        Some(2) => Kind::Intermittent,
        _ => Kind::Unspecified,
    };
    Some(Effect {
        sku: sku.to_owned(),
        kind,
        label: field(item, "accessibilityLabel", "accessibility_label")
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        layers,
        still: field(item, "staticFrameSrc", "static_frame_src")
            .as_str()
            .or_else(|| field(item, "thumbnailPreviewSrc", "thumbnail_preview_src").as_str())
            .map(address),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn names_the_effect_a_profile_equips_with_the_servers_first() {
        let profile = json!({
            "user_profile": {"profile_effect": {"id": "10", "expires_at": null}},
            "guild_member_profile": {"profile_effect": {"id": "20"}},
        });
        assert_eq!(sku(&profile, true).as_deref(), Some("20"));
        assert_eq!(sku(&profile, false).as_deref(), Some("10"));
        assert_eq!(
            sku(&json!({"profile_effect": {"id": "30"}}), true).as_deref(),
            Some("30")
        );
        assert_eq!(
            sku(&json!({"user_profile": {"profile_effect": null}}), true),
            None
        );
    }

    #[test]
    fn finds_an_effect_equipped_among_the_collectibles() {
        let keyed = json!({"user_profile": {"profile_effect": null,
            "collectibles": {"nameplate": {"sku_id": "1"}, "profile_effect": {"sku_id": "40"}}}});
        assert_eq!(sku(&keyed, false).as_deref(), Some("40"));
        let listed = json!({"user_profile": {"collectibles": [
            {"type": "nameplate", "sku_id": "1"}, {"type": "profile_effect", "sku_id": "41"}]}});
        assert_eq!(sku(&listed, true).as_deref(), Some("41"));
        let none = json!({"user_profile": {"collectibles": {"nameplate": {"sku_id": "1"}}}});
        assert_eq!(sku(&none, true), None);
        assert!(shape(&keyed["user_profile"]["collectibles"]).contains("sku_id=\"40\""));
    }

    #[test]
    fn reads_the_layers_of_a_product_in_the_order_they_lie() {
        let product = json!({"sku_id": "9", "items": [
        {"type": 1, "sku_id": "9", "asset": "x"},
        {"type": 3, "sku_id": "9", "accessibilityLabel": "Falling petals", "animationType": 1,
         "staticFrameSrc": "https://cdn.discordapp.com/assets/petals/still.png",
         "effects": [
            {"src": "https://cdn.discordapp.com/assets/petals/loop.png", "loop": true,
             "start": 1200, "duration": 5000, "loopDelay": 800,
             "position": {"x": 0, "y": 12}, "zIndex": 102},
            {"src": "/assets/petals/intro.png", "loop": false, "start": 0, "duration": 1500,
             "position": {"x": 0, "y": 0}, "zIndex": 101, "height": 880, "width": 450},
        ]}]});
        let effect = parse("9", &product).unwrap();
        assert_eq!(
            (effect.kind, effect.label.as_str()),
            (Kind::Persistent, "Falling petals")
        );
        assert_eq!(effect.layers.len(), 2);
        let (intro, looping) = (&effect.layers[0], &effect.layers[1]);
        assert!(intro.src.ends_with("/assets/petals/intro.png") && intro.src.starts_with("http"));
        assert_eq!(
            (intro.start, intro.duration, intro.looped),
            (0, 1500, false)
        );
        assert_eq!(
            (looping.start, looping.duration, looping.loop_delay),
            (1200, 5000, 800)
        );
        assert_eq!((looping.looped, looping.y, looping.z), (true, 12, 102));
        assert!(effect.still.unwrap().ends_with("/petals/still.png"));
        assert_eq!(parse("9", &json!({"items": [{"type": 1}]})), None);
    }

    #[test]
    fn finds_an_effect_in_the_list_by_its_id_or_sku() {
        let list = json!({"profile_effect_configs": [
            {"type": 1, "id": "5", "sku_id": "6", "accessibility_label": "Confetti",
             "animation_type": 1,
             "thumbnail_preview_src": "https://cdn.discordapp.com/assets/c/thumb.png",
             "effects": [
                {"src": "https://cdn.discordapp.com/assets/c/a.png", "loop": false,
                 "start_delay": 300, "duration": 2000, "position": {"x": 0, "y": 0}, "z_index": 1}
             ]},
            {"id": "7", "sku_id": "8", "effects": []},
        ]});
        for sku in ["5", "6"] {
            let effect = find(sku, &list).unwrap();
            assert_eq!(effect.sku, sku);
            assert_eq!(effect.layers.len(), 1);
            assert_eq!(effect.layers[0].start, 300);
            assert!(effect.still.unwrap().ends_with("/thumb.png"));
        }
        assert_eq!(find("9", &list), None);
        assert!(find("7", &list).unwrap().layers.is_empty());
    }
}
