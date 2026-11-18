//! The colour popover's state: saturation, brightness and hue, with the
//! code written beside them (`ColourPopover`, `ui/server.slint:989`).

use iced::Color;

/// What the popover's pad, bar or code asks to be.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// The pad was pressed or dragged: saturation along it, brightness up
    /// it, each from 0 to 1.
    Pad { sat: f32, val: f32 },
    /// The hue bar: from 0 to 360.
    Hue(f32),
    /// The code was edited, as typed.
    Text(String),
}

/// A colour being picked, and the code as it is written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Colour {
    pub hue: f32,
    pub sat: f32,
    pub val: f32,
    /// The code, without its `#`. A wrong one stays as typed while the pad
    /// keeps the last colour that was right.
    pub text: String,
}

impl Colour {
    /// The popover opening on `start`, a code with or without its `#`.
    pub fn start(start: &str) -> Self {
        let mut colour = Self {
            text: start.trim().trim_start_matches('#').to_owned(),
            ..Self::default()
        };
        if let Some(found) = parse_hex(start) {
            colour.load(found);
        }
        colour
    }

    /// The colour the pad, bar and handles show.
    pub fn colour(&self) -> Color {
        hsv(self.hue, self.sat, self.val)
    }

    /// The hue at full saturation and brightness, the pad's far edge.
    pub fn pure(&self) -> Color {
        hsv(self.hue, 1.0, 1.0)
    }

    /// Takes the change. A pad or bar writes its colour into the code; a
    /// code that is a colour moves them. It gives the colour to use, `None`
    /// for a code that isn't one: that draft stays, and nothing changes.
    pub fn apply(&mut self, change: Change) -> Option<Color> {
        match change {
            Change::Pad { sat, val } => {
                self.sat = sat.clamp(0.0, 1.0);
                self.val = val.clamp(0.0, 1.0);
            }
            Change::Hue(hue) => self.hue = hue.clamp(0.0, 360.0),
            Change::Text(text) => {
                let found = parse_hex(&text);
                self.text = text;
                return found.inspect(|found| self.load(*found));
            }
        }
        let colour = self.colour();
        self.text = format_hex(colour);
        Some(colour)
    }

    fn load(&mut self, colour: Color) {
        let (max, min) = (
            colour.r.max(colour.g).max(colour.b),
            colour.r.min(colour.g).min(colour.b),
        );
        let delta = max - min;
        self.val = max;
        self.sat = if max > 0.0 { delta / max } else { 0.0 };
        // A grey has no hue: the bar stays where it was.
        if delta > 0.0 {
            let sector = if max == colour.r {
                ((colour.g - colour.b) / delta).rem_euclid(6.0)
            } else if max == colour.g {
                (colour.b - colour.r) / delta + 2.0
            } else {
                (colour.r - colour.g) / delta + 4.0
            };
            self.hue = sector * 60.0;
        }
    }
}

/// A colour from its hue (0 to 360), saturation and brightness (0 to 1).
fn hsv(hue: f32, sat: f32, val: f32) -> Color {
    let hue = hue.rem_euclid(360.0);
    let chroma = val * sat;
    let sector = hue / 60.0;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let base = val - chroma;
    Color::from_rgb(r + base, g + base, b + base)
}

/// `rrggbb` as the code is written, lower case, no `#`.
pub fn format_hex(colour: Color) -> String {
    let [r, g, b, _] = colour.into_rgba8();
    format!("{r:02x}{g:02x}{b:02x}")
}

/// A colour from six hex digits, with a `#` or without.
pub fn parse_hex(text: &str) -> Option<Color> {
    let hex = text.trim().trim_start_matches('#');
    // `from_str_radix` would take a sign: "+12345" isn't a colour.
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some(Color::from_rgb8(
        (rgb >> 16) as u8,
        (rgb >> 8) as u8,
        rgb as u8,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_moves_the_pad_and_a_pad_writes_the_code() {
        let mut colour = Colour::start("#ff0000");
        assert_eq!((colour.hue, colour.sat, colour.val), (0.0, 1.0, 1.0));
        assert_eq!(colour.text, "ff0000");
        colour.apply(Change::Hue(120.0));
        assert_eq!(colour.text, "00ff00");
        let chosen = colour.apply(Change::Pad { sat: 0.0, val: 1.0 });
        assert_eq!(chosen.map(format_hex).as_deref(), Some("ffffff"));
        assert_eq!(colour.hue, 120.0, "a white keeps the bar");
    }

    #[test]
    fn a_wrong_code_stays_as_typed_and_changes_nothing() {
        let mut colour = Colour::start("336699");
        let before = colour.clone();
        assert_eq!(colour.apply(Change::Text("33669".into())), None);
        assert_eq!(colour.text, "33669");
        assert_eq!(
            (colour.hue, colour.sat, colour.val),
            (before.hue, before.sat, before.val)
        );
        let found = colour.apply(Change::Text("#00ff00".into()));
        assert_eq!(found.map(format_hex).as_deref(), Some("00ff00"));
        assert_eq!(colour.hue, 120.0);
        assert_eq!(parse_hex("+12345"), None, "a sign is no digit");
    }

    #[test]
    fn hsv_round_trips_a_code() {
        for code in ["b8a587", "123456", "fedcba", "0000ff"] {
            let colour = Colour::start(code);
            assert_eq!(format_hex(colour.colour()), code);
        }
    }
}
