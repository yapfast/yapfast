//! What a face's look chooses, and how the looks blend while a flag eases.

use iced::Color;

use anomi_theme::Theme;

use super::paint;

/// What a face's look chooses for one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colours {
    pub fill: Color,
    pub ink: Color,
    /// The caption's, which follows the pointer over the shape.
    pub caption: Color,
}

impl Colours {
    pub fn new(fill: Color, ink: Color) -> Self {
        Self {
            fill,
            ink,
            caption: ink,
        }
    }

    /// `self` at 0, `other` at 1.
    pub(super) fn blend(self, other: Self, amount: f32) -> Self {
        Self {
            fill: paint::blend(self.fill, other.fill, amount),
            ink: paint::blend(self.ink, other.ink, amount),
            caption: paint::blend(self.caption, other.caption, amount),
        }
    }
}

/// A face's look: its colours for `(hovered, enabled, open)`, where `open` is
/// the flag the face is given (a menu open, a toggle on, an alert). Called at
/// draw time with the theme, so a theme switch needs no rebuild; the face
/// blends the looks of a flag that is changing by how far it has eased.
pub type Look<'a> = Box<dyn Fn(&Theme, bool, bool, bool) -> Colours + 'a>;

/// `look(false)` at 0 and `look(true)` at 1, and only the one that shows when
/// the flag has settled.
pub(super) fn between(amount: f32, mut look: impl FnMut(bool) -> Colours) -> Colours {
    if amount <= 0.0 {
        look(false)
    } else if amount >= 1.0 {
        look(true)
    } else {
        let from = look(false);
        from.blend(look(true), amount)
    }
}
