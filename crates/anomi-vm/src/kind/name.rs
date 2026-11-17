//! How a person's name is dressed: their display name style as Discord
//! sends it (a face, an effect and up to five colours,
//! `yapfast_core::chat::NameStyle`, `display_name_styles` on the wire) and
//! the nameplate behind their row. A view-model holds them as they are and
//! hands them to F15's widgets: `anomi_widgets::name::styled_name(text,
//! row.style)` (its `Style` is [`NameStyle`]) and
//! `anomi_widgets::nameplate::nameplate(..).plate(&row.plate)`. What the
//! Slint side made of a style in `row` and `font_only_in`
//! (`src/ui/name_style.rs:197`, `:284`) is here: a style without colours is
//! none, an effect this client doesn't know is solid, and a name in a server
//! keeps only its face ([`NameStyle::font_only`]).

use iced::Color;
use iced::widget::image;

/// How a styled name is drawn: Discord's `effect_id`, 1 to 8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NameEffect {
    /// One colour.
    #[default]
    Solid,
    /// The colours across the name, from 10% to 90% of its width.
    Gradient,
    /// White letters edged in a lighter shade of the colour.
    Neon,
    /// The colour's light shades in bands, edged in its dark one.
    Toon,
    /// White letters edged in the darkest shade over a copy in the colour.
    Pop,
    /// As the gradient: Discord's glow isn't drawn, as in the Slint app.
    Glow,
    /// The colours round once, back to the first.
    Prism,
    /// Each letter in the next colour.
    Gummy,
}

impl NameEffect {
    /// The effect of Discord's id; one this client doesn't know is solid,
    /// as Discord's client draws it.
    pub const fn of(id: u8) -> Self {
        match id {
            2 => Self::Gradient,
            3 => Self::Neon,
            4 => Self::Toon,
            5 => Self::Pop,
            6 => Self::Glow,
            7 => Self::Prism,
            8 => Self::Gummy,
            _ => Self::Solid,
        }
    }

    /// Discord's id.
    pub const fn id(self) -> u8 {
        match self {
            Self::Solid => 1,
            Self::Gradient => 2,
            Self::Neon => 3,
            Self::Toon => 4,
            Self::Pop => 5,
            Self::Glow => 6,
            Self::Prism => 7,
            Self::Gummy => 8,
        }
    }
}

/// A person's display name style. It has at least one colour, or none when
/// it keeps only its face ([`NameStyle::font_only`]), drawn in the row's
/// ink. Small and `Copy`: a row holds it by value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NameStyle {
    font: u8,
    effect: NameEffect,
    len: u8,
    colors: [u32; 5],
}

impl NameStyle {
    /// Discord's `font_id`, `effect_id` and RGB colours, of which it keeps
    /// five. None without a colour: the name is drawn as any other.
    pub fn new(font: u8, effect: u8, colors: &[u32]) -> Option<Self> {
        if colors.is_empty() {
            return None;
        }
        let len = colors.len().min(5);
        let mut kept = [0; 5];
        kept[..len].copy_from_slice(&colors[..len]);
        Some(Self {
            font,
            effect: NameEffect::of(effect),
            len: len as u8,
            colors: kept,
        })
    }

    /// The style as a name in a Discord server wears it: its face alone, in
    /// the row's own colour, with none of its colours or effect, even while
    /// it's pointed at. Profiles, DMs and the style's own editor show it
    /// whole.
    pub fn font_only(self) -> Self {
        Self {
            effect: NameEffect::Solid,
            len: 0,
            colors: [0; 5],
            ..self
        }
    }

    /// Discord's `font_id`: 11 is the usual font; its face is
    /// `anomi_theme::fonts::Name::of(font_id)`.
    pub fn font_id(&self) -> u8 {
        self.font
    }

    pub fn effect(&self) -> NameEffect {
        self.effect
    }

    /// The RGB colours it asks for; none when it keeps only its face.
    pub fn colors(&self) -> &[u32] {
        &self.colors[..usize::from(self.len)]
    }
}

/// The nameplate behind a person's row: its still, once it came, and its
/// palette's colours on a dark theme and on a light one (none for a palette
/// Anomi doesn't know, which leaves the picture without a tint). Slint's
/// `nameplate` and `nameplate-dark`/`-light` of `MemberRow` and `RoomRow`.
/// The default is none: the plate draws nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Nameplate {
    pub picture: Option<image::Handle>,
    pub palette: Option<[Color; 2]>,
}

impl Nameplate {
    /// Whether there's anything to draw.
    pub fn is_some(&self) -> bool {
        self.picture.is_some() || self.palette.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_colour_is_no_style_and_an_unknown_effect_is_solid() {
        assert_eq!(NameStyle::new(3, 2, &[]), None);
        let unknown = NameStyle::new(11, 99, &[1]).expect("a colour");
        assert_eq!(unknown.effect(), NameEffect::Solid);
        assert_eq!(
            NameStyle::new(11, 0, &[1]).map(|s| s.effect()),
            Some(NameEffect::Solid)
        );
        let many = NameStyle::new(11, 2, &[1, 2, 3, 4, 5, 6, 7]).expect("colours");
        assert_eq!(many.colors(), &[1, 2, 3, 4, 5]);
        for id in 1..=8 {
            assert_eq!(NameEffect::of(id).id(), id);
        }
    }

    #[test]
    fn in_a_server_a_style_keeps_only_its_face() {
        let style = NameStyle::new(14, 3, &[0x00ff99]).expect("a colour");
        let plain = style.font_only();
        assert_eq!(plain.colors(), &[] as &[u32]);
        assert_eq!(plain.effect(), NameEffect::Solid);
        assert_eq!(plain.font_id(), 14);
    }

    #[test]
    fn a_plate_without_picture_or_palette_is_none() {
        assert!(!Nameplate::default().is_some());
        let tinted = Nameplate {
            palette: Some([Color::BLACK, Color::WHITE]),
            ..Nameplate::default()
        };
        assert!(tinted.is_some());
    }
}
