//! A theme's `content`: how it draws what people bring — their pictures,
//! avatars, names, profiles, embeds and activity — rather than Yapfast's own
//! surfaces.
//!
//! It only shows when the person lets it: Settings → Themes → *Let the theme
//! style people's profiles and media*, off unless turned on, for each theme.
//! Off, everyone's pictures, colours and cosmetics show as they made them.
//!
//! Every field picks from a closed list, and `keep` (or leaving it out) draws
//! the thing as stock. Pictures are recoloured once, as they're handed to the
//! interface, never per frame; text keeps 4.5:1 against what it's on.

use serde_json::Value;

use super::layout::closed;
use super::package::{Error, colour_at, named, number, object};

closed! {
    /// The shape every avatar is cut to.
    AvatarShape { Keep => "keep", Circle => "circle", Square => "square", Rounded => "rounded" }
}

closed! {
    /// Something people wear that a theme may leave out.
    Shown { Keep => "keep", Hide => "hide" }
}

closed! {
    /// People's own colours, or the theme's.
    Follow { Keep => "keep", Theme => "theme" }
}

closed! {
    /// How role colours on names show.
    NameColours {
        /// As the server gave them.
        Keep => "keep",
        /// Every name in the theme's name colour.
        Theme => "theme",
        /// Each keeps its hue, with its saturation and lightness pulled into
        /// the theme's range and its contrast forced.
        Clamp => "clamp",
        /// Each person's name in a colour of the theme's `palette`, picked
        /// from who they are, as a chat terminal colours nicks.
        Palette => "palette",
    }
}

closed! {
    /// How nameplates behind names show.
    Nameplates { Keep => "keep", Hide => "hide", Tint => "tint" }
}

/// How people's pictures are drawn: avatars, server icons, emoji pictures,
/// stickers, attachments, embed thumbnails, link previews and GIFs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Media {
    #[default]
    Normal,
    /// Luminance mapped from `dark` to `light`, as a two-ink print. ARGB.
    Duotone { dark: u32, light: u32 },
    /// Washed towards `colour` by `amount` (0–1).
    Tint { colour: u32, amount: f32 },
    /// Greys.
    Mono,
}

impl Media {
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Duotone { .. } => "duotone",
            Self::Tint { .. } => "tint",
            Self::Mono => "mono",
        }
    }
}

/// `content.avatars`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Avatars {
    pub shape: AvatarShape,
    /// Corner radius in pixels for `rounded` (0–24).
    pub radius: f32,
    pub decorations: Shown,
    /// A ring in the theme's colour around every avatar, in place of
    /// decorations: ARGB and width in pixels (1–4).
    pub ring: Option<(u32, f32)>,
}

/// `content.names`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Names {
    pub colours: NameColours,
    /// The colour `theme` draws every name in, ARGB; the theme's `text`
    /// when not set.
    pub colour: Option<u32>,
    /// `clamp`'s saturation and lightness ranges, 0–1 each; see
    /// [`Names::clamp_ranges`] for when they're not set.
    pub saturation: Option<(f32, f32)>,
    pub lightness: Option<(f32, f32)>,
    /// Display name styles (fonts, gradients, glow) and ransom names.
    pub styles: Follow,
    pub nameplates: Nameplates,
    /// `palette`'s colours, ARGB, the first `palette_len` of them.
    pub palette: [u32; MAX_PALETTE],
    pub palette_len: u8,
}

/// The most colours a names palette holds.
pub const MAX_PALETTE: usize = 8;

impl Names {
    /// The palette's colours.
    pub fn palette(&self) -> &[u32] {
        &self.palette[..usize::from(self.palette_len)]
    }

    /// The palette's colour for someone, from their id, the same each time.
    pub fn palette_colour(&self, id: &str) -> Option<u32> {
        let palette = self.palette();
        if palette.is_empty() {
            return None;
        }
        // FNV-1a: steady across runs and machines, unlike the std hasher.
        let mut hash: u32 = 0x811c_9dc5;
        for byte in id.bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        Some(palette[hash as usize % palette.len()])
    }

    /// Saturation and lightness `clamp` holds colours in: the theme's, or
    /// muted mid tones that read on its ground.
    pub fn clamp_ranges(&self, dark: bool) -> ((f32, f32), (f32, f32)) {
        (
            self.saturation.unwrap_or((0.2, 0.65)),
            self.lightness
                .unwrap_or(if dark { (0.6, 0.82) } else { (0.22, 0.42) }),
        )
    }
}

/// `content.profiles`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Profiles {
    /// Profile themes: Nitro colours and the banner colour. `theme` puts the
    /// card on the theme's own card surface and art; a banner picture still
    /// shows, recoloured as `media` says.
    pub themes: Follow,
    pub effects: Shown,
}

/// What a theme does to people's content; stock when default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Content {
    pub media: Media,
    pub avatars: Avatars,
    pub names: Names,
    pub profiles: Profiles,
    /// The embed colour bar and the links in embeds.
    pub embeds: Follow,
    /// Activity and game tiles, and the LIVE pill.
    pub activity: Follow,
}

impl Content {
    /// Whether it changes anything at all, so the setting is offered.
    pub fn is_stock(&self) -> bool {
        *self == Self::default()
    }

    /// What it changes, as the cost line lists it: `content (if allowed):
    /// duotone pictures, square avatars, theme name colours`.
    pub fn summary(&self) -> Option<String> {
        self.phrase()
            .map(|phrase| format!("content (if allowed): {phrase}"))
    }

    /// What it changes, as a list in words: `duotone pictures, square
    /// avatars`; none when it changes nothing.
    pub fn phrase(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if self.media != Media::Normal {
            parts.push(format!("{} pictures", self.media.name()));
        }
        if self.avatars.shape != AvatarShape::Keep {
            parts.push(format!("{} avatars", self.avatars.shape.name()));
        }
        if self.avatars.ring.is_some() {
            parts.push("avatar rings".into());
        } else if self.avatars.decorations == Shown::Hide {
            parts.push("no decorations".into());
        }
        if self.names.colours != NameColours::Keep {
            parts.push(format!("{} name colours", self.names.colours.name()));
        }
        if self.names.styles == Follow::Theme {
            parts.push("theme name styles".into());
        }
        if self.names.nameplates != Nameplates::Keep {
            parts.push(format!("nameplates {}", self.names.nameplates.name()));
        }
        if self.profiles.themes == Follow::Theme {
            parts.push("theme profile cards".into());
        }
        if self.profiles.effects == Shown::Hide {
            parts.push("no profile effects".into());
        }
        if self.embeds == Follow::Theme {
            parts.push("theme embeds".into());
        }
        if self.activity == Follow::Theme {
            parts.push("theme activity".into());
        }
        (!parts.is_empty()).then(|| parts.join(", "))
    }
}

pub(super) fn read_content(
    value: &Value,
    place: &str,
    content: &mut Content,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (key, value) in object(value, place)? {
        let here = format!("{place}.{key}");
        match key.as_str() {
            "media" => content.media = read_media(value, &here)?,
            "avatars" => read_avatars(value, &here, &mut content.avatars, notes)?,
            "names" => read_names(value, &here, &mut content.names, notes)?,
            "profiles" => {
                for (key, value) in object(value, &here)? {
                    let at = format!("{here}.{key}");
                    match key.as_str() {
                        "themes" => {
                            content.profiles.themes = named(value, &at, Follow::ALL, Follow::name)?
                        }
                        "effects" => {
                            content.profiles.effects = named(value, &at, Shown::ALL, Shown::name)?
                        }
                        _ => {
                            notes.push(format!("\"{at}\" isn't something profiles takes; ignored"))
                        }
                    }
                }
            }
            "embeds" => content.embeds = named(value, &here, Follow::ALL, Follow::name)?,
            "activity" => content.activity = named(value, &here, Follow::ALL, Follow::name)?,
            _ => notes.push(format!("\"{here}\" isn't something content takes; ignored")),
        }
    }
    Ok(())
}

/// `"normal"`, `"mono"`, `{ "duotone": { "dark", "light" } }` or
/// `{ "tint": { "colour", "amount" } }`.
fn read_media(value: &Value, at: &str) -> Result<Media, Error> {
    let wrong = || {
        Error(format!(
            "\"{at}\" is \"normal\", \"mono\", {{ \"duotone\": {{ \"dark\", \"light\" }} }} or {{ \"tint\": {{ \"colour\", \"amount\" }} }}"
        ))
    };
    match value {
        Value::String(text) => match text.as_str() {
            "normal" => Ok(Media::Normal),
            "mono" => Ok(Media::Mono),
            _ => Err(wrong()),
        },
        Value::Object(fields) if fields.len() == 1 => {
            let (kind, inner) = fields.iter().next().expect("one field");
            let here = format!("{at}.{kind}");
            let inner = object(inner, &here)?;
            let colour = |key: &str| {
                inner
                    .get(key)
                    .ok_or_else(|| Error(format!("\"{here}\" needs \"{key}\"")))
                    .and_then(|value| colour_at(value, &format!("{here}.{key}")))
            };
            match kind.as_str() {
                "duotone" => Ok(Media::Duotone {
                    dark: colour("dark")?,
                    light: colour("light")?,
                }),
                "tint" => Ok(Media::Tint {
                    colour: colour("colour")?,
                    amount: match inner.get("amount") {
                        Some(amount) => number(amount, &format!("{here}.amount"), (0.0, 1.0))?,
                        None => 0.35,
                    },
                }),
                _ => Err(wrong()),
            }
        }
        _ => Err(wrong()),
    }
}

fn read_avatars(
    value: &Value,
    here: &str,
    avatars: &mut Avatars,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    for (key, value) in object(value, here)? {
        let at = format!("{here}.{key}");
        match key.as_str() {
            "shape" => match value {
                Value::Object(fields) => {
                    let radius = fields.get("rounded").ok_or_else(|| {
                        Error(format!("\"{at}\" is a shape name or {{ \"rounded\": 8 }}"))
                    })?;
                    avatars.shape = AvatarShape::Rounded;
                    avatars.radius = number(radius, &format!("{at}.rounded"), (0.0, 24.0))?;
                }
                _ => {
                    avatars.shape = named(value, &at, AvatarShape::ALL, AvatarShape::name)?;
                    if avatars.shape == AvatarShape::Rounded && avatars.radius == 0.0 {
                        avatars.radius = 8.0;
                    }
                }
            },
            "decorations" => avatars.decorations = named(value, &at, Shown::ALL, Shown::name)?,
            "ring" => {
                let fields = object(value, &at)?;
                let colour = fields
                    .get("colour")
                    .ok_or_else(|| Error(format!("\"{at}\" needs \"colour\"")))
                    .and_then(|value| colour_at(value, &format!("{at}.colour")))?;
                let width = match fields.get("width") {
                    Some(width) => number(width, &format!("{at}.width"), (1.0, 4.0))?,
                    None => 2.0,
                };
                avatars.ring = Some((colour, width));
            }
            _ => notes.push(format!("\"{at}\" isn't something avatars takes; ignored")),
        }
    }
    Ok(())
}

fn read_names(
    value: &Value,
    here: &str,
    names: &mut Names,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    let range = |value: &Value, at: &str| -> Result<(f32, f32), Error> {
        match value.as_array().map(Vec::as_slice) {
            Some([low, high]) => {
                let (low, high) = (number(low, at, (0.0, 1.0))?, number(high, at, (0.0, 1.0))?);
                if low > high {
                    return Err(Error(format!("\"{at}\" goes from low to high")));
                }
                Ok((low, high))
            }
            _ => Err(Error(format!("\"{at}\" is [low, high], each 0 to 1"))),
        }
    };
    for (key, value) in object(value, here)? {
        let at = format!("{here}.{key}");
        match key.as_str() {
            "colours" => names.colours = named(value, &at, NameColours::ALL, NameColours::name)?,
            "colour" => names.colour = Some(colour_at(value, &at)?),
            "saturation" => names.saturation = Some(range(value, &at)?),
            "lightness" => names.lightness = Some(range(value, &at)?),
            "palette" => {
                let items = value
                    .as_array()
                    .filter(|items| (2..=MAX_PALETTE).contains(&items.len()))
                    .ok_or_else(|| {
                        Error(format!("\"{at}\" is a list of 2 to {MAX_PALETTE} colours"))
                    })?;
                names.palette = [0; MAX_PALETTE];
                for (index, item) in items.iter().enumerate() {
                    names.palette[index] = colour_at(item, &format!("{at}[{index}]"))?;
                }
                names.palette_len = items.len() as u8;
                if names.colours == NameColours::Keep {
                    names.colours = NameColours::Palette;
                }
            }
            "styles" => names.styles = named(value, &at, Follow::ALL, Follow::name)?,
            "nameplates" => {
                names.nameplates = named(value, &at, Nameplates::ALL, Nameplates::name)?
            }
            _ => notes.push(format!("\"{at}\" isn't something names takes; ignored")),
        }
    }
    Ok(())
}

/// Recolours RGBA pixels, straight alpha, in place, as `media` says. One
/// table of 256 entries a picture, so a large one costs a lookup a pixel.
pub fn recolour(media: Media, rgba: &mut [u8]) {
    let table: [[u8; 3]; 256] = match media {
        Media::Normal => return,
        Media::Mono => std::array::from_fn(|grey| [grey as u8; 3]),
        Media::Duotone { dark, light } => {
            let (dark, light) = (channels(dark), channels(light));
            std::array::from_fn(|level| {
                std::array::from_fn(|channel| {
                    let (from, to) = (f32::from(dark[channel]), f32::from(light[channel]));
                    (from + (to - from) * level as f32 / 255.0).round() as u8
                })
            })
        }
        Media::Tint { colour, amount } => {
            // A wash by luminance, so the picture's light and shade stay.
            let colour = channels(colour);
            let table: [[u8; 3]; 256] = std::array::from_fn(|level| {
                std::array::from_fn(|channel| {
                    (f32::from(colour[channel]) * level as f32 / 255.0 * amount).round() as u8
                })
            });
            let keep = 1.0 - amount;
            for pixel in rgba.chunks_exact_mut(4) {
                let washed = table[usize::from(luma(pixel))];
                for channel in 0..3 {
                    pixel[channel] = (f32::from(pixel[channel]) * keep + f32::from(washed[channel]))
                        .round()
                        .min(255.0) as u8;
                }
            }
            return;
        }
    };
    for pixel in rgba.chunks_exact_mut(4) {
        let mapped = table[usize::from(luma(pixel))];
        pixel[..3].copy_from_slice(&mapped);
    }
}

/// Rec. 601 luma in integers.
fn luma(pixel: &[u8]) -> u8 {
    ((u32::from(pixel[0]) * 77 + u32::from(pixel[1]) * 150 + u32::from(pixel[2]) * 29) >> 8) as u8
}

fn channels(argb: u32) -> [u8; 3] {
    [(argb >> 16) as u8, (argb >> 8) as u8, argb as u8]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::package::{Mode, Package};

    fn theme(content: &str) -> Result<Package, Error> {
        Package::parse(&format!(
            r#"{{ "theme_api": 1, "id": "com.example.own", "name": "Own", "version": "1.0.0",
                "content": {content},
                "variants": {{ "dark": {{ "base": "yap" }} }} }}"#
        ))
    }

    #[test]
    fn reads_every_content_field() {
        let package = theme(
            r##"{
                "media": { "duotone": { "dark": "#03110a", "light": "#5cff85" } },
                "avatars": { "shape": { "rounded": 6 }, "decorations": "hide", "ring": { "colour": "#5cff85", "width": 2 } },
                "names": { "colours": "clamp", "colour": "#5cff85", "saturation": [0.2, 0.5], "lightness": [0.6, 0.8],
                           "styles": "theme", "nameplates": "tint" },
                "profiles": { "themes": "theme", "effects": "hide" },
                "embeds": "theme", "activity": "theme"
            }"##,
        )
        .unwrap();
        assert!(package.notes.is_empty(), "{:?}", package.notes);
        let content = package.resolve(Mode::Dark).unwrap().content;
        assert_eq!(
            content.media,
            Media::Duotone {
                dark: 0xff03110a,
                light: 0xff5cff85
            }
        );
        assert_eq!(content.avatars.shape, AvatarShape::Rounded);
        assert_eq!(content.avatars.radius, 6.0);
        assert_eq!(content.avatars.decorations, Shown::Hide);
        assert_eq!(content.avatars.ring, Some((0xff5cff85, 2.0)));
        assert_eq!(content.names.colours, NameColours::Clamp);
        assert_eq!(content.names.saturation, Some((0.2, 0.5)));
        assert_eq!(content.names.styles, Follow::Theme);
        assert_eq!(content.names.nameplates, Nameplates::Tint);
        assert_eq!(content.profiles.themes, Follow::Theme);
        assert_eq!(content.profiles.effects, Shown::Hide);
        assert_eq!(content.embeds, Follow::Theme);
        assert_eq!(content.activity, Follow::Theme);
        assert!(
            content
                .summary()
                .unwrap()
                .starts_with("content (if allowed): duotone pictures")
        );
    }

    #[test]
    fn short_forms_and_stock() {
        let package = theme(r#"{ "media": "mono", "avatars": { "shape": "square" } }"#).unwrap();
        let content = package.resolve(Mode::Dark).unwrap().content;
        assert_eq!(content.media, Media::Mono);
        assert_eq!(content.avatars.shape, AvatarShape::Square);
        let tint = theme(r##"{ "media": { "tint": { "colour": "#ff0000" } } }"##).unwrap();
        assert!(matches!(
            tint.resolve(Mode::Dark).unwrap().content.media,
            Media::Tint { amount, .. } if amount == 0.35
        ));
        let stock = theme("{}").unwrap();
        assert!(stock.resolve(Mode::Dark).unwrap().content.is_stock());
    }

    #[test]
    fn refuses_values_out_of_their_lists() {
        assert!(theme(r#"{ "media": "sepia" }"#).is_err());
        assert!(theme(r#"{ "avatars": { "shape": "hexagon" } }"#).is_err());
        assert!(theme(r#"{ "avatars": { "shape": { "rounded": 99 } } }"#).is_err());
        assert!(theme(r#"{ "names": { "lightness": [0.9, 0.1] } }"#).is_err());
        assert!(theme(r#"{ "embeds": "loud" }"#).is_err());
        let noted = theme(r#"{ "sparkle": true }"#).unwrap();
        assert!(
            noted
                .notes
                .iter()
                .any(|note| note.contains("content.sparkle"))
        );
    }

    #[test]
    fn recolouring_maps_luminance_and_keeps_alpha() {
        let mut pixels = [0, 0, 0, 255, 255, 255, 255, 128];
        recolour(
            Media::Duotone {
                dark: 0xff102030,
                light: 0xffa0b0c0,
            },
            &mut pixels,
        );
        assert_eq!(pixels, [0x10, 0x20, 0x30, 255, 0xa0, 0xb0, 0xc0, 128]);
        let mut grey = [255, 0, 0, 255];
        recolour(Media::Mono, &mut grey);
        assert_eq!(grey[0], grey[1]);
        assert_eq!(grey[1], grey[2]);
        let mut same = [12, 34, 56, 78];
        recolour(Media::Normal, &mut same);
        assert_eq!(same, [12, 34, 56, 78]);
    }
}
