//! A profile's theme, a Nitro setting: its two colours, the card's
//! gradient between them under a wash, and the text over it, whatever the
//! interface's own theme is (`ProfileTheme`, `ui/people.slint:388`, and the
//! card's properties at `:1378`-`:1387`).

use iced::Color;

use crate::people::activity::card::Look;

/// Discord's strong, subtle and muted text over a theme: black or white at
/// 96%, 88% and 76%.
const INK: f32 = 0.96;
const INK2: f32 = 0.88;
const INK3: f32 = 0.76;
/// The unplayed part of a song's bar: the text at 20%.
const TRACK: f32 = 0.2;

/// A profile's theme, worked out once from its two colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub primary: Color,
    pub secondary: Color,
    /// Each under the wash: the card's ground runs from one to the other.
    pub start: Color,
    pub end: Color,
    /// The first colour is light: the text is black and the wash white.
    pub light: bool,
    /// What an activity card on it is drawn in.
    pub activity: Look,
}

impl Theme {
    pub fn new([primary, secondary]: [Color; 2]) -> Self {
        let light = light(primary);
        let start = wash(primary, light);
        let ink = ink(light, INK);
        Self {
            primary,
            secondary,
            start,
            end: wash(secondary, light),
            light,
            activity: Look {
                ink: Some(ink),
                ink2: Some(self::ink(light, INK2)),
                ink3: Some(self::ink(light, INK3)),
                ground: Some(start),
                track: Some(ink.scale_alpha(TRACK)),
            },
        }
    }

    /// The text, strong, subtle and muted.
    pub fn ink(&self) -> Color {
        ink(self.light, INK)
    }

    pub fn ink2(&self) -> Color {
        ink(self.light, INK2)
    }

    pub fn ink3(&self) -> Color {
        ink(self.light, INK3)
    }
}

fn ink(light: bool, alpha: f32) -> Color {
    let base = if light { Color::BLACK } else { Color::WHITE };
    base.scale_alpha(alpha)
}

/// `ProfileTheme.light`: the colour's luma is at least half.
pub fn light(color: Color) -> bool {
    (0.299 * color.r + 0.587 * color.g + 0.114 * color.b) * 255.0 >= 127.5
}

/// `ProfileTheme.wash`: 40% of the colour over white on a light theme,
/// black on a dark one.
pub fn wash(color: Color, light: bool) -> Color {
    let over = if light { 1.0 } else { 0.0 };
    let blend = |channel: f32| channel * 0.4 + over * 0.6;
    Color::from_rgba(
        blend(color.r),
        blend(color.g),
        blend(color.b),
        color.a * 0.4 + 0.6,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_is_light_by_its_first_colour() {
        assert!(light(Color::from_rgb8(0xf5, 0xa9, 0xc8)));
        assert!(!light(Color::from_rgb8(0x20, 0x10, 0x40)));
        let theme = Theme::new([Color::WHITE, Color::BLACK]);
        assert!(theme.light && theme.ink().r == 0.0);
    }

    #[test]
    fn the_wash_takes_six_tenths_of_white_or_black() {
        let washed = wash(Color::from_rgb(1.0, 0.0, 0.0), false);
        assert!((washed.r - 0.4).abs() < 1e-6 && washed.g.abs() < 1e-6);
        let washed = wash(Color::BLACK, true);
        assert!((washed.r - 0.6).abs() < 1e-6 && washed.a == 1.0);
    }
}
