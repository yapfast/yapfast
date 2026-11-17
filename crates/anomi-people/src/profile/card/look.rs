//! What a card's place and its profile theme make of it: its sizes
//! (`ui/people.slint:1370`-`:1373`, `:1444`-`:1449`) and its colours, which
//! a theme sets whatever the interface's own theme is (`ProfileTheme`,
//! `ui/people.slint:388`, and the card's properties at `:1378`-`:1437`).

use iced::Color;

use anomi_theme::Palette;
use anomi_theme::rgba;
use anomi_vm::people::activity::card::Look as ActivityLook;
use anomi_vm::people::profile::{Card, Place, Theme};
use anomi_widgets::avatar::Ground;
use anomi_widgets::presence::Ring;

/// The popout's width (`ui/yapfast.slint:5031`) and the full profile's
/// card's (`card-width`, `ui/people.slint:2137`); the panel's is its
/// column's, the profile panel's 320 less 8 each side.
pub const POPOUT: f32 = 300.0;
const SIDE: f32 = 400.0;
const PANEL: f32 = 304.0;

/// A card's sizes in its place.
#[derive(Clone, Copy, Debug)]
pub struct Size {
    pub width: f32,
    pub banner: f32,
    pub avatar: f32,
    /// The ground's ring round the avatar.
    pub ring: f32,
    /// The words' inset from the card's sides.
    pub pad: f32,
    pub radius: f32,
    /// The lower corners: the full profile's run on past the modal's foot.
    pub foot: f32,
    /// Between the sections, and inside a titled one.
    pub gap: f32,
    pub titled: f32,
    pub name: f32,
}

impl Size {
    pub fn of(place: Place) -> Self {
        let side = place.large();
        let radius = if place == Place::Panel { 8.0 } else { 12.0 };
        Self {
            width: match place {
                Place::Side => SIDE,
                Place::Panel => PANEL,
                Place::Popout | Place::Preview => POPOUT,
            },
            banner: if side { 140.0 } else { 105.0 },
            avatar: if side { 120.0 } else { 80.0 },
            ring: if side { 8.0 } else { 6.0 },
            pad: if side { 32.0 } else { 16.0 },
            radius,
            foot: if side { 0.0 } else { radius },
            gap: if side { 24.0 } else { 12.0 },
            titled: if side { 8.0 } else { 6.0 },
            name: if side { 24.0 } else { 20.0 },
        }
    }

    /// The banner with the avatar hanging over its edge: half the avatar
    /// below it and 8 px, or the ring and 12 px on the full profile's.
    pub fn head(&self) -> f32 {
        let under = if self.avatar > 80.0 {
            self.ring + 12.0
        } else {
            8.0
        };
        self.banner + self.avatar / 2.0 + under
    }
}

/// A colour of the palette, or one a profile theme fixes.
#[derive(Clone, Copy, Debug)]
pub enum Tone {
    Token(fn(&Palette) -> Color),
    Fixed(Color),
}

impl Tone {
    pub fn at(self, palette: &Palette) -> Color {
        match self {
            Self::Token(token) => token(palette),
            Self::Fixed(color) => color,
        }
    }

    pub fn fixed(self) -> Option<Color> {
        match self {
            Self::Fixed(color) => Some(color),
            Self::Token(_) => None,
        }
    }
}

/// The card's colours.
#[derive(Clone, Copy, Debug)]
pub struct Inks {
    pub theme: Option<Theme>,
    /// The text, strong, subtle and muted.
    pub ink: Tone,
    pub ink2: Tone,
    pub ink3: Tone,
    /// A row under the pointer.
    pub hover: Tone,
    /// What lies behind the avatar's ring: the card's own ground.
    pub ground: Tone,
    /// What the presence dot's ring is cut from: the same.
    pub ring: Ring,
    pub bubble: Tone,
    pub bubble_edge: Tone,
    /// A button's tile on the card, and under the pointer.
    pub tile: Tone,
    pub tile_hover: Tone,
}

impl Inks {
    pub fn new(card: &Card, place: Place) -> Self {
        match card.theme {
            Some(theme) => themed(theme),
            None => plain(place),
        }
    }

    pub fn light(&self) -> bool {
        self.theme.is_some_and(|theme| theme.light)
    }

    /// What an activity card on this one is drawn in: its theme's
    /// colours, or the palette's.
    pub fn activity(card: &Card) -> &ActivityLook {
        const PLAIN: ActivityLook = ActivityLook {
            ink: None,
            ink2: None,
            ink3: None,
            ground: None,
            track: None,
        };
        card.theme.as_ref().map_or(&PLAIN, |theme| &theme.activity)
    }
}

/// The theme's text, and the rest as the card measured them.
fn themed(theme: Theme) -> Inks {
    let pick = |light: u32, dark: u32| Tone::Fixed(rgba(if theme.light { light } else { dark }));
    Inks {
        theme: Some(theme),
        ink: Tone::Fixed(theme.ink()),
        ink2: Tone::Fixed(theme.ink2()),
        ink3: Tone::Fixed(theme.ink3()),
        hover: pick(0x00000014, 0xffffff0a),
        ground: Tone::Fixed(theme.start),
        ring: Ring::Color(theme.start),
        bubble: pick(0xffffffff, 0x232427ff),
        bubble_edge: pick(0x00000026, 0x404044ff),
        tile: pick(0xffffffb3, 0xffffff1f),
        tile_hover: pick(0xffffffe6, 0xffffff33),
    }
}

/// The interface's own colours, on the place's ground: the full
/// profile's card is raised off its modal, the panel lies on the panel's
/// ground, the popout on the sidebar's.
fn plain(place: Place) -> Inks {
    let (ground, ring) = match place {
        Place::Side => (
            Tone::Token(|palette| palette.raised),
            Ring::Ground(Ground::Panel),
        ),
        Place::Panel => (
            Tone::Token(|palette| palette.panel),
            Ring::Ground(Ground::Panel),
        ),
        Place::Popout | Place::Preview => (
            Tone::Token(|palette| palette.sidebar),
            Ring::Ground(Ground::Sidebar),
        ),
    };
    Inks {
        theme: None,
        ink: Tone::Token(|palette| palette.text),
        ink2: Tone::Token(|palette| palette.secondary),
        ink3: Tone::Token(|palette| palette.muted),
        hover: Tone::Token(|palette| palette.hover),
        ground,
        ring,
        bubble: Tone::Token(|palette| palette.raised),
        bubble_edge: Tone::Token(|palette| palette.edge),
        tile: Tone::Token(|palette| palette.text.scale_alpha(0.12)),
        tile_hover: Tone::Token(|palette| palette.text.scale_alpha(0.2)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_profiles_card_is_larger() {
        let side = Size::of(Place::Side);
        assert_eq!((side.banner, side.avatar, side.pad), (140.0, 120.0, 32.0));
        assert_eq!(side.head(), 140.0 + 60.0 + 20.0);
        let popout = Size::of(Place::Popout);
        assert_eq!(popout.head(), 105.0 + 40.0 + 8.0);
        assert_eq!(Size::of(Place::Panel).radius, 8.0);
    }
}
