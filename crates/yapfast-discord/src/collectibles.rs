//! What a person equips from Discord's shop, as it reaches Yapfast on user
//! objects: an avatar decoration, drawn around their avatar, and a nameplate,
//! drawn behind their row in a member list and on their account card. Both
//! are pictures on the CDN's media path, named by the SKU they were sold as.

use std::sync::Arc;

use serde_json::Value;

use crate::ready;

/// A nameplate's palette: what Discord's client calls `darkBackground` and
/// `lightBackground`, which it tints the plate's gradient with on a dark theme
/// and on a light one. A palette Discord adds later, or a custom one, has no
/// colour here.
const PALETTES: [(&str, [u32; 2]); 12] = [
    ("crimson", [0x90_00_07, 0xe7_04_0f]),
    ("berry", [0x89_3a_99, 0xb1_1f_cf]),
    ("sky", [0x00_80_b7, 0x56_cc_ff]),
    ("teal", [0x08_64_60, 0x7d_ee_d7]),
    ("forest", [0x2d_54_01, 0x6a_a6_24]),
    ("bubble_gum", [0xdc_3e_97, 0xf9_57_b3]),
    ("violet", [0x73_0b_c8, 0x97_2f_ed]),
    ("cobalt", [0x01_31_c2, 0x42_78_ff]),
    ("clover", [0x04_7b_20, 0x63_cd_5a]),
    ("lemon", [0xf6_cd_12, 0xfe_d4_00]),
    ("white", [0xff_ff_ff, 0xff_ff_ff]),
    ("black", [0x00_00_00, 0x00_00_00]),
];

/// A nameplate someone equips, as [`crate::message::Names`] keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Plate {
    /// The SKU it was sold as, which names its pictures.
    pub sku: u64,
    /// Its palette, as an index into [`PALETTES`]; none for one Yapfast
    /// doesn't know, whose plate shows its picture without the colour.
    palette: Option<u8>,
}

impl Plate {
    /// Reads `collectibles.nameplate`, when it has a SKU. A palette Yapfast
    /// doesn't know, such as a custom one, leaves the plate without a colour.
    pub fn parse(nameplate: &Value) -> Option<Self> {
        let sku = nameplate["sku_id"].as_str()?.parse().ok()?;
        let palette = nameplate["palette"].as_str().and_then(palette);
        Some(Self { sku, palette })
    }

    /// A plate saved by [`Self::palette_name`].
    pub fn restore(sku: u64, palette: &str) -> Self {
        Self {
            sku,
            palette: self::palette(palette),
        }
    }

    /// The palette's name, as Discord sends it; empty without one.
    pub fn palette_name(&self) -> &'static str {
        self.palette
            .map_or("", |palette| PALETTES[usize::from(palette)].0)
    }

    /// The plate as a member list or card shows it, with the address of its
    /// picture.
    pub fn shown(&self, picture: Arc<str>) -> Nameplate {
        Nameplate {
            picture,
            colors: self.palette.map(|palette| PALETTES[usize::from(palette)].1),
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
    pub picture: Arc<str>,
    /// The RGB colours its gradient takes from its palette, on a dark theme
    /// and on a light one; none for a palette Yapfast doesn't know, which
    /// leaves the picture bare.
    pub colors: Option<[u32; 2]>,
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
        assert_eq!(
            plate.shown("plate.png".into()).colors,
            Some([0x730bc8, 0x972fed])
        );
        assert!(
            picture(plate.sku).ends_with("/media/v1/collectibles-shop/1409983105577783410/static")
        );
        assert_eq!(Plate::restore(plate.sku, plate.palette_name()), plate);
    }

    #[test]
    fn keeps_the_picture_of_a_palette_it_does_not_know() {
        let custom = Plate::parse(&json!({"sku_id": "5", "palette": "custom_palette"})).unwrap();
        let shown = custom.shown("plate.png".into());
        assert_eq!(&*shown.picture, "plate.png");
        assert_eq!(shown.colors, None);
        assert_eq!(custom.palette_name(), "");
        assert_eq!(Plate::restore(5, ""), custom, "it survives being saved");
        assert_eq!(Plate::parse(&json!({"sku_id": "5"})), Some(custom));
        assert_eq!(Plate::parse(&json!({"palette": "violet"})), None);
        assert_eq!(decoration_sku(&json!({"asset": "a_x"})), None);
        assert_eq!(decoration_sku(&json!({"sku_id": "42"})), Some(42));
    }
}
