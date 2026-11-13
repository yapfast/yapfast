//! Profile effects: the animation over a profile card that someone equips
//! from Discord's shop. A user's profile names the effect by SKU
//! (`profile_effect: {id, expires_at}`); the layers it plays come from the
//! shop's product, `GET /collectibles-products/{sku}`, whose item Discord's
//! client builds its `ProfileEffect` from. See
//! <https://docs.discord.food/resources/collectibles>.

use serde_json::Value;

use crate::ready;

/// One picture of an effect: an APNG laid over the card at a place, shown from
/// a time for a duration, around again when it loops.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layer {
    /// The address of its picture.
    pub src: String,
    /// When it starts and how long it plays, in milliseconds after the effect
    /// starts.
    pub start: u32,
    pub duration: u32,
    /// It plays again after `loop_delay` milliseconds, and again.
    pub looped: bool,
    pub loop_delay: u32,
    /// Where its top left corner lies, in pixels from the card's.
    pub x: i32,
    pub y: i32,
}

/// A profile effect, as the shop describes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effect {
    /// The SKU it was sold as.
    pub sku: String,
    /// Its layers from the lowest up.
    pub layers: Vec<Layer>,
    /// The picture that shows while it doesn't play.
    pub still: Option<String>,
}

/// The effect a profile response names: the server's, else the user's own.
pub fn sku(profile: &Value, guild: bool) -> Option<String> {
    let named = |value: &Value| {
        let effect = &value["profile_effect"];
        effect["sku_id"]
            .as_str()
            .or(effect["id"].as_str())
            .map(str::to_owned)
    };
    guild
        .then(|| named(&profile["guild_member_profile"]))
        .flatten()
        .or_else(|| named(&profile["user_profile"]))
}

/// An address the shop gives, which is whole or starts at the CDN's root.
fn address(src: &str) -> String {
    if src.starts_with("http://") || src.starts_with("https://") {
        src.to_owned()
    } else {
        format!("{}/{}", ready::cdn(), src.trim_start_matches('/'))
    }
}

fn millis(value: &Value) -> u32 {
    value
        .as_u64()
        .map_or(0, |value| u32::try_from(value).unwrap_or(u32::MAX))
}

/// The effect in a collectible product's answer: its first item that has
/// effects. An effect with `randomizedSources` plays one of them, the same
/// one in each layer, as the client does: `pick` chooses it.
pub fn parse(sku: &str, product: &Value, pick: usize) -> Option<Effect> {
    let item = product["items"]
        .as_array()?
        .iter()
        .find(|item| item["effects"].is_array())?;
    let mut layers: Vec<(i64, Layer)> = item["effects"]
        .as_array()?
        .iter()
        .enumerate()
        .filter_map(|(index, layer)| {
            let position = &layer["position"];
            let sources = layer["randomizedSources"].as_array();
            let src = sources
                .filter(|sources| !sources.is_empty())
                .and_then(|sources| sources[pick % sources.len()]["src"].as_str())
                .or(layer["src"].as_str())?;
            Some((
                layer["zIndex"].as_i64().unwrap_or(index as i64),
                Layer {
                    src: address(src),
                    start: millis(&layer["start"]),
                    duration: millis(&layer["duration"]),
                    looped: layer["loop"].as_bool().unwrap_or(false),
                    loop_delay: millis(&layer["loopDelay"]),
                    x: position["x"].as_i64().map_or(0, |x| x as i32),
                    y: position["y"].as_i64().map_or(0, |y| y as i32),
                },
            ))
        })
        .collect();
    layers.sort_by_key(|(z, _)| *z);
    Some(Effect {
        sku: sku.to_owned(),
        layers: layers.into_iter().map(|(_, layer)| layer).collect(),
        still: item["staticFrameSrc"]
            .as_str()
            .or(item["reducedMotionSrc"].as_str())
            .or(item["thumbnailPreviewSrc"].as_str())
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
            "guild_member_profile": {"profile_effect": {"sku_id": "20"}},
        });
        assert_eq!(sku(&profile, true).as_deref(), Some("20"));
        assert_eq!(sku(&profile, false).as_deref(), Some("10"));
        assert_eq!(
            sku(&json!({"user_profile": {"profile_effect": null}}), true),
            None
        );
    }

    #[test]
    fn reads_the_layers_of_a_product_in_the_order_they_lie() {
        let product = json!({"sku_id": "9", "items": [
        {"type": 1, "sku_id": "9", "asset": "x"},
        {"type": 1, "sku_id": "9", "animationType": 1,
         "staticFrameSrc": "https://cdn.discordapp.com/assets/petals/still.png",
         "reducedMotionSrc": "https://cdn.discordapp.com/assets/petals/reduced.png",
         "effects": [
            {"src": "https://cdn.discordapp.com/assets/petals/loop.png", "loop": true,
             "start": 1200, "duration": 5000, "loopDelay": 800,
             "position": {"x": 0, "y": 12}, "zIndex": 102},
            {"src": "/assets/petals/intro.png", "loop": false, "start": 0, "duration": 1500,
             "position": {"x": 0, "y": 0}, "zIndex": 101, "height": 880, "width": 450},
        ]}]});
        let effect = parse("9", &product, 0).unwrap();
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
        assert_eq!((looping.looped, looping.y), (true, 12));
        assert!(effect.still.unwrap().ends_with("/petals/still.png"));
        assert_eq!(parse("9", &json!({"items": [{"type": 1}]}), 0), None);
    }

    #[test]
    fn holds_the_reduced_motion_picture_when_it_has_no_static_frame() {
        let product = json!({"items": [{"effects": [],
            "reducedMotionSrc": "https://cdn.discordapp.com/assets/c/reduced.png",
            "thumbnailPreviewSrc": "https://cdn.discordapp.com/assets/c/thumb.png"}]});
        let effect = parse("9", &product, 0).unwrap();
        assert!(effect.still.unwrap().ends_with("/reduced.png"));
    }

    #[test]
    fn plays_the_same_one_of_the_randomized_sources_in_each_layer() {
        let product = json!({"items": [{"effects": [
            {"src": "https://x/a.png", "randomizedSources": [{"src": "https://x/a0.png"}, {"src": "https://x/a1.png"}]},
            {"src": "https://x/b.png", "randomizedSources": [{"src": "https://x/b0.png"}, {"src": "https://x/b1.png"}]},
            {"src": "https://x/c.png", "randomizedSources": []},
        ]}]});
        let effect = parse("9", &product, 3).unwrap();
        let sources: Vec<_> = effect
            .layers
            .iter()
            .map(|layer| layer.src.as_str())
            .collect();
        assert_eq!(
            sources,
            ["https://x/a1.png", "https://x/b1.png", "https://x/c.png"]
        );
    }
}
