//! How the account's profile theme colours its card, as Discord's client
//! works it out (`ProfileTheme`, `ui/people.slint:388`): the theme is light
//! when its first colour is (brightness, red, green and blue weighed
//! 0.299, 0.587 and 0.114, at least half of full); a light theme lies under
//! a 60% white wash and takes dark text, a dark one under a 60% black wash
//! and takes white text. Yapfast's own theme leaves it as it is.

use iced::{Background, Color, Radians, gradient};

use anomi_theme::Palette;
use anomi_vm::account::Profile;
use anomi_widgets::avatar::Ground;
use anomi_widgets::name::Ink;
use anomi_widgets::presence::Ring;

/// Strong text over a light and a dark theme (`ProfileTheme`,
/// `ui/people.slint:398`: `#000000f5` and `#fffffff5`).
const INK_ON_LIGHT: Color = Color::from_rgba(0.0, 0.0, 0.0, 245.0 / 255.0);
const INK_ON_DARK: Color = Color::from_rgba(1.0, 1.0, 1.0, 245.0 / 255.0);
/// The server tag's fill over a light and a dark card
/// (`account-popover.slint:78`: `#ffffff3d` and `#97979f33`).
const TAG_ON_LIGHT: Color = Color::from_rgba(1.0, 1.0, 1.0, 61.0 / 255.0);
const TAG_ON_DARK: Color = Color::from_rgba(151.0 / 255.0, 151.0 / 255.0, 159.0 / 255.0, 0.2);

/// A group of rows or the game's card: the raised fill, 8 px round, no
/// edge (`account-popover.slint:91`).
pub fn group(theme: &anomi_theme::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(theme.palette().raised.into()),
        border: iced::Border::default().rounded(super::RADIUS),
        ..iced::widget::container::Style::default()
    }
}

/// Under the card: either Yapfast's panel, or the theme's two colours.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub theme: Option<Theme>,
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub primary: Color,
    pub light: bool,
    /// The wash over the first and the second colour.
    pub start: Color,
    pub end: Color,
}

impl Look {
    pub fn of(profile: &Profile) -> Self {
        Self {
            theme: profile.theme.map(|[primary, secondary]| {
                let light = brightness(primary) >= 0.5;
                Theme {
                    primary,
                    light,
                    start: wash(primary, light),
                    end: wash(secondary, light),
                }
            }),
        }
    }

    /// The card's ground.
    pub fn ground(&self, palette: &Palette) -> Background {
        match self.theme {
            Some(theme) => Background::Gradient(
                gradient::Linear::new(Radians(std::f32::consts::PI))
                    .add_stop(0.0, theme.start)
                    .add_stop(1.0, theme.end)
                    .into(),
            ),
            None => palette.panel.into(),
        }
    }

    pub fn border(&self, palette: &Palette) -> Color {
        self.theme.map_or(palette.line, |theme| theme.primary)
    }

    /// The ring that cuts the avatar and its dot out of the banner.
    pub fn ring(&self, palette: &Palette) -> Color {
        self.theme.map_or(palette.panel, |theme| theme.start)
    }

    /// The same ring for a widget that draws its own.
    pub fn dot_ring(&self) -> Ring {
        match self.theme {
            Some(theme) => Ring::Color(theme.start),
            None => Ring::Ground(Ground::Panel),
        }
    }

    /// Text over the card: Discord's strong text, black or white at 96%.
    pub fn ink(&self) -> Ink {
        match self.theme {
            Some(theme) if theme.light => Ink::Fixed(INK_ON_LIGHT),
            Some(_) => Ink::Fixed(INK_ON_DARK),
            None => Ink::Token(|palette| palette.text),
        }
    }

    /// The strong ink of a theme, for what Yapfast's own ink would not do.
    pub fn themed_ink(&self) -> Option<Color> {
        match self.ink() {
            Ink::Fixed(color) => Some(color),
            Ink::Token(_) => None,
        }
    }

    /// The server tag's fill over the card.
    pub fn tag(&self) -> Color {
        match self.theme {
            Some(theme) if theme.light => TAG_ON_LIGHT,
            _ => TAG_ON_DARK,
        }
    }
}

fn brightness(color: Color) -> f32 {
    0.299 * color.r + 0.587 * color.g + 0.114 * color.b
}

/// `colour` under the wash: 40% of it and the rest white or black.
fn wash(colour: Color, light: bool) -> Color {
    let over = if light { 1.0 } else { 0.0 };
    let blend = |c: f32| c * 0.4 + over * 0.6;
    Color {
        r: blend(colour.r),
        g: blend(colour.g),
        b: blend(colour.b),
        a: colour.a * 0.4 + 0.6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dark_theme_takes_white_text_and_a_light_one_black() {
        let dark = Profile {
            theme: Some([Color::from_rgb8(20, 10, 60), Color::BLACK]),
            ..Profile::default()
        };
        let light = Profile {
            theme: Some([Color::from_rgb8(240, 230, 200), Color::WHITE]),
            ..Profile::default()
        };
        assert!(!Look::of(&dark).theme.unwrap().light);
        assert!(Look::of(&light).theme.unwrap().light);
        assert!(Look::of(&Profile::default()).theme.is_none());
    }
}
