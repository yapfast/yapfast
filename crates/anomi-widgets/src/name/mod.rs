//! A person's name drawn in their display name style: Discord's styles give
//! a name a face, an effect and up to five colours. The Slint side:
//! `StyledName` (`ui/controls.slint:731`), with `NameStyleRow` filled by
//! `src/ui/name_style.rs`.
//!
//! - [`Style`] is the style as Discord sends it, the view-models'
//!   `anomi_vm::kind::NameStyle`; [`Style::font_only`] is how a name in a
//!   server wears it, as `font_only_in` made it.
//! - [`styled_name`] draws a name, as wide as its text and eliding when
//!   it's given less. Without a style, with only its face, or with one
//!   colour, it's text in the face; a gummy name puts each letter in the
//!   next colour, unless it's too long to show whole; gradient, glow,
//!   prism, neon, toon and pop are drawn from the glyphs' outlines, which
//!   can be filled with a gradient and edged as plain text can't.
//! - Each colour is first made to read against the member list's ground,
//!   one set for dark themes and one for light (`colours`), and the
//!   outlines are made once and kept in the widget's tree state until the
//!   name, its width, the style or the theme's darkness changes (`drawn`).
//! - A name is faded as a whole, as Slint fades the element: neon, toon
//!   and pop at an opacity below 1 are drawn once into a picture at full
//!   opacity, and the picture faded, so their edges never show through the
//!   letters.
//! - It reports its text to operations, so `ui.click("Bob")` finds a row
//!   by the name drawn in it.
//!
//! Nothing here moves: Discord's animated name effects aren't drawn, as
//! in the Slint app, so a name never asks for a frame.

mod colours;
mod drawn;
mod outline;
mod paint;
mod style;
#[cfg(test)]
pub(crate) mod tests;
mod widget;

pub use style::{Effect, Style, face};

use iced::font::Weight;
use iced::{Color, Font, Length};

use anomi_theme::{Palette, fonts, typography};

/// The colour of a name with no colours of its own: a theme's token, or a
/// colour such as a role's. `StyledName.ink`, which is `Theme.text` unless
/// a row says otherwise.
#[derive(Clone, Copy, Debug)]
pub enum Ink {
    Token(fn(&Palette) -> Color),
    Fixed(Color),
}

impl Ink {
    fn resolve(self, palette: &Palette) -> Color {
        match self {
            Self::Token(token) => token(palette),
            Self::Fixed(color) => color,
        }
    }
}

impl Default for Ink {
    fn default() -> Self {
        Self::Token(|palette| palette.text)
    }
}

impl From<Color> for Ink {
    fn from(color: Color) -> Self {
        Self::Fixed(color)
    }
}

impl From<fn(&Palette) -> Color> for Ink {
    fn from(token: fn(&Palette) -> Color) -> Self {
        Self::Token(token)
    }
}

/// `StyledName`'s default size and weight (`ui/controls.slint:735`).
const SIZE: f32 = 16.0;
const WEIGHT: Weight = Weight::Medium;

/// A name in its style; see [`styled_name`].
pub struct StyledName<'a> {
    text: &'a str,
    style: Option<Style>,
    size: f32,
    weight: Weight,
    ink: Ink,
    width: Length,
    height: Length,
    opacity: f32,
}

/// `text` drawn in `style`, or as plain text in its ink without one: 16 px,
/// medium, as wide as the text and a line high.
pub fn styled_name(text: &str, style: Option<Style>) -> StyledName<'_> {
    StyledName {
        text,
        style,
        size: SIZE,
        weight: WEIGHT,
        ink: Ink::default(),
        width: Length::Shrink,
        height: Length::Shrink,
        opacity: 1.0,
    }
}

impl StyledName<'_> {
    /// The text's size in logical pixels; the edges and offsets scale with it.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// The weight of a name in the usual font; a style's face has one weight
    /// of its own.
    pub fn weight(mut self, weight: Weight) -> Self {
        self.weight = weight;
        self
    }

    /// The colour of a name without colours of its own.
    pub fn ink(mut self, ink: impl Into<Ink>) -> Self {
        self.ink = ink.into();
        self
    }

    /// Given more than its text needs, a gradient spans the whole width,
    /// as the Slint text's did; given less, the name elides.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// The text is centred on the height given.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// The whole name's opacity, as an offline member's is halved.
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// The font the name is shaped in: its style's face, at that face's
    /// weight, or the interface's at the weight asked.
    fn font(&self) -> Font {
        self.style
            .and_then(|style| face(&style))
            .map_or_else(|| typography::weight(self.weight), fonts::name)
    }

    /// Whether the name is drawn a letter at a time, when it fits.
    fn gummy(&self) -> bool {
        self.style
            .is_some_and(|style| style.effect() == Effect::Gummy && !style.colors().is_empty())
    }
}
