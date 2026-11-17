//! A status dot whose ring is the raised ground, or the lit row's fill
//! (`Theme.raised` in `account-popover.slint:121`; a `StatusOption`'s own
//! ground, `ui/dialogs.slint:572`). `anomi_widgets::avatar::Ground` has no
//! raised ground yet, so this picks the colour as it draws, from the theme,
//! and lets the foundation's dot do the rest.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree};
use iced::advanced::{mouse, renderer};
use iced::{Color, Length, Rectangle, Size};

use anomi_theme::{Palette, Renderer, Theme};
use anomi_vm::kind::Presence;
use anomi_widgets::presence::{Ring, StatusDot, status_dot};

pub struct Ringed {
    presence: Presence,
    width: f32,
    ring: fn(&Palette) -> Color,
}

/// The dot of `presence`, `width` wide, its ring the palette's `raised`
/// (`lit` false) or `active` (the row's lit fill).
pub fn dot(presence: Presence, width: f32, lit: bool) -> Ringed {
    Ringed {
        presence,
        width,
        ring: if lit {
            |palette| palette.active
        } else {
            |palette| palette.raised
        },
    }
}

impl Ringed {
    fn dot(&self, ring: Color) -> StatusDot {
        status_dot(self.presence, self.width, Ring::Color(ring))
    }
}

impl widget::Meta for Ringed {}

impl<M> widget::Widget<M, Theme, Renderer> for Ringed {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width), Length::Fixed(self.width))
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        widget::Widget::<M, Theme, Renderer>::layout(
            &mut self.dot(Color::TRANSPARENT),
            tree,
            renderer,
            limits,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let dot = self.dot((self.ring)(theme.palette()));
        widget::Widget::<M, Theme, Renderer>::draw(
            &dot, tree, renderer, theme, style, layout, cursor, viewport,
        );
    }
}
