//! Display name styles as Discord's client colours them: each colour the
//! person picked is lightened until it reads against the dark background
//! (`lZ` in the client), and the derived shades the effects use (`$0`) are
//! made from that. The colours are computed once per style, off the HSL
//! maths chroma.js does, which the client uses.

use slint::Color;
use yapfast_core::chat::NameStyle;

use super::*;

/// The member list's background, which colours are made to read against.
const GROUND: u32 = 0x121214;
/// The contrast a colour keeps against it: gradients and prisms, whose
/// letters are lit by the colours around them, need less.
fn contrast_needed(effect: u8) -> f64 {
    match effect {
        2 | 6 | 7 => 2.5,
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
    byte(r) << 16 | byte(g) << 8 | byte(b)
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

/// `colour` made lighter, five percent of lightness at a time and at most
/// ten times, until it has `ratio` against `ground`, as the client does on a
/// dark background.
fn readable(color: u32, ground: u32, ratio: f64) -> u32 {
    let ground = unpack(ground);
    let mut color = unpack(color);
    for _ in 0..10 {
        if contrast(color, ground) >= ratio {
            break;
        }
        let (hue, saturation, lightness) = to_hsl(color);
        if lightness >= 0.95 {
            break;
        }
        color = unpack(pack(from_hsl(hue, saturation, lightness + 0.05)));
    }
    pack(color)
}

/// The shades the solid, neon, toon and pop effects draw with, from one
/// colour: `main`, a lighter and a lighter still, a darker and a darker
/// still, and the strokes of toon and neon.
struct Shades {
    main: u32,
    light1: u32,
    light2: u32,
    dark1: u32,
    dark2: u32,
    toon: u32,
    neon: u32,
}

fn shades(main: u32) -> Shades {
    let (hue, saturation, lightness) = to_hsl(unpack(main));
    let with_lightness = |lightness: f64| pack(from_hsl(hue, saturation, lightness.clamp(0., 1.)));
    Shades {
        main,
        light1: with_lightness((1.2 * lightness).min(1.)),
        light2: with_lightness((1.6 * lightness).min(1.)),
        dark1: with_lightness((0.6 * lightness).max(0.)),
        dark2: with_lightness((0.2 * lightness).max(0.)),
        toon: with_lightness((0.4 * lightness).max(0.12)),
        neon: pack(from_hsl(
            hue,
            (1.2 * saturation).min(1.),
            (lightness + 0.1).min(0.6),
        )),
    }
}

fn color(rgb: u32) -> Color {
    Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// The font family Discord's client draws a style's `font_id` in; the usual
/// font, the empty string, for 11, the retired ids and Medieval (7), whose
/// licence isn't confirmed (ui/fonts/README.md).
pub(super) fn family(font: u8) -> &'static str {
    match font {
        3 => "Sakura",
        4 => "Jellybean",
        6 => "Modern",
        8 => "8Bit",
        12 => "Tempo",
        13 => "Monkey Bars",
        14 => "Mainframe",
        15 => "Headbang",
        16 => "Journal",
        _ => "",
    }
}

/// The weight of that font's only face, which the name is drawn in whatever
/// weight the row asks for.
fn weight(font: u8) -> i32 {
    match font {
        6 | 8 => 500,
        12 => 600,
        13 | 14 | 16 => 700,
        _ => 400,
    }
}

/// A display name style as the window draws it; none is the default, with
/// no effect.
pub(super) fn row(style: Option<NameStyle>, name: &str) -> NameStyleRow {
    let Some(style) = style else {
        return NameStyleRow::default();
    };
    let ratio = contrast_needed(style.effect);
    let adjusted: Vec<u32> = style
        .colors()
        .iter()
        .map(|rgb| readable(*rgb, GROUND, ratio))
        .collect();
    let Some(&first) = adjusted.first() else {
        return NameStyleRow::default();
    };
    let shades = shades(first);
    let mut colors: Vec<Color> = adjusted.iter().map(|rgb| color(*rgb)).collect();
    let given = colors.clone();
    // The gradient's stops reach up to five colours.
    colors.resize(5, color(first));
    // A gummy name's letters take the colours in turn, spaces left out of
    // the count.
    let letters: Vec<NameLetter> = if style.effect == 8 {
        let mut turn = 0;
        name.chars()
            .map(|letter| {
                let color = given[turn % given.len()];
                if !letter.is_whitespace() {
                    turn += 1;
                }
                NameLetter {
                    text: letter.to_string().into(),
                    color,
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    NameStyleRow {
        letters: ModelRc::new(VecModel::from(letters)),
        effect: i32::from(style.effect),
        family: family(style.font).into(),
        weight: weight(style.font),
        count: adjusted.len() as i32,
        colors: ModelRc::new(VecModel::from(colors)),
        main: color(shades.main),
        light1: color(shades.light1),
        light2: color(shades.light2),
        dark1: color(shades.dark1),
        dark2: color(shades.dark2),
        toon: color(shades.toon),
        neon: color(shades.neon),
    }
}

/// A name in a Discord server, as Yapfast draws it there: the style's font
/// alone, in the row's own colour, with none of its colours or effect, even
/// while it's pointed at. Profiles, DMs and the style's own editor show the
/// whole style.
/// `in_server` says whether the name is in one.
pub(super) fn font_only_in(in_server: bool, row: NameStyleRow) -> NameStyleRow {
    if !in_server || row.effect == 0 {
        return row;
    }
    // A solid style with no colours is drawn in the row's own ink.
    NameStyleRow {
        effect: 1,
        family: row.family,
        weight: row.weight,
        ..NameStyleRow::default()
    }
}

/// Whether a place, a room's id, is in a Discord server rather than a DM,
/// where names show as [`font_only_in`] says.
pub(super) fn in_server(place: Option<&str>) -> bool {
    place
        .and_then(yapfast_core::discord::parse_room)
        .is_some_and(|(guild, _)| guild != yapfast_core::discord::DM_GUILD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_in_a_server_keep_only_their_font() {
        let style = row(Some(NameStyle::new(14, 7, &[0xff0000, 0x00ff00])), "Ann");
        let plain = font_only_in(true, style.clone());
        assert_eq!((plain.effect, plain.count), (1, 0), "no colours or effect");
        assert_eq!(
            (plain.family.as_str(), plain.weight),
            (style.family.as_str(), style.weight)
        );
        assert_eq!(font_only_in(true, NameStyleRow::default()).effect, 0);
        assert_eq!(
            font_only_in(false, style.clone()).count,
            2,
            "outside, all of it"
        );
        assert!(in_server(Some("discord:10:20")));
        assert!(!in_server(Some("discord:@me:30")));
        assert!(!in_server(None));
    }

    #[test]
    fn dark_colours_are_lightened_until_they_read() {
        // Black on the dark ground is lifted to the contrast asked for, in
        // steps of five percent.
        let lifted = readable(0x000000, GROUND, 3.);
        assert!(contrast(unpack(lifted), unpack(GROUND)) >= 3.);
        assert!(lifted != 0x000000);
        // A colour that reads already stays.
        assert_eq!(readable(0x00ff99, GROUND, 3.), 0x00ff99);
    }

    #[test]
    fn shades_follow_the_clients_hsl_steps() {
        let green = shades(0x00ff99);
        assert_eq!(green.main, 0x00ff99);
        // Lightness .5 becomes .6, .8, .3 and .1.
        let (_, _, light1) = to_hsl(unpack(green.light1));
        let (_, _, dark1) = to_hsl(unpack(green.dark1));
        assert!((light1 - 0.6).abs() < 0.01 && (dark1 - 0.3).abs() < 0.01);
        // Neon's stroke is a bit lighter than the colour, at most .6.
        let (_, _, neon) = to_hsl(unpack(green.neon));
        assert!((neon - 0.6).abs() < 0.01);
    }

    #[test]
    fn gummy_letters_take_the_colours_in_turn_leaving_spaces_out() {
        use yapfast_discord::collectibles::Style;
        let style = Style::parse(&serde_json::json!(
            {"font_id": 11, "effect_id": 8, "colors": [0xff0000, 0x00ff00]}
        ))
        .map(Into::into);
        let letters: Vec<_> = row(style, "ab c").letters.iter().collect();
        assert_eq!(letters.len(), 4);
        let reds = |letter: &NameLetter| letter.color.red() > letter.color.green();
        assert!(reds(&letters[0]) && !reds(&letters[1]), "a red, b green");
        assert_eq!(letters[2].text, " ");
        assert!(reds(&letters[3]), "c is the third letter, red again");
        // Other effects have none.
        let solid =
            Style::parse(&serde_json::json!({"font_id": 11, "effect_id": 1, "colors": [1]}));
        assert_eq!(row(solid.map(Into::into), "ab").letters.row_count(), 0);
    }

    #[test]
    fn a_style_without_an_effect_row_is_the_default() {
        assert_eq!(row(None, "x").effect, 0);
        assert_eq!(family(14), "Mainframe");
        assert_eq!(family(11), "");
        assert_eq!(family(7), "", "Medieval isn't bundled");
    }
}
