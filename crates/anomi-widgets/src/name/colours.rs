//! A style's colours as Discord's client makes them: each colour the person
//! picked is lightened or darkened until it reads against the ground (`lZ`
//! in the client), and the shades the effects draw with (`$0`) are made
//! from the first, off the HSL maths of chroma.js, which the client uses.
//! One set for dark themes and one for light. Ported from
//! `src/ui/name_style.rs:13-146` and `palette` (`:220`).

use iced::Color;

use super::style::{Effect, Style};

/// The member list's ground on a dark and on a light theme, which colours
/// are made to read against (`src/ui/name_style.rs:16`). A toon name reads
/// against `TOON_GROUND` on either, as its letters are edged.
const DARK_GROUND: u32 = 0x12_12_14;
const LIGHT_GROUND: u32 = 0xfb_fb_fb;
const TOON_GROUND: u32 = 0x33_33_33;

/// The contrast a colour keeps against its ground: gradients and prisms,
/// whose letters are lit by the colours around them, need less.
fn contrast_needed(effect: Effect) -> f64 {
    match effect {
        Effect::Gradient | Effect::Glow | Effect::Prism => 2.5,
        _ => 3.0,
    }
}

type Rgb = (f64, f64, f64);

fn unpack(rgb: u32) -> Rgb {
    (
        f64::from((rgb >> 16) as u8),
        f64::from((rgb >> 8) as u8),
        f64::from(rgb as u8),
    )
}

fn pack((r, g, b): Rgb) -> u32 {
    let byte = |value: f64| u32::from(value.round().clamp(0., 255.) as u8);
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

/// Hue in degrees, saturation and lightness from 0 to 1.
fn to_hsl((r, g, b): Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (r / 255., g / 255., b / 255.);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.;
    let delta = max - min;
    if delta == 0. {
        return (0., 0., lightness);
    }
    let saturation = delta / (1. - (2. * lightness - 1.).abs());
    let hue = if max == r {
        ((g - b) / delta).rem_euclid(6.)
    } else if max == g {
        (b - r) / delta + 2.
    } else {
        (r - g) / delta + 4.
    };
    (hue * 60., saturation, lightness)
}

fn from_hsl(hue: f64, saturation: f64, lightness: f64) -> Rgb {
    let chroma = (1. - (2. * lightness - 1.).abs()) * saturation;
    let sector = hue.rem_euclid(360.) / 60.;
    let x = chroma * (1. - (sector % 2. - 1.).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.),
        1 => (x, chroma, 0.),
        2 => (0., chroma, x),
        3 => (0., x, chroma),
        4 => (x, 0., chroma),
        _ => (chroma, 0., x),
    };
    let m = lightness - chroma / 2.;
    ((r + m) * 255., (g + m) * 255., (b + m) * 255.)
}

fn luminance((r, g, b): Rgb) -> f64 {
    let linear = |value: f64| {
        let value = value / 255.;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `color` made lighter on a dark `ground` and darker on a light one, five
/// percent of lightness at a time and at most ten times, until it has
/// `ratio` against `ground`.
fn readable(color: u32, ground: u32, ratio: f64) -> u32 {
    let ground = unpack(ground);
    let lighter = luminance(ground) <= 0.5;
    let mut color = unpack(color);
    for _ in 0..10 {
        if contrast(color, ground) >= ratio {
            break;
        }
        let (hue, saturation, lightness) = to_hsl(color);
        if lighter && lightness >= 0.95 || !lighter && lightness <= 0.05 {
            break;
        }
        let step = if lighter { 0.05 } else { -0.05 };
        color = unpack(pack(from_hsl(hue, saturation, lightness + step)));
    }
    pack(color)
}

fn color(rgb: u32) -> Color {
    anomi_theme::rgb(rgb)
}

/// What a style draws with on one ground: its colours made to read, and
/// the shades the solid, neon, toon and pop effects take from the first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Colours {
    /// How many colours the style gave, one to five.
    pub count: usize,
    /// Those colours, the first repeated past `count`.
    pub colors: [Color; 5],
    pub main: Color,
    pub light1: Color,
    pub light2: Color,
    pub dark1: Color,
    pub dark2: Color,
    /// Toon's edge.
    pub toon: Color,
    /// Neon's edge.
    pub neon: Color,
}

impl Colours {
    /// `style`'s colours on a dark theme's ground or a light one's; none
    /// for a style that keeps only its face.
    pub fn of(style: &Style, dark: bool) -> Option<Self> {
        let given = style.colors();
        let first = *given.first()?;
        let effect = style.effect();
        let ground = match (effect, dark) {
            (Effect::Toon, _) => TOON_GROUND,
            (_, true) => DARK_GROUND,
            (_, false) => LIGHT_GROUND,
        };
        let ratio = contrast_needed(effect);
        let main = readable(first, ground, ratio);
        let mut colors = [color(main); 5];
        for (slot, rgb) in colors.iter_mut().zip(given) {
            *slot = color(readable(*rgb, ground, ratio));
        }
        let (hue, saturation, lightness) = to_hsl(unpack(main));
        let shade =
            |lightness: f64| color(pack(from_hsl(hue, saturation, lightness.clamp(0., 1.))));
        Some(Self {
            count: given.len(),
            colors,
            main: color(main),
            light1: shade((1.2 * lightness).min(1.)),
            light2: shade((1.6 * lightness).min(1.)),
            dark1: shade((0.6 * lightness).max(0.)),
            dark2: shade((0.2 * lightness).max(0.)),
            toon: shade((0.4 * lightness).max(0.12)),
            neon: color(pack(from_hsl(
                hue,
                (1.2 * saturation).min(1.),
                (lightness + 0.1).min(0.6),
            ))),
        })
    }

    /// The colour of a gummy name's letter, the `turn`th that isn't a
    /// space: the colours in turn.
    pub fn letter(&self, turn: usize) -> Color {
        self.colors[turn % self.count.max(1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(effect: u8, colors: &[u32]) -> Style {
        Style::new(11, effect, colors).expect("a colour")
    }

    fn rgb(color: Color) -> u32 {
        let [r, g, b, _] = color.into_rgba8();
        (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
    }

    #[test]
    fn colours_change_until_they_read_against_their_ground() {
        // Black on the dark ground is lifted to the contrast asked for, in
        // steps of five percent.
        let lifted = readable(0x000000, DARK_GROUND, 3.);
        assert!(contrast(unpack(lifted), unpack(DARK_GROUND)) >= 3.);
        assert_ne!(lifted, 0x000000);
        // White on a light one is brought down.
        let lowered = readable(0xffffff, LIGHT_GROUND, 3.);
        assert!(contrast(unpack(lowered), unpack(LIGHT_GROUND)) >= 3.);
        // A colour that reads already stays.
        assert_eq!(readable(0x00ff99, DARK_GROUND, 3.), 0x00ff99);
    }

    #[test]
    fn shades_follow_the_clients_hsl_steps() {
        let green = Colours::of(&style(1, &[0x00ff99]), true).expect("colours");
        assert_eq!(rgb(green.main), 0x00ff99);
        // Lightness .5 becomes .6 and .3.
        let lightness = |color: Color| to_hsl(unpack(rgb(color))).2;
        assert!((lightness(green.light1) - 0.6).abs() < 0.01);
        assert!((lightness(green.dark1) - 0.3).abs() < 0.01);
        // Neon's edge is a bit lighter than the colour, at most .6.
        assert!((lightness(green.neon) - 0.6).abs() < 0.01);
    }

    #[test]
    fn a_light_themes_colours_differ_where_the_colour_needs_it() {
        let white = style(1, &[0xffffff]);
        let dark = Colours::of(&white, true).expect("colours");
        let light = Colours::of(&white, false).expect("colours");
        assert_eq!(rgb(dark.main), 0xffffff);
        assert!(
            rgb(light.main) >> 16 < 0xff,
            "white is brought down to read"
        );
        // A toon name reads against the same grey on both.
        let toon = style(4, &[0xffffff]);
        assert_eq!(Colours::of(&toon, true), Colours::of(&toon, false));
    }

    #[test]
    fn the_stops_repeat_the_first_and_letters_take_the_colours_in_turn() {
        let two = Colours::of(&style(8, &[0xff0000, 0x00ff00]), true).expect("colours");
        assert_eq!(two.count, 2);
        assert_eq!(two.colors[2], two.colors[0]);
        assert_eq!(two.letter(0), two.colors[0]);
        assert_eq!(two.letter(1), two.colors[1]);
        assert_eq!(two.letter(2), two.colors[0]);
        // A face alone has no colours.
        assert_eq!(Colours::of(&style(1, &[1]).font_only(), true), None);
    }
}
