//! The dragged row's faint copy: where it hangs, the ground under it, which
//! its owner shapes for each row, and the theme that fades what's drawn on
//! it. The Slint side: the copies in `ui/rail.slint:1613`-`:1641`, each a
//! `Rectangle` at `opacity: 0.6` (`:1616`, `:1631`) that fades its ground
//! and everything on it as one layer.
//!
//! iced at this pin draws no layer at an opacity, so the copy is drawn with
//! its theme's colours and its text faded instead. Pictures inside it (an
//! avatar's picture, an icon drawn as an image) can't fade that way: they
//! show at full strength on the faint ground. The faded theme is made once
//! a drag and kept in the list's state ([`Faded`]).

use std::cell::RefCell;

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::{Border, Color, Point, Rectangle, Size};

use anomi_theme::{Palette, Renderer, Theme, metrics};

/// How strong the copy is by default: 60% (`ui/rail.slint:1616`, `:1631`).
const ALPHA: f32 = 0.6;
/// Onto rows that are targets the copy's top is this far below the
/// pointer, so the name of the row it would drop on stays readable, as
/// someone dragged onto a voice channel (`ui/rail.slint:1614`).
pub const BELOW_POINTER: f32 = 12.0;

/// The ground under a dragged row's copy, as its owner shapes it: how far
/// in from the row's sides, its corners, its height (the row's own when
/// `None`) and how strong the copy is. By default it's inset as the rows'
/// own grounds are, with their corners (`ui/rail.slint:1628`-`:1630`), at
/// 60%. A category's copy is 36 px tall with 4 px corners, and a person's
/// dragged onto voice channels 40 px in from the left, 30 px tall
/// (`:1613`-`:1615`); N04 builds both copies with
/// [`List::drag_copy`](super::List::drag_copy), as neither is the row
/// itself. A dragged role (X08) lies on `raised` across the whole row with
/// 6 px corners, opaque (`ui/roles.slint:157`-`:161`):
/// `Ground { fill: Fill::Raised, left: 0.0, right: 0.0, radius: 6.0, alpha: 1.0, ..Ground::default() }`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    /// The palette's colour it's filled with.
    pub fill: Fill,
    pub left: f32,
    pub right: f32,
    pub radius: f32,
    pub height: Option<f32>,
    /// How strong the ground and the copy on it are, from 0 to 1.
    pub alpha: f32,
}

/// The palette token a [`Ground`] is filled with: `Theme.active` for the
/// sidebar's rows (`ui/rail.slint:1616`, `:1631`), `Theme.raised` for a role
/// (`ui/roles.slint:157`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fill {
    #[default]
    Active,
    Raised,
}

impl Fill {
    pub fn colour(self, palette: &Palette) -> Color {
        match self {
            Self::Active => palette.active,
            Self::Raised => palette.raised,
        }
    }
}

impl Default for Ground {
    fn default() -> Self {
        Self {
            fill: Fill::Active,
            left: metrics::ROW_INSET,
            right: metrics::ROW_INSET,
            radius: metrics::RADIUS_ROW,
            height: None,
            alpha: ALPHA,
        }
    }
}

/// Where `ground` lies under the copy of a row drawn at `row`: its top at
/// the row's.
pub fn ghost_ground(row: Rectangle, ground: Ground) -> Rectangle {
    Rectangle::new(
        Point::new(row.x + ground.left, row.y),
        Size::new(
            (row.width - ground.left - ground.right).max(0.0),
            ground.height.unwrap_or(row.height),
        ),
    )
}

/// Draws `ground` where it lies, at `bounds`.
pub fn draw_ground(renderer: &mut Renderer, theme: &Theme, bounds: Rectangle, ground: Ground) {
    renderer.fill_quad(
        Quad {
            bounds,
            border: Border::default().rounded(ground.radius),
            ..Quad::default()
        },
        ground
            .fill
            .colour(theme.palette())
            .scale_alpha(ground.alpha),
    );
}

/// The faded theme of the drag going on, made at its first draw (a layout
/// has no theme) and kept until the theme or the alpha changes, so a drag
/// doesn't make a theme each frame. The list's state holds one; its layout
/// empties it when no drag goes on.
#[derive(Debug, Default)]
pub struct Faded(RefCell<Option<(Theme, f32, Theme)>>);

impl Faded {
    /// `theme` at `alpha`, made once while both stay the same.
    pub fn get(&self, theme: &Theme, alpha: f32) -> Theme {
        let mut kept = self.0.borrow_mut();
        if let Some((from, at, faint)) = kept.as_ref()
            && *at == alpha
            && from == theme
        {
            return faint.clone();
        }
        let faint = faded(theme, alpha);
        *kept = Some((theme.clone(), alpha, faint.clone()));
        faint
    }

    pub fn clear(&mut self) {
        *self.0.get_mut() = None;
    }
}

/// `theme` with each of its colours at `alpha` of its strength, for the
/// copy drawn on the ground; `theme` itself when that's all of it.
pub fn faded(theme: &Theme, alpha: f32) -> Theme {
    if alpha >= 1.0 {
        return theme.clone();
    }
    let mut palette = *theme.palette();
    for index in 0..Palette::NAMES.len() {
        if let Some(colour) = palette.colour_mut(index) {
            *colour = colour.scale_alpha(alpha);
        }
    }
    Theme::new(theme.id(), theme.name(), palette)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: Rectangle = Rectangle {
        x: 0.0,
        y: 100.0,
        width: 248.0,
        height: 32.0,
    };

    #[test]
    fn the_ground_is_inset_as_the_rows_are_by_default() {
        let ground = ghost_ground(ROW, Ground::default());
        assert_eq!((ground.x, ground.y), (8.0, 100.0));
        assert_eq!(ground.width, 232.0);
        assert_eq!(ground.height, ROW.height, "the row's own height");
    }

    #[test]
    fn the_owner_shapes_the_ground_as_a_person_dragged_onto_voice_channels() {
        let person = Ground {
            left: 40.0,
            right: 8.0,
            radius: 4.0,
            height: Some(30.0),
            ..Ground::default()
        };
        let ground = ghost_ground(ROW, person);
        assert_eq!((ground.x, ground.y), (40.0, 100.0));
        assert_eq!((ground.width, ground.height), (200.0, 30.0));
    }

    #[test]
    fn a_ground_wider_than_its_row_is_empty() {
        let wide = Ground {
            left: 200.0,
            right: 100.0,
            ..Ground::default()
        };
        assert_eq!(ghost_ground(ROW, wide).width, 0.0);
    }

    #[test]
    fn the_copy_fades_with_its_ground_at_60_percent() {
        assert_eq!(Ground::default().alpha, 0.6);
        for theme in [
            Theme::built_in(None),
            Theme::built_in(Some("catppuccin-latte")),
        ] {
            let faint = faded(&theme, 0.6);
            let (strong, weak) = (theme.palette(), faint.palette());
            for (name, a, b) in [
                ("text", strong.text, weak.text),
                ("secondary", strong.secondary, weak.secondary),
            ] {
                assert!((b.a - a.a * 0.6).abs() < 1e-6, "{name}: {} of {}", b.a, a.a);
                assert_eq!((b.r, b.g, b.b), (a.r, a.g, a.b), "{name}: the same hue");
            }
            assert_eq!(faint.id(), theme.id());
            assert_eq!(faint.is_dark(), theme.is_dark());
            assert_eq!(faded(&theme, 1.0), theme, "an opaque copy keeps it");
        }
    }

    #[test]
    fn a_ground_is_filled_with_the_token_it_names() {
        let theme = Theme::built_in(None);
        let palette = theme.palette();
        assert_eq!(Ground::default().fill.colour(palette), palette.active);
        assert_eq!(Fill::Raised.colour(palette), palette.raised);
        assert_ne!(palette.active, palette.raised, "Graphite tells them apart");
    }

    #[test]
    fn a_drag_fades_its_theme_once() {
        let kept = Faded::default();
        let theme = Theme::built_in(None);
        let first = kept.get(&theme, 0.6);
        let again = kept.get(&theme, 0.6);
        assert!(std::ptr::eq(first.palette(), again.palette()), "the same");
        assert_eq!(first, faded(&theme, 0.6));
        let latte = Theme::built_in(Some("catppuccin-latte"));
        assert_eq!(kept.get(&latte, 0.6), faded(&latte, 0.6), "made again");
    }
}
