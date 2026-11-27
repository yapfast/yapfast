//! What the kit's pieces draw with: the theme, the icons and the window's
//! room. Views hand it to the kit instead of the whole window's state.

use iced::{Color, Size, advanced::image::Handle};

use crate::{
    icons::{Icon, Icons},
    theme::Theme,
    widgets::Pic,
};

/// The theme and the icons, borrowed, and the window's room; copied
/// freely.
#[derive(Clone, Copy)]
pub struct Look<'a> {
    pub theme: &'a Theme,
    pub icons: &'a Icons,
    /// The room under the window's own title bar (`App::window`).
    pub window: Size,
}

impl<'a> Look<'a> {
    /// One of the kit's icons at `size` in `color`, drawn once.
    pub fn icon(self, icon: Icon, size: u32, color: u32) -> Handle {
        self.icons.coloured(icon, size, color)
    }

    /// One of the kit's icons as a picture, `size` square, in `color`.
    pub fn glyph(self, icon: Icon, size: u32, color: Color) -> Pic {
        let glyph = Pic::square(self.icon(icon, size, crate::theme::rgb(color)), size as f32);
        // Drawn once in the colour's own; its alpha fades the picture.
        if color.a < 1.0 {
            glyph.opacity(color.a)
        } else {
            glyph
        }
    }

    /// As [`Self::glyph`], but in `color` even where the theme's icon pack
    /// keeps a replacement's own colours.
    pub fn tinted_glyph(self, icon: Icon, size: u32, color: Color) -> Pic {
        let glyph = Pic::square(
            self.icons.tinted(icon, size, crate::theme::rgb(color)),
            size as f32,
        );
        if color.a < 1.0 {
            glyph.opacity(color.a)
        } else {
            glyph
        }
    }

    /// One of the kit's icons in its own colours, `size` square, drawn once.
    pub fn drawn(self, icon: Icon, size: u32) -> Pic {
        Pic::square(self.icons.own(icon, size), size as f32)
    }
}
