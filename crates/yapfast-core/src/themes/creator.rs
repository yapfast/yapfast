//! The theme creator: a handful of plain choices — an accent, a ground, a
//! picture, a message style, corners, a font, density and glass — made into
//! a whole theme package. Every colour is derived from the accent and the
//! ground, and nudged until it passes the readability checks [`check`]
//! applies, so a theme made here always installs. See `docs/themes.md`.
//!
//! Nothing here touches the window: the interface keeps a [`Spec`], asks
//! [`derive`] for colours, and writes [`manifest`] with the files
//! [`store_picture`] and [`store_font`] put in the theme's folder.
//!
//! [`check`]: super::check

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::check::{self, MAX_FONT_BYTES, MAX_IMAGE_BYTES, contrast};
use super::package::READABLE;
use super::{MANIFEST, THEME_API};

/// Where a creator theme keeps its picture.
pub const PICTURE: &str = "art/picture.jpg";
/// The soft backdrop glass shows when there's no picture.
pub const BACKDROP: &str = "art/backdrop.jpg";

/// What the theme's ground is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Background {
    #[default]
    Dark,
    Light,
    /// The chosen picture, under a dark interface.
    Picture,
}

/// How messages are laid out: `layout.messages`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Messages {
    #[default]
    Flat,
    Bubbles,
    Cards,
    Irc,
}

/// Spacing: `spacing.density`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Cozy,
    Roomy,
}

/// The picture a theme was made from: its colours, as the creator read them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Picture {
    /// The average of its 16 × 16 sample, as the checks read it, `0xRRGGBB`.
    pub mean: u32,
    /// Its main colours, the most used first, `0xRRGGBB`.
    pub palette: Vec<u32>,
}

/// A font the person brought, kept in the theme's `fonts/`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportedFont {
    /// Its file in the theme's folder, `fonts/…`.
    pub file: String,
    /// The family its name table gives.
    pub family: String,
    /// Its licence, from its name table, if it says.
    pub licence: Option<String>,
    /// Some basic Latin characters are missing and draw in the default font.
    pub partial: bool,
}

/// Everything the creator asks, as `layout.json` keeps it with the custom
/// theme it made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spec {
    /// `0xRRGGBB`.
    pub accent: u32,
    /// The colour whose hue tints the grounds, `0xRRGGBB`.
    pub ground: u32,
    pub background: Background,
    /// 0 to 100: how much the picture is blurred.
    pub blur: u8,
    /// 0 to 100: how much the picture is dimmed, at least what text needs.
    pub dim: u8,
    pub messages: Messages,
    /// 0 square to 100 round; 50 is Yapfast's own corners.
    pub corners: u8,
    /// A font's key: one of [`FONTS`], or an imported font's file.
    pub body: String,
    /// The headings' font, or `None` for the body's.
    pub headings: Option<String>,
    pub density: Density,
    pub glass: bool,
    pub picture: Option<Picture>,
    pub fonts: Vec<ImportedFont>,
    /// The built-in theme it started from, whose colours stand under the
    /// ones it sets while its light or dark matches; `None` for Yapfast's
    /// own.
    pub base: Option<String>,
    /// Every hairline outline's width in pixels, 0 to 4; 1 is Yapfast's own.
    pub outline: u8,
}

impl Default for Spec {
    fn default() -> Self {
        Self {
            accent: 0xffbf47,
            ground: 0x1e1f24,
            background: Background::Dark,
            blur: 40,
            dim: 35,
            messages: Messages::Flat,
            corners: 50,
            body: "default".to_owned(),
            headings: None,
            density: Density::Cozy,
            glass: false,
            picture: None,
            fonts: Vec::new(),
            base: None,
            outline: 1,
        }
    }
}

/// A font the creator offers, which ships with Yapfast (`ui/fonts/`, each
/// under the SIL Open Font License 1.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltInFont {
    pub key: &'static str,
    /// What the creator calls it.
    pub label: &'static str,
    /// The family Yapfast already registers it under, for its sample.
    pub family: &'static str,
    pub weight: i32,
    /// Its file in `ui/fonts/`; the default font has none.
    pub file: Option<&'static str>,
    /// Its typeface's own name, for the licence.
    pub typeface: &'static str,
}

/// The fonts the creator offers, in its order.
pub const FONTS: [BuiltInFont; 8] = [
    BuiltInFont {
        key: "default",
        label: "Default",
        family: "",
        weight: 500,
        file: None,
        typeface: "DM Sans",
    },
    BuiltInFont {
        key: "readable",
        label: "Readable",
        family: "Atkinson Hyperlegible Next",
        weight: 400,
        file: Some("AtkinsonHyperlegibleNext-Regular.ttf"),
        typeface: "Atkinson Hyperlegible Next",
    },
    BuiltInFont {
        key: "slab",
        label: "Slab",
        family: "Tempo",
        weight: 600,
        file: Some("DisplayName-Tempo.ttf"),
        typeface: "Zilla Slab",
    },
    BuiltInFont {
        key: "modern",
        label: "Modern",
        family: "Modern",
        weight: 500,
        file: Some("DisplayName-Modern.ttf"),
        typeface: "MuseoModerno",
    },
    BuiltInFont {
        key: "playful",
        label: "Playful",
        family: "Monkey Bars",
        weight: 700,
        file: Some("DisplayName-MonkeyBars.ttf"),
        typeface: "Playpen Sans",
    },
    BuiltInFont {
        key: "hand",
        label: "Handwritten",
        family: "Journal",
        weight: 700,
        file: Some("DisplayName-Journal.ttf"),
        typeface: "Kalam",
    },
    BuiltInFont {
        key: "pixel",
        label: "Pixel",
        family: "8Bit",
        weight: 500,
        file: Some("DisplayName-8Bit.ttf"),
        typeface: "Pixelify Sans",
    },
    BuiltInFont {
        key: "tech",
        label: "Tech",
        family: "Mainframe",
        weight: 700,
        file: Some("DisplayName-Mainframe.ttf"),
        typeface: "Orbitron",
    },
];

/// Eight accents that read well on dark and light grounds alike.
pub const ACCENTS: [u32; 8] = [
    0xffbf47, // marigold, Yap's own
    0xff6b6b, // coral
    0xff7ac6, // pink
    0xb18cff, // violet
    0x5b8cff, // blue
    0x2ec5d3, // teal
    0x4ccf7e, // green
    0xa3b1c6, // slate
];

// Colour, in OKLab and OKLCh, where lightness steps look even.

fn to_linear(value: f64) -> f64 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn to_gamma(value: f64) -> f64 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// `0xRRGGBB` as OKLCh: lightness 0–1, chroma, hue in degrees.
pub fn lch(colour: u32) -> [f64; 3] {
    let channel = |shift: u32| to_linear(f64::from((colour >> shift) & 0xff) / 255.0);
    let (r, g, b) = (channel(16), channel(8), channel(0));
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let lightness = 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let bb = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    let hue = bb.atan2(a).to_degrees().rem_euclid(360.0);
    [lightness, a.hypot(bb), hue]
}

/// OKLCh as linear sRGB, which may be out of gamut.
fn linear_of([lightness, chroma, hue]: [f64; 3]) -> [f64; 3] {
    let (a, b) = (
        chroma * hue.to_radians().cos(),
        chroma * hue.to_radians().sin(),
    );
    let l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
    ]
}

/// OKLCh as `0xRRGGBB`, its chroma lowered until it fits in sRGB.
pub fn from_lch([lightness, chroma, hue]: [f64; 3]) -> u32 {
    let lightness = lightness.clamp(0.0, 1.0);
    let fits = |chroma: f64| {
        linear_of([lightness, chroma, hue])
            .iter()
            .all(|value| (-0.000_1..=1.000_1).contains(value))
    };
    let mut chroma = chroma.max(0.0);
    if !fits(chroma) {
        let (mut low, mut high) = (0.0, chroma);
        for _ in 0..24 {
            let middle = (low + high) / 2.0;
            if fits(middle) {
                low = middle;
            } else {
                high = middle;
            }
        }
        chroma = low;
    }
    let [r, g, b] = linear_of([lightness, chroma, hue])
        .map(|value| (to_gamma(value.clamp(0.0, 1.0)) * 255.0).round() as u32);
    (r << 16) | (g << 8) | b
}

fn opaque(colour: u32) -> u32 {
    0xff00_0000 | (colour & 0xff_ffff)
}

/// The least contrast a colour has against each of some grounds.
fn worst(colour: u32, grounds: &[u32]) -> f64 {
    grounds
        .iter()
        .map(|ground| contrast(opaque(colour), opaque(*ground)))
        .fold(f64::INFINITY, f64::min)
}

/// Moves a colour's lightness away from the grounds, a little at a time,
/// until it reads at `needed` against all of them; then drains its chroma if
/// lightness alone can't. Quietly: the person only sees a colour that reads.
pub fn nudge(colour: u32, grounds: &[u32], needed: f64, lighter: bool) -> u32 {
    if worst(colour, grounds) >= needed {
        return colour;
    }
    let [mut lightness, mut chroma, hue] = lch(colour);
    for _ in 0..200 {
        lightness = (lightness + if lighter { 0.005 } else { -0.005 }).clamp(0.0, 1.0);
        if lightness <= 0.0 || lightness >= 1.0 {
            chroma *= 0.8;
        }
        let candidate = from_lch([lightness, chroma, hue]);
        if worst(candidate, grounds) >= needed {
            return candidate;
        }
    }
    if lighter { 0xffffff } else { 0x000000 }
}

/// What text needs, with a margin over the checks' bar.
const TEXT: f64 = READABLE + 0.15;
/// What secondary and muted text need over art, with a margin.
const HINT: f64 = check::art::HINT + 0.2;

/// A theme's colours, as the creator made them.
#[derive(Clone, Debug, PartialEq)]
pub struct Derived {
    pub dark: bool,
    /// The built-in theme its colours stand over.
    pub base: &'static str,
    /// Every colour it sets, by its name in `ui/theme.slint`, as `0xAARRGGBB`.
    pub colours: Vec<(&'static str, u32)>,
    /// How dimmed the picture is, 0–1: the person's choice, or more where
    /// text needs it.
    pub dim: f32,
    /// How much of a glass pane's ground covers what's behind it, 0–1.
    pub glass_opacity: f32,
}

impl Derived {
    pub fn colour(&self, token: &str) -> Option<u32> {
        self.colours
            .iter()
            .find(|(name, _)| *name == token)
            .map(|(_, colour)| *colour)
    }

    fn set(&mut self, token: &'static str, colour: u32) {
        match self.colours.iter_mut().find(|(name, _)| *name == token) {
            Some(slot) => slot.1 = colour,
            None => self.colours.push((token, colour)),
        }
    }
}

/// The colours the window's `Theme` holds that the creator sets: all but
/// the protected red.
pub const BASE_TOKENS: [&str; 34] = [
    "rail",
    "sidebar",
    "panel",
    "chat",
    "stage",
    "raised",
    "field",
    "sunken",
    "hover",
    "active",
    "row-hover",
    "line",
    "edge",
    "outline",
    "outline-chat",
    "outline-field",
    "text",
    "secondary",
    "muted",
    "faint",
    "accent",
    "accent-hover",
    "on-accent",
    "accent-soft",
    "live",
    "loud",
    "link",
    "button-default",
    "button-default-hover",
    "overlay",
    "shimmer",
    "reply-spine",
    "tooltip",
    "avatar",
];

/// Whether a spec makes a dark theme.
pub fn is_dark(spec: &Spec) -> bool {
    spec.background != Background::Light
}

/// `hover` and `row-hover` are washes laid over whatever the row sits on, a
/// ground or a picture, never a ground of their own: white on dark themes,
/// black on light, at these alphas out of 255 (about 8% and 6%; 6% and 4%).
const HOVER: u32 = 0x14;
const ROW_HOVER: u32 = 0x0f;
const HOVER_LIGHT: u32 = 0x0f;
const ROW_HOVER_LIGHT: u32 = 0x0a;

/// The washes that light a row under the pointer.
const WASHES: [&str; 2] = ["hover", "row-hover"];

/// The grounds and inks of a spec, before any is nudged: each ground a step
/// of lightness in the ground's hue, as Yap's own steps go.
fn ramp(spec: &Spec) -> Vec<(&'static str, u32)> {
    let dark = is_dark(spec);
    let [_, ground_chroma, ground_hue] = lch(spec.ground);
    let tint = ground_chroma.min(if dark { 0.03 } else { 0.022 });
    let ground = |lightness: f64, scale: f64| from_lch([lightness, tint * scale, ground_hue]);
    let ink = |lightness: f64| from_lch([lightness, tint * 0.35, ground_hue]);
    let accent = spec.accent & 0xff_ffff;
    let [accent_lightness, accent_chroma, accent_hue] = lch(accent);
    let alpha = |colour: u32, alpha: u32| (alpha << 24) | (colour & 0xff_ffff);
    let colours: Vec<(&'static str, u32)> = if dark {
        let line = ground(0.32, 0.9);
        vec![
            ("rail", ground(0.175, 1.0)),
            ("sidebar", ground(0.2, 1.0)),
            ("panel", ground(0.18, 1.0)),
            ("chat", ground(0.235, 1.0)),
            ("stage", ground(0.235, 1.0)),
            ("raised", ground(0.27, 0.95)),
            ("field", ground(0.285, 0.9)),
            ("sunken", ground(0.15, 1.0)),
            ("hover", alpha(0xffffff, HOVER)),
            ("active", ground(0.305, 0.9)),
            ("row-hover", alpha(0xffffff, ROW_HOVER)),
            ("line", line),
            ("edge", ground(0.13, 1.0)),
            ("outline", line),
            ("outline-chat", line),
            ("outline-field", line),
            ("text", ink(0.955)),
            ("secondary", ink(0.82)),
            ("muted", ink(0.7)),
            ("faint", ink(0.5)),
            ("accent", accent),
            (
                "accent-hover",
                from_lch([
                    (accent_lightness + 0.06).min(0.97),
                    accent_chroma,
                    accent_hue,
                ]),
            ),
            ("on-accent", 0),
            ("accent-soft", alpha(accent, 0x26)),
            ("live", 0x3ecf7e),
            ("loud", 0xff8c42),
            ("link", from_lch([0.74, 0.13, 250.0])),
            ("button-default", ground(0.335, 0.8)),
            ("button-default-hover", ground(0.37, 0.8)),
            ("overlay", 0xffffff),
            ("shimmer", 0x10ff_ffff),
            ("reply-spine", ground(0.44, 0.6)),
            ("tooltip", ground(0.13, 1.0)),
            ("avatar", from_lch([0.58, 0.03, ground_hue])),
        ]
    } else {
        let line = ground(0.87, 1.0);
        vec![
            ("rail", ground(0.915, 1.0)),
            ("sidebar", ground(0.948, 0.9)),
            ("panel", ground(0.93, 1.0)),
            ("chat", ground(0.985, 0.4)),
            ("stage", ground(0.985, 0.4)),
            ("raised", ground(0.995, 0.25)),
            ("field", ground(0.93, 0.9)),
            ("sunken", ground(0.895, 1.0)),
            ("hover", alpha(0x000000, HOVER_LIGHT)),
            ("active", ground(0.895, 1.0)),
            ("row-hover", alpha(0x000000, ROW_HOVER_LIGHT)),
            ("line", line),
            ("edge", ground(0.84, 1.0)),
            ("outline", line),
            ("outline-chat", line),
            ("outline-field", ground(0.82, 1.0)),
            ("text", ink(0.24)),
            ("secondary", ink(0.38)),
            ("muted", ink(0.48)),
            ("faint", ink(0.68)),
            ("accent", accent),
            (
                "accent-hover",
                from_lch([
                    (accent_lightness - 0.06).max(0.05),
                    accent_chroma,
                    accent_hue,
                ]),
            ),
            ("on-accent", 0),
            ("accent-soft", alpha(accent, 0x2e)),
            ("live", 0x1f9d55),
            ("loud", 0xd9622b),
            ("link", from_lch([0.52, 0.16, 255.0])),
            ("button-default", ground(0.89, 1.0)),
            ("button-default-hover", ground(0.86, 1.0)),
            ("overlay", 0x000000),
            ("shimmer", 0x66ff_ffff),
            ("reply-spine", ground(0.78, 1.0)),
            ("tooltip", ground(0.995, 0.25)),
            ("avatar", from_lch([0.62, 0.03, ground_hue])),
        ]
    };
    colours
        .into_iter()
        .map(|(token, colour)| {
            (
                token,
                if colour >> 24 == 0 {
                    opaque(colour)
                } else {
                    colour
                },
            )
        })
        .collect()
}

/// Derives a spec's every colour from its accent and ground.
pub fn derive(spec: &Spec) -> Derived {
    finish(spec, ramp(spec))
}

/// A spec's colours from a set the person may have fine-tuned (Appearance's
/// Advanced): the theme-only colours, the inks' contrast, and the picture's
/// dim worked out again from them.
pub fn finish(spec: &Spec, colours: Vec<(&'static str, u32)>) -> Derived {
    let dark = is_dark(spec);
    let mut derived = Derived {
        dark,
        base: if dark { "yap" } else { "catppuccin-latte" },
        colours,
        dim: f32::from(spec.dim.min(100)) / 100.0 * 0.85,
        glass_opacity: if dark { 0.55 } else { 0.62 },
    };
    let get = |derived: &Derived, token: &str| derived.colour(token).unwrap_or(0xff00_0000);
    let accent = get(&derived, "accent");
    // On the accent: white, or a dark ink in its hue, whichever reads.
    let [_, _, accent_hue] = lch(accent & 0xff_ffff);
    let dark_ink = from_lch([0.2, 0.03, accent_hue]);
    let on_accent = [0xffffff, dark_ink, 0x000000]
        .into_iter()
        .max_by(|one, two| {
            let score = |ink: u32| {
                let reads = worst(ink, &[accent, get(&derived, "accent-hover")]);
                // White when it reads: it's the brighter look on most accents.
                reads
                    + if ink == 0xffffff && reads >= TEXT {
                        2.0
                    } else {
                        0.0
                    }
            };
            score(*one).total_cmp(&score(*two))
        })
        .unwrap_or(0xffffff);
    derived.set("on-accent", opaque(on_accent));
    // The theme-only grounds, set to what they'd follow, so the checks and
    // the window agree on them.
    let raised = get(&derived, "raised");
    let field = get(&derived, "field");
    let panel = get(&derived, "panel");
    let sidebar = get(&derived, "sidebar");
    derived.set("menu", raised);
    derived.set("composer", field);
    derived.set("code", panel);
    derived.set("embed", sidebar);
    derived.set("account", panel);
    derived.set("actions", raised);
    let mut surfaces: Vec<u32> = ["sidebar", "panel", "chat", "field"]
        .iter()
        .map(|token| get(&derived, token))
        .collect();
    // A row under the pointer shows its wash over the ground it's on, and
    // its text has to read there too.
    let mut lit = Vec::new();
    for ground in ["sidebar", "panel", "chat", "field", "raised"] {
        for wash in WASHES {
            lit.push(washed(get(&derived, wash), get(&derived, ground)));
        }
    }
    surfaces.extend(lit);
    let mut under_text = surfaces.clone();
    under_text.extend(
        [
            "raised", "menu", "composer", "code", "embed", "account", "actions", "active",
            "tooltip",
        ]
        .iter()
        .map(|token| get(&derived, token)),
    );
    let lighter = dark;
    let text = nudge(get(&derived, "text"), &under_text, TEXT, lighter);
    derived.set("text", opaque(text));
    let mut under_quiet = surfaces.clone();
    under_quiet.push(get(&derived, "account"));
    for token in ["secondary", "muted"] {
        let ink = nudge(get(&derived, token), &under_quiet, TEXT, lighter);
        derived.set(token, opaque(ink));
    }
    let link = nudge(get(&derived, "link"), &surfaces, TEXT, lighter);
    derived.set("link", opaque(link));
    // The accent as text or an icon: the accent itself where it reads, a
    // little lighter or darker where it doesn't.
    let accent_text = nudge(accent & 0xff_ffff, &surfaces, TEXT, lighter);
    derived.set("accent-text", opaque(accent_text));
    // Faint stays faint, but never vanishes.
    let faint = nudge(
        get(&derived, "faint"),
        &[get(&derived, "sidebar")],
        2.6,
        lighter,
    );
    derived.set("faint", opaque(faint));
    keep_art_readable(spec, &mut derived);
    derived
}

/// The colour a picture's average shows as once dimmed.
fn dimmed(mean: u32, dim: f32) -> u32 {
    let scale =
        |shift: u32| ((f32::from(((mean >> shift) & 0xff) as u8)) * (1.0 - dim)).round() as u32;
    0xff00_0000 | (scale(16) << 16) | (scale(8) << 8) | scale(0)
}

/// Mixes `under` and `over` as a glass pane does, `amount` of `over`.
fn mix(under: u32, over: u32, amount: f32) -> u32 {
    let channel = |shift: u32| {
        let (under, over) = (
            ((under >> shift) & 0xff) as f32,
            ((over >> shift) & 0xff) as f32,
        );
        (under * (1.0 - amount) + over * amount).round() as u32
    };
    0xff00_0000 | (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// A wash, `0xAARRGGBB`, laid over an opaque `under`, as the window draws it.
fn washed(wash: u32, under: u32) -> u32 {
    mix(under, wash, (wash >> 24) as f32 / 255.0)
}

/// What the art under text averages to: `(mean of the art, dimmed)`, and the
/// art's own mean, if the theme has art.
pub fn art_mean(spec: &Spec) -> Option<u32> {
    match (&spec.picture, spec.background) {
        (Some(picture), Background::Picture) => Some(picture.mean),
        _ if spec.glass => Some(backdrop_mean(spec)),
        _ => None,
    }
}

/// Raises the picture's dim, and glass's cover, until text reads over the
/// art as `check::art` measures it.
fn keep_art_readable(spec: &Spec, derived: &mut Derived) {
    let Some(mean) = art_mean(spec) else {
        return;
    };
    let picture = spec.background == Background::Picture && spec.picture.is_some();
    if !picture {
        // The backdrop is made to read; it isn't dimmed.
        derived.dim = 0.0;
    }
    let inks: Vec<(u32, f64)> = [("text", TEXT), ("secondary", HINT), ("muted", HINT)]
        .iter()
        .filter_map(|(token, needed)| Some((derived.colour(token)?, *needed)))
        .collect();
    let grounds: Vec<u32> = ["sidebar", "chat", "panel"]
        .iter()
        .filter_map(|token| derived.colour(token))
        .collect();
    let washes: Vec<u32> = WASHES
        .iter()
        .filter_map(|token| derived.colour(token))
        .collect();
    let reads = |derived: &Derived| {
        let art = dimmed(mean, derived.dim);
        let mut unders: Vec<u32> = if spec.glass {
            grounds
                .iter()
                .map(|ground| mix(art, *ground, derived.glass_opacity))
                .collect()
        } else {
            vec![art]
        };
        // A hovered row's wash lies over the art as well.
        let lit: Vec<u32> = unders
            .iter()
            .flat_map(|under| washes.iter().map(move |wash| washed(*wash, *under)))
            .collect();
        unders.extend(lit);
        inks.iter()
            .all(|(ink, needed)| worst(*ink, &unders) >= *needed)
    };
    for _ in 0..60 {
        if reads(derived) {
            return;
        }
        if picture && derived.dark && derived.dim < 0.95 {
            derived.dim = (derived.dim + 0.02).min(0.95);
        } else if derived.glass_opacity < 0.95 {
            derived.glass_opacity = (derived.glass_opacity + 0.02).min(0.95);
        } else {
            break;
        }
    }
}

/// The two colours of the backdrop glass shows without a picture.
fn backdrop_colours(spec: &Spec) -> (u32, u32, u32) {
    let dark = is_dark(spec);
    let [_, chroma, hue] = lch(spec.accent & 0xff_ffff);
    let [_, _, ground_hue] = lch(spec.ground);
    let chroma = chroma.clamp(0.04, 0.12);
    if dark {
        (
            from_lch([0.2, 0.03, ground_hue]),
            from_lch([0.42, chroma, hue]),
            from_lch([0.34, chroma * 0.8, (hue + 45.0) % 360.0]),
        )
    } else {
        (
            from_lch([0.95, 0.015, ground_hue]),
            from_lch([0.86, chroma * 0.6, hue]),
            from_lch([0.9, chroma * 0.5, (hue + 45.0) % 360.0]),
        )
    }
}

const BACKDROP_SIZE: (u32, u32) = (640, 400);

/// The backdrop as pixels: the ground, with two soft glows of the accent.
fn backdrop_pixels(spec: &Spec) -> image::RgbImage {
    let (ground, one, two) = backdrop_colours(spec);
    let rgb = |colour: u32| [16, 8, 0].map(|shift| ((colour >> shift) & 0xff) as f32);
    let (ground, one, two) = (rgb(ground), rgb(one), rgb(two));
    let (width, height) = BACKDROP_SIZE;
    image::RgbImage::from_fn(width, height, |x, y| {
        let (u, v) = (x as f32 / width as f32, y as f32 / height as f32);
        let glow = |cx: f32, cy: f32, radius: f32| {
            let distance = ((u - cx).powi(2) + ((v - cy) * 0.62).powi(2)).sqrt() / radius;
            (1.0 - distance).clamp(0.0, 1.0).powf(1.6)
        };
        let (first, second) = (glow(0.18, 0.2, 0.75), glow(0.86, 0.85, 0.7));
        image::Rgb(std::array::from_fn(|channel| {
            let value = ground[channel]
                + (one[channel] - ground[channel]) * first
                + (two[channel] - ground[channel]) * second * (1.0 - first * 0.5);
            value.clamp(0.0, 255.0).round() as u8
        }))
    })
}

/// The backdrop as a JPEG for the theme's folder.
pub fn backdrop_jpeg(spec: &Spec) -> Vec<u8> {
    let mut bytes = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 92);
    backdrop_pixels(spec)
        .write_with_encoder(encoder)
        .expect("a JPEG encodes into memory");
    bytes
}

/// The backdrop's average, as the checks will sample its JPEG.
fn backdrop_mean(spec: &Spec) -> u32 {
    let picture = image::DynamicImage::ImageRgb8(backdrop_pixels(spec));
    sample_mean(&picture)
}

/// The mean of a picture's 16 × 16 sample, as `check::art` reads it.
fn sample_mean(picture: &image::DynamicImage) -> u32 {
    let small = picture.thumbnail_exact(16, 16).to_rgb8();
    let mut sum = [0.0f32; 3];
    for pixel in small.pixels() {
        for (total, value) in sum.iter_mut().zip(pixel.0) {
            *total += f32::from(value) / 255.0;
        }
    }
    let count = (small.width() * small.height()).max(1) as f32;
    let byte = |total: f32| ((total / count).clamp(0.0, 1.0) * 255.0).round() as u32;
    (byte(sum[0]) << 16) | (byte(sum[1]) << 8) | byte(sum[2])
}

// Pictures.

/// A picture made ready for a theme: re-encoded small enough to ship, its
/// colours read, and a thumbnail for the creator.
pub struct PreparedPicture {
    pub jpeg: Vec<u8>,
    pub picture: Picture,
    /// A suggested accent and ground from it.
    pub accent: u32,
    pub ground: u32,
    /// RGBA, about 480 pixels wide.
    pub thumbnail: image::RgbaImage,
}

/// The largest picture a creator theme keeps, on its long side.
const PICTURE_SIDE: u32 = 1920;

/// Reads any picture the person chose: decodes it under limits, scales it
/// to at most 1920 pixels, re-encodes it as a JPEG under the themes' 2 MiB,
/// and reads its colours.
pub fn prepare_picture(bytes: &[u8]) -> Result<PreparedPicture, String> {
    let refused = || "That file isn't a picture Yapfast can use".to_owned();
    let format = image::guess_format(bytes).map_err(|_| refused())?;
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(512 << 20);
    reader.limits(limits);
    let picture = reader.decode().map_err(|_| refused())?;
    let picture = if picture.width().max(picture.height()) > PICTURE_SIDE {
        picture.resize(
            PICTURE_SIDE,
            PICTURE_SIDE,
            image::imageops::FilterType::Triangle,
        )
    } else {
        picture
    };
    let rgb = image::DynamicImage::ImageRgb8(picture.to_rgb8());
    let mut quality = 88;
    let jpeg = loop {
        let mut bytes = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality);
        rgb.write_with_encoder(encoder).map_err(|_| refused())?;
        if bytes.len() as u64 <= MAX_IMAGE_BYTES || quality <= 40 {
            break bytes;
        }
        quality -= 12;
    };
    // Read back as the checks will read it.
    let stored = image::load_from_memory(&jpeg).map_err(|_| refused())?;
    let mean = sample_mean(&stored);
    let found = palette(&stored);
    let thumbnail = stored.thumbnail(480, 480).to_rgba8();
    Ok(PreparedPicture {
        jpeg,
        picture: Picture {
            mean,
            palette: found.colours,
        },
        accent: found.accent,
        ground: found.ground,
        thumbnail,
    })
}

/// A picture's colours, as [`palette`] reads them.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// Up to six colours made usable as accents, the most used first.
    pub colours: Vec<u32>,
    /// The one that makes the best accent: lively and well used.
    pub accent: u32,
    /// The one whose hue tints the grounds: its darkest well-used colour.
    pub ground: u32,
}

/// Finds a picture's main colours with k-means in OKLab over a 64 × 64
/// sample, seeded the same way every time, so a picture always gives the
/// same palette.
pub fn palette(picture: &image::DynamicImage) -> Palette {
    let small = picture.thumbnail_exact(64, 64).to_rgb8();
    let points: Vec<[f64; 3]> = small
        .pixels()
        .map(|pixel| {
            let colour = (u32::from(pixel.0[0]) << 16)
                | (u32::from(pixel.0[1]) << 8)
                | u32::from(pixel.0[2]);
            let [lightness, chroma, hue] = lch(colour);
            [
                lightness,
                chroma * hue.to_radians().cos(),
                chroma * hue.to_radians().sin(),
            ]
        })
        .collect();
    let distance = |one: &[f64; 3], two: &[f64; 3]| {
        (0..3)
            .map(|axis| (one[axis] - two[axis]).powi(2))
            .sum::<f64>()
    };
    // Seeds: the mean, then each time the point farthest from every seed.
    const K: usize = 6;
    let mean = points.iter().fold([0.0; 3], |sum, point| {
        std::array::from_fn(|axis| sum[axis] + point[axis] / points.len() as f64)
    });
    let mut centres = vec![mean];
    while centres.len() < K {
        let farthest = points
            .iter()
            .max_by(|one, two| {
                let near = |point: &[f64; 3]| {
                    centres
                        .iter()
                        .map(|centre| distance(point, centre))
                        .fold(f64::INFINITY, f64::min)
                };
                near(one).total_cmp(&near(two))
            })
            .copied()
            .unwrap_or(mean);
        centres.push(farthest);
    }
    let mut counts = vec![0usize; K];
    for _ in 0..12 {
        let mut sums = [[0.0; 3]; K];
        counts = vec![0; K];
        for point in &points {
            let nearest = (0..K)
                .min_by(|one, two| {
                    distance(point, &centres[*one]).total_cmp(&distance(point, &centres[*two]))
                })
                .unwrap_or(0);
            counts[nearest] += 1;
            for axis in 0..3 {
                sums[nearest][axis] += point[axis];
            }
        }
        for index in 0..K {
            if counts[index] > 0 {
                centres[index] = sums[index].map(|sum| sum / counts[index] as f64);
            }
        }
    }
    let total = points.len().max(1) as f64;
    let mut clusters: Vec<([f64; 3], f64)> = (0..K)
        .filter(|index| counts[*index] > 0)
        .map(|index| {
            let [lightness, a, b] = centres[index];
            (
                [
                    lightness,
                    a.hypot(b),
                    b.atan2(a).to_degrees().rem_euclid(360.0),
                ],
                counts[index] as f64 / total,
            )
        })
        .collect();
    clusters.sort_by(|one, two| two.1.total_cmp(&one.1));
    // As accents: lively and light enough to read on a dark ground.
    let usable = |[lightness, chroma, hue]: [f64; 3]| {
        from_lch([lightness.clamp(0.62, 0.82), chroma.clamp(0.07, 0.2), hue])
    };
    let mut colours: Vec<u32> = Vec::new();
    for (colour, _) in &clusters {
        let candidate = usable(*colour);
        let [l1, c1, h1] = lch(candidate);
        let close = colours.iter().any(|kept| {
            let [l2, c2, h2] = lch(*kept);
            let hue = ((h1 - h2 + 540.0) % 360.0 - 180.0).abs();
            (l1 - l2).abs() < 0.05 && (c1 - c2).abs() < 0.04 && (hue < 18.0 || c1.min(c2) < 0.03)
        });
        if !close {
            colours.push(candidate);
        }
    }
    let accent = clusters
        .iter()
        .max_by(|(one, weight_one), (two, weight_two)| {
            let score = |[lightness, chroma, _]: [f64; 3], weight: f64| {
                let light = 1.0 - ((lightness - 0.68).abs() * 1.5).min(0.9);
                chroma * weight.sqrt().max(0.12) * light
            };
            score(*one, *weight_one).total_cmp(&score(*two, *weight_two))
        })
        .map_or(0xffbf47, |(colour, _)| {
            let [lightness, chroma, hue] = *colour;
            from_lch([lightness.clamp(0.66, 0.8), chroma.clamp(0.1, 0.2), hue])
        });
    let ground = clusters
        .iter()
        .filter(|(_, weight)| *weight >= 0.05)
        .min_by(|(one, _), (two, _)| one[0].total_cmp(&two[0]))
        .map_or(0x1e1f24, |(colour, _)| from_lch(*colour));
    Palette {
        colours,
        accent,
        ground,
    }
}

// Fonts.

/// What the creator learns of a font the person brings.
#[derive(Clone, Debug, PartialEq)]
pub struct FontFacts {
    pub family: String,
    pub licence: Option<String>,
    /// Some of basic Latin is missing: those characters draw in the default
    /// font.
    pub partial: bool,
    /// `ttf` or `otf`.
    pub extension: &'static str,
}

/// What a font that isn't one gets told.
pub const NOT_A_FONT: &str = "That file isn't a font Yapfast can use";

/// Checks a font the person chose, as a theme's fonts are checked, and reads
/// its family, licence and how much of basic Latin it draws.
pub fn inspect_font(bytes: &[u8]) -> Result<FontFacts, String> {
    if bytes.len() as u64 > MAX_FONT_BYTES {
        return Err(format!(
            "That font is larger than {} MB, the most a theme can carry",
            MAX_FONT_BYTES >> 20
        ));
    }
    check::check_font(bytes).map_err(|reason| {
        if reason.contains("Latin") {
            "That font has no Latin letters, so Yapfast can't draw text with it".to_owned()
        } else {
            NOT_A_FONT.to_owned()
        }
    })?;
    let read = std::panic::catch_unwind(|| {
        let face = ttf_parser::Face::parse(bytes, 0).ok()?;
        let name = |id: u16| {
            face.names()
                .into_iter()
                .filter(|name| name.name_id == id)
                .find_map(|name| name.to_string())
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
        };
        let family = name(ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
            .or_else(|| name(ttf_parser::name_id::FAMILY))?;
        let licence =
            name(ttf_parser::name_id::LICENSE).or_else(|| name(ttf_parser::name_id::LICENSE_URL));
        let partial =
            (0x20u8..0x7f).any(|byte| face.glyph_index(char::from(byte)).is_none() && byte != b' ');
        Some((family, licence, partial))
    });
    let Ok(Some((family, licence, partial))) = read else {
        return Err(NOT_A_FONT.to_owned());
    };
    let extension = if bytes.starts_with(b"OTTO") {
        "otf"
    } else {
        "ttf"
    };
    Ok(FontFacts {
        family,
        licence,
        partial,
        extension,
    })
}

/// A short name for a file: lowercase letters, digits and `-`.
pub fn slug(text: &str) -> String {
    let slug = text
        .to_lowercase()
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() {
                letter
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "font".to_owned()
    } else {
        slug.chars().take(48).collect()
    }
}

/// Copies a checked font into a theme's `fonts/`, and returns it as the spec
/// keeps it. A font already there under the same name is replaced.
pub fn store_font(folder: &Path, bytes: &[u8]) -> Result<ImportedFont, String> {
    let facts = inspect_font(bytes)?;
    let file = format!("fonts/{}.{}", slug(&facts.family), facts.extension);
    write_file(&folder.join(&file), bytes)
        .map_err(|error| format!("The font couldn't be kept: {error}"))?;
    Ok(ImportedFont {
        file,
        family: facts.family,
        licence: facts.licence,
        partial: facts.partial,
    })
}

/// Keeps a prepared picture in a theme's folder.
pub fn store_picture(folder: &Path, jpeg: &[u8]) -> std::io::Result<()> {
    write_file(&folder.join(PICTURE), jpeg)
}

/// Writes a file whole, through a file beside it, so that one read meanwhile
/// sees the old file or the new one, never half.
fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = path.with_extension("partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path)
}

// The package.

/// A radius at `corners` (0–100), `stock` at 50, square at 0 and `round` at
/// 100.
fn radius(corners: u8, stock: f64, round: f64) -> f64 {
    let t = f64::from(corners.min(100)) / 100.0;
    let value = if t <= 0.5 {
        stock * t / 0.5
    } else {
        stock + (round - stock) * (t - 0.5) / 0.5
    };
    (value * 2.0).round() / 2.0
}

fn hex(colour: u32) -> String {
    match colour >> 24 {
        0xff => format!("#{:06x}", colour & 0xff_ffff),
        alpha => format!("#{:06x}{alpha:02x}", colour & 0xff_ffff),
    }
}

/// The `shape` for a spec's corners and outlines; none at Yapfast's own.
fn shape(spec: &Spec) -> Option<Value> {
    let outline = spec.outline.min(4);
    if spec.corners == 50 {
        return (outline != 1).then(|| json!({ "outline_width": outline }));
    }
    let corners = spec.corners;
    let round = corners >= 90;
    let mut shape = serde_json::Map::new();
    for (surface, stock, most) in [
        ("button", 6.0, 18.0),
        ("field", 6.0, 18.0),
        ("composer", 10.0, 26.0),
        ("card", 12.0, 22.0),
        ("menu", 8.0, 16.0),
        ("dialog", 12.0, 24.0),
        ("tooltip", 6.0, 12.0),
        ("chip", 8.0, 14.0),
        ("message", 4.0, 12.0),
        ("rail-icon", 16.0, 24.0),
        ("tile", 10.0, 18.0),
        ("code", 4.0, 12.0),
        ("embed", 4.0, 14.0),
        ("actions", 8.0, 14.0),
    ] {
        let value = if round && matches!(surface, "button" | "chip" | "field") {
            999.0
        } else {
            radius(corners, stock, most)
        };
        shape.insert(surface.to_owned(), json!({ "radius": value }));
    }
    shape.insert(
        "avatar".to_owned(),
        json!({ "radius": radius(corners, 50.0, 50.0) }),
    );
    shape.insert(
        "radii".to_owned(),
        json!({
            "small": radius(corners, 4.0, 8.0),
            "medium": radius(corners, 8.0, 14.0),
            "large": radius(corners, 12.0, 20.0),
        }),
    );
    if outline != 1 {
        shape.insert("outline_width".to_owned(), json!(outline));
    }
    Some(Value::Object(shape))
}

/// A font key's file in the theme's folder, if it has one.
pub fn font_file(spec: &Spec, key: &str) -> Option<String> {
    if let Some(font) = FONTS.iter().find(|font| font.key == key) {
        return font.file.map(|file| format!("fonts/{file}"));
    }
    spec.fonts
        .iter()
        .find(|font| font.file == key)
        .map(|font| font.file.clone())
}

/// Every built-in font file a spec's theme ships, by its name in
/// `ui/fonts/`.
pub fn built_in_files(spec: &Spec) -> Vec<&'static str> {
    let mut files = Vec::new();
    for key in std::iter::once(spec.body.as_str()).chain(spec.headings.as_deref()) {
        if let Some(file) = FONTS
            .iter()
            .find(|font| font.key == key)
            .and_then(|font| font.file)
            && !files.contains(&file)
        {
            files.push(file);
        }
    }
    files
}

/// The theme's `theme.json`: its id is `local.creator.<id>`.
pub fn manifest(spec: &Spec, name: &str, id: &str, derived: &Derived) -> String {
    let mode = if derived.dark { "dark" } else { "light" };
    let colours: serde_json::Map<String, Value> = derived
        .colours
        .iter()
        .map(|(token, colour)| ((*token).to_owned(), Value::String(hex(*colour))))
        .collect();
    let mut theme = json!({
        "theme_api": THEME_API,
        "id": format!("local.creator.{}", slug(id)),
        "name": if name.trim().is_empty() { "My theme" } else { name.trim() },
        "version": "1.0.0",
        "description": description(spec),
        "authors": [],
        "license": "CC0-1.0 AND OFL-1.1",
        "source": "",
        "preview": [],
        "variants": { mode: { "base": derived.base, "colours": colours } },
    });
    let object = theme.as_object_mut().expect("an object");
    if let Some(shape) = shape(spec) {
        object.insert("shape".to_owned(), shape);
    }
    let mut types = serde_json::Map::new();
    let body = font_file(spec, &spec.body);
    let headings = match &spec.headings {
        None => body.clone(),
        Some(key) => font_file(spec, key),
    };
    if let Some(file) = &body {
        types.insert("body".to_owned(), json!({ "font": file }));
        types.insert("labels".to_owned(), json!({ "font": file }));
    }
    if let Some(file) = &headings {
        types.insert("headings".to_owned(), json!({ "font": file }));
    }
    if !types.is_empty() {
        object.insert("type".to_owned(), Value::Object(types));
    }
    match spec.density {
        Density::Cozy => {}
        Density::Compact => {
            object.insert("spacing".to_owned(), json!({ "density": "compact" }));
        }
        Density::Roomy => {
            object.insert("spacing".to_owned(), json!({ "density": "roomy" }));
        }
    }
    let layout = match spec.messages {
        Messages::Flat => None,
        Messages::Bubbles => Some(json!({
            "messages": "bubbles",
            "bubbles": { "max_width": 72, "radius": radius(spec.corners, 16.0, 22.0) },
        })),
        Messages::Cards => Some(json!({
            "messages": "cards",
            "cards": { "tilt": 0, "gap": 8, "padding": 12,
                       "shadow": { "blur": 10, "y": 2, "colour": if derived.dark { "#00000040" } else { "#0000001a" } } },
        })),
        Messages::Irc => Some(json!({
            "messages": "irc",
            "chat_avatars": false,
            "irc": { "time_width": 52, "nick_width": 120, "separator": "│" },
        })),
    };
    if let Some(layout) = layout {
        object.insert("layout".to_owned(), layout);
    }
    let picture = spec.background == Background::Picture && spec.picture.is_some();
    let blur = (f64::from(spec.blur.min(100)) / 100.0 * 48.0).round();
    let dim = (f64::from(derived.dim) * 100.0).round() / 100.0;
    let mut art = serde_json::Map::new();
    if spec.glass {
        let image = if picture { PICTURE } else { BACKDROP };
        let mut window = json!({ "image": image, "fit": "cover" });
        if picture && dim > 0.0 {
            window["dim"] = json!(dim);
        }
        art.insert("surfaces".to_owned(), json!({ "window": [window] }));
        let opacity = (f64::from(derived.glass_opacity) * 100.0).round() / 100.0;
        art.insert(
            "glass".to_owned(),
            json!({
                "surfaces": ["rail", "sidebar", "chat", "members", "account"],
                "blur": if picture { blur.max(8.0) } else { 24.0 },
                "opacity": opacity,
                "gap": 10,
                "radius": radius(spec.corners, 12.0, 24.0).min(32.0),
                "highlight": if derived.dark { "#ffffff14" } else { "#ffffff99" },
            }),
        );
    } else if picture {
        let mut layer = json!({ "image": PICTURE, "fit": "cover" });
        if blur > 0.0 {
            layer["blur"] = json!(blur);
        }
        if dim > 0.0 {
            layer["dim"] = json!(dim);
        }
        art.insert("surfaces".to_owned(), json!({ "chat": [layer] }));
    }
    if !art.is_empty() {
        object.insert("art".to_owned(), Value::Object(art));
    }
    let mut text = serde_json::to_string_pretty(&theme).expect("JSON values serialise");
    text.push('\n');
    text
}

/// What the theme says of itself: made with the creator, and its fonts.
fn description(spec: &Spec) -> String {
    let mut fonts: Vec<String> = Vec::new();
    for key in std::iter::once(spec.body.as_str()).chain(spec.headings.as_deref()) {
        if let Some(font) = FONTS
            .iter()
            .find(|font| font.key == key && font.file.is_some())
        {
            fonts.push(format!("{} (OFL-1.1)", font.typeface));
        } else if let Some(font) = spec.fonts.iter().find(|font| font.file == key) {
            fonts.push(match &font.licence {
                Some(licence) => format!("{} ({})", font.family, short(licence, 60)),
                None => format!("{} (licence not stated)", font.family),
            });
        }
    }
    fonts.dedup();
    let mut text = "Made with Yapfast's theme creator.".to_owned();
    if !fonts.is_empty() {
        text.push_str(" Fonts: ");
        text.push_str(&fonts.join(", "));
        text.push('.');
    }
    short(&text, 300)
}

fn short(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(most - 1).collect();
    cut.push('…');
    cut
}

/// The theme's `LICENSE`: its fonts' licences, and its own.
pub fn licence(spec: &Spec) -> String {
    let mut text = String::from(
        "This theme's colours and settings: CC0-1.0, no rights reserved.\n\n\
         Only share themes with fonts and pictures you're allowed to share.\n",
    );
    for key in std::iter::once(spec.body.as_str()).chain(spec.headings.as_deref()) {
        if let Some(font) = FONTS
            .iter()
            .find(|font| font.key == key && font.file.is_some())
        {
            let line = format!(
                "\nfonts/{}: {}, under the SIL Open Font License 1.1 (https://openfontlicense.org).\n",
                font.file.unwrap_or_default(),
                font.typeface
            );
            if !text.contains(&line) {
                text.push_str(&line);
            }
        } else if let Some(font) = spec.fonts.iter().find(|font| font.file == key) {
            let line = format!(
                "\n{}: {}, {}\n",
                font.file,
                font.family,
                font.licence
                    .as_deref()
                    .unwrap_or("whose licence its file doesn't state.")
            );
            if !text.contains(&line) {
                text.push_str(&line);
            }
        }
    }
    if spec.background == Background::Picture && spec.picture.is_some() {
        text.push_str(&format!(
            "\n{PICTURE}: chosen by the theme's maker; its rights are theirs to check.\n"
        ));
    }
    text
}

/// Writes a creator theme's folder: `theme.json`, `LICENSE`, the built-in
/// fonts it names (from `font_bytes`, by file name) and the backdrop glass
/// shows. Its picture and imported fonts are already there, from
/// [`store_picture`] and [`store_font`]; files it no longer names are left,
/// since nothing reads them.
pub fn write_folder(
    folder: &Path,
    spec: &Spec,
    name: &str,
    id: &str,
    derived: &Derived,
    font_bytes: impl Fn(&str) -> Option<&'static [u8]>,
) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    for file in built_in_files(spec) {
        let path = folder.join("fonts").join(file);
        if !path.exists()
            && let Some(bytes) = font_bytes(file)
        {
            write_file(&path, bytes)?;
        }
    }
    if spec.glass && !(spec.background == Background::Picture && spec.picture.is_some()) {
        write_file(&folder.join(BACKDROP), &backdrop_jpeg(spec))?;
    }
    write_file(&folder.join("LICENSE"), licence(spec).as_bytes())?;
    write_file(
        &folder.join(MANIFEST),
        manifest(spec, name, id, derived).as_bytes(),
    )
}

/// Copies one theme folder over another, file by file: the draft into a
/// saved theme, or a saved theme into the draft or an export.
pub fn copy_folder(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        std::fs::remove_dir_all(to)?;
    }
    std::fs::create_dir_all(to)?;
    let mut stack: Vec<PathBuf> = vec![PathBuf::new()];
    while let Some(relative) = stack.pop() {
        for entry in std::fs::read_dir(from.join(&relative))? {
            let entry = entry?;
            let path = relative.join(entry.file_name());
            let kind = entry.file_type()?;
            if kind.is_dir() {
                std::fs::create_dir_all(to.join(&path))?;
                stack.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .is_none_or(|extension| extension != "partial")
            {
                std::fs::copy(from.join(&path), to.join(&path))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
