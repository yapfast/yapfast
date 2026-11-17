//! A display name style as Discord sends it: a face, an effect and up to
//! five colours (`yapfast_core::chat::NameStyle`, `display_name_styles` on
//! the wire). What the Slint side made of it in `row` and `font_only_in`
//! (`src/ui/name_style.rs:197`, `:284`) is here: a style without colours is
//! none, an effect this client doesn't know is solid, and a name in a server
//! keeps only its face.

use anomi_theme::fonts::Name;

/// How a styled name is drawn: Discord's `effect_id`, 1 to 8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Effect {
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

impl Effect {
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
/// it keeps only its face ([`Style::font_only`]), drawn in the row's ink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Style {
    font: u8,
    effect: Effect,
    len: u8,
    colors: [u32; 5],
}

impl Style {
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
            effect: Effect::of(effect),
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
            effect: Effect::Solid,
            len: 0,
            colors: [0; 5],
            ..self
        }
    }

    /// Discord's `font_id`.
    pub fn font_id(&self) -> u8 {
        self.font
    }

    /// The face it's drawn in; none for the usual font, which is 11 and
    /// the retired ids.
    pub fn face(&self) -> Option<Name> {
        Name::of(self.font)
    }

    pub fn effect(&self) -> Effect {
        self.effect
    }

    /// The RGB colours it asks for; none when it keeps only its face.
    pub fn colors(&self) -> &[u32] {
        &self.colors[..usize::from(self.len)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_colour_is_no_style_and_an_unknown_effect_is_solid() {
        assert_eq!(Style::new(3, 2, &[]), None);
        let unknown = Style::new(11, 99, &[1]).expect("a colour");
        assert_eq!(unknown.effect(), Effect::Solid);
        assert_eq!(
            Style::new(11, 0, &[1]).map(|s| s.effect()),
            Some(Effect::Solid)
        );
        let many = Style::new(11, 2, &[1, 2, 3, 4, 5, 6, 7]).expect("colours");
        assert_eq!(many.colors(), &[1, 2, 3, 4, 5]);
        for id in 1..=8 {
            assert_eq!(Effect::of(id).id(), id);
        }
    }

    #[test]
    fn in_a_server_a_style_keeps_only_its_face() {
        let style = Style::new(14, 3, &[0x00ff99]).expect("a colour");
        let plain = style.font_only();
        assert_eq!(plain.colors(), &[] as &[u32]);
        assert_eq!(plain.effect(), Effect::Solid);
        assert_eq!(plain.face(), style.face());
        assert_eq!(plain.face().map(Name::family), Some("Mainframe"));
        // The usual font has no face.
        assert_eq!(Style::new(11, 1, &[1]).and_then(|s| s.face()), None);
    }
}
