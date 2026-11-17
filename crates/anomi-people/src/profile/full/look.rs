//! What the modal's parts are drawn in: the interface's colours, or the
//! profile's theme as Discord's client works them out (`ProfileModal`'s
//! properties, `ui/people.slint:2118`-`:2148`). The card on the left reads
//! the same theme through `card::look::Inks`, so the text, the pointer's
//! ground and the status dot's ring come from there.

use iced::advanced::text::IntoFragment;
use iced::widget::text;
use iced::{Color, Font};

use anomi_theme::{Palette, Theme, rgba};
use anomi_vm::people::full_profile::FullProfile;
use anomi_vm::people::profile::Place;
use anomi_widgets::avatar::Ground;
use anomi_widgets::presence::Ring;

use crate::profile::card::look::Inks;

/// Which of the text's three strengths.
#[derive(Clone, Copy, Debug)]
pub enum Shade {
    Ink,
    Ink2,
    Ink3,
}

/// The modal's colours for one profile.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub inks: Inks,
    /// The theme's two colours, when the profile has one.
    colours: Option<[Color; 2]>,
}

/// What Slint's `color.mix(other, factor)` gives: `factor` of `base`.
pub(super) fn mix(base: Color, other: Color, factor: f32) -> Color {
    let blend = |a: f32, b: f32| a * factor + b * (1.0 - factor);
    Color::from_rgba(
        blend(base.r, other.r),
        blend(base.g, other.g),
        blend(base.b, other.b),
        blend(base.a, other.a),
    )
}

impl Look {
    pub fn new(full: &FullProfile) -> Self {
        Self {
            inks: Inks::new(&full.card, Place::Side),
            colours: full
                .card
                .theme
                .as_ref()
                .map(|theme| [theme.primary, theme.secondary]),
        }
    }

    pub fn themed(&self) -> bool {
        self.colours.is_some()
    }

    pub fn light(&self) -> bool {
        self.inks.light()
    }

    /// What the whole modal lies on: the theme's two colours mixed under
    /// the wash and 15% further toward it, one flat colour, as Discord's.
    pub fn ground(&self, palette: &Palette) -> Color {
        self.themed_ground().unwrap_or(palette.sidebar)
    }

    /// What a tile takes on the modal: the raised ground, or a white wash
    /// over a theme (24% on a light one).
    pub fn tile(&self, palette: &Palette) -> Color {
        match self.colours {
            None => palette.raised,
            Some(_) if self.light() => rgba(0xffffff3d),
            Some(_) => rgba(0xffffff14),
        }
    }

    /// The well a game's picture sits in: the tile on a theme.
    pub fn well(&self, palette: &Palette) -> Color {
        match self.colours {
            None => palette.field,
            Some(_) => self.tile(palette),
        }
    }

    /// The tab bar's rule.
    pub fn line(&self, palette: &Palette) -> Color {
        match self.colours {
            None => palette.line,
            Some(_) => self.inks.ink.at(palette).scale_alpha(0.12),
        }
    }

    pub fn shade(&self, shade: Shade, palette: &Palette) -> Color {
        match shade {
            Shade::Ink => self.ink(palette),
            Shade::Ink2 => self.ink2(palette),
            Shade::Ink3 => self.ink3(palette),
        }
    }

    /// Words in `shade`, in `size` and `font`.
    pub fn words<'a>(
        self,
        words: impl IntoFragment<'a>,
        size: f32,
        font: Font,
        shade: Shade,
    ) -> text::Text<'a, Theme> {
        text(words)
            .size(size)
            .font(font)
            .style(move |theme: &Theme| text::Style {
                color: Some(self.shade(shade, theme.palette())),
                ..text::Style::default()
            })
    }

    pub fn ink(&self, palette: &Palette) -> Color {
        self.inks.ink.at(palette)
    }

    pub fn ink2(&self, palette: &Palette) -> Color {
        self.inks.ink2.at(palette)
    }

    pub fn ink3(&self, palette: &Palette) -> Color {
        self.inks.ink3.at(palette)
    }

    pub fn hover(&self, palette: &Palette) -> Color {
        self.inks.hover.at(palette)
    }

    /// What a row's status dot is cut from at rest or under the pointer:
    /// the modal's ground, with the hover wash over it.
    pub fn ring(&self, hovered: bool) -> Ring {
        let Some(ground) = self.themed_ground() else {
            return Ring::Ground(if hovered {
                Ground::Hover
            } else {
                Ground::Sidebar
            });
        };
        if !hovered {
            return Ring::Color(ground);
        }
        let (wash, alpha) = if self.light() {
            (Color::BLACK, 0x14 as f32 / 255.0)
        } else {
            (Color::WHITE, 0x0a as f32 / 255.0)
        };
        Ring::Color(mix(wash, ground, alpha))
    }

    fn themed_ground(&self) -> Option<Color> {
        let [primary, secondary] = self.colours?;
        let over = if self.light() {
            Color::WHITE
        } else {
            Color::BLACK
        };
        let wash = mix(mix(primary, secondary, 0.5), over, 0.4);
        Some(mix(wash, over, 0.85))
    }
}
