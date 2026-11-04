//! What a person equips from Discord's shop, as it reaches Yapfast on user
//! objects: an avatar decoration, drawn around their avatar, and a nameplate,
//! drawn behind their row in a member list and on their account card. Both
//! are pictures on the CDN's media path, named by the SKU they were sold as.

use serde_json::Value;

use crate::ready;

/// A nameplate's palette: what Discord's client calls `darkBackground`, which
/// it tints the plate's gradient with on a dark theme. A palette Discord
/// adds later, or a custom one, has no plate here.
const PALETTES: [(&str, u32); 12] = [
    ("crimson", 0x90_00_07),
    ("berry", 0x89_3a_99),
    ("sky", 0x00_80_b7),
    ("teal", 0x08_64_60),
    ("forest", 0x2d_54_01),
    ("bubble_gum", 0xdc_3e_97),
    ("violet", 0x73_0b_c8),
    ("cobalt", 0x01_31_c2),
    ("clover", 0x04_7b_20),
    ("lemon", 0xf6_cd_12),
    ("white", 0xff_ff_ff),
    ("black", 0x00_00_00),
];

/// A nameplate someone equips, as [`crate::message::Names`] keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plate {
    /// The SKU it was sold as, which names its pictures.
    pub sku: u64,
    /// Its palette, as an index into [`PALETTES`].
    palette: u8,
}

impl Plate {
    /// Reads `collectibles.nameplate`, when it has a SKU and a palette
    /// Yapfast knows.
    pub fn parse(nameplate: &Value) -> Option<Self> {
        let sku = nameplate["sku_id"].as_str()?.parse().ok()?;
        let palette = palette(nameplate["palette"].as_str()?)?;
        Some(Self { sku, palette })
    }

    /// A plate saved by [`Self::palette_name`].
    pub fn restore(sku: u64, palette: &str) -> Option<Self> {
        Some(Self {
            sku,
            palette: self::palette(palette)?,
        })
    }

    /// The palette's name, as Discord sends it.
    pub fn palette_name(&self) -> &'static str {
        PALETTES[usize::from(self.palette)].0
    }

    /// The plate as a member list or card shows it.
    pub fn shown(&self) -> Nameplate {
        Nameplate {
            picture: picture(self.sku),
            color: PALETTES[usize::from(self.palette)].1,
        }
    }
}

fn palette(name: &str) -> Option<u8> {
    PALETTES
        .iter()
        .position(|(known, _)| *known == name)
        .and_then(|index| u8::try_from(index).ok())
}

/// Display name styles Discord's client draws a name with: a font, an effect
/// and up to five colours. Most people have none, so [`crate::message::Names`]
/// keeps them in a map of those who do, at 24 bytes each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Style {
    /// Discord's `font_id`: 11 is the usual font, the rest are Sakura (3),
    /// Jellybean (4), Modern (6), Medieval (7), 8Bit (8), Vampyre (10),
    /// Tempo (12), Monkey Bars (13), Mainframe (14), Headbang (15) and
    /// Journal (16).
    pub font: u8,
    /// Discord's `effect_id`: solid 1, gradient 2, neon 3, toon 4, pop 5, glow
    /// 6, prism 7, gummy 8.
    pub effect: u8,
    len: u8,
    colors: [u32; 5],
}

impl Style {
    /// Reads `display_name_styles`: `{font_id, effect_id, colors}`, the
    /// colours as integers.
    pub fn parse(style: &Value) -> Option<Self> {
        let font = u8::try_from(style["font_id"].as_u64()?).ok()?;
        let effect = u8::try_from(style["effect_id"].as_u64()?).ok()?;
        let mut colors = [0; 5];
        let mut len = 0;
        for color in style["colors"].as_array()?.iter().take(5) {
            colors[len] = u32::try_from(color.as_u64()?).ok()? & 0xff_ff_ff;
            len += 1;
        }
        Some(Self {
            font,
            effect,
            len: u8::try_from(len).ok()?,
            colors,
        })
    }

    /// The colours it asks for, as RGB.
    pub fn colors(&self) -> &[u32] {
        &self.colors[..usize::from(self.len)]
    }
}

/// A nameplate ready to show.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Nameplate {
    /// The address of its still picture: a strip that fills the row's right
    /// end, behind the name.
    pub picture: String,
    /// The RGB colour its gradient takes from its palette.
    pub color: u32,
}

/// The SKU of an avatar decoration, from `avatar_decoration_data`.
pub fn decoration_sku(data: &Value) -> Option<u64> {
    data["sku_id"].as_str()?.parse().ok()
}

/// The address of a collectible's still picture, which Discord's client
/// shows unless the person hovers it: the decoration around an avatar, or
/// the strip of a nameplate. The animated one is `/animated`, an APNG,
/// or `/video`, a WebM.
pub fn picture(sku: u64) -> String {
    format!("{}/media/v1/collectibles-shop/{sku}/static", ready::cdn())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_a_nameplate_and_its_palette() {
        let plate = Plate::parse(&json!({
            "sku_id": "1409983105577783410", "asset": "nameplates/cherry/",
            "label": "Cherry blossoms", "palette": "violet",
        }))
        .unwrap();
        assert_eq!(plate.sku, 1_409_983_105_577_783_410);
        assert_eq!(plate.palette_name(), "violet");
        let shown = plate.shown();
        assert_eq!(shown.color, 0x730bc8);
        assert!(
            shown
                .picture
                .ends_with("/media/v1/collectibles-shop/1409983105577783410/static")
        );
        assert_eq!(Plate::restore(plate.sku, plate.palette_name()), Some(plate));
    }

    #[test]
    fn skips_a_palette_it_does_not_know() {
        let custom = json!({"sku_id": "5", "palette": "custom_palette"});
        assert_eq!(Plate::parse(&custom), None);
        assert_eq!(Plate::parse(&json!({"palette": "violet"})), None);
        assert_eq!(decoration_sku(&json!({"asset": "a_x"})), None);
        assert_eq!(decoration_sku(&json!({"sku_id": "42"})), Some(42));
    }
}
