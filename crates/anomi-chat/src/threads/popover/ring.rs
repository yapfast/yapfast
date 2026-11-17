//! The ring round a member's avatar. In Slint it is `raised` while the card
//! is lit and `hover` at rest, easing with the card (`ui/header.slint:388`),
//! so it reads the card's lit value, which the card sets as it draws just
//! before its content does.

use std::cell::Cell;
use std::rc::Rc;

use iced::advanced::layout;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree};
use iced::advanced::{Layout, mouse};
use iced::{Border, Color, Length, Rectangle, Size};

use anomi_theme::{Renderer, Theme};

/// How lit the card is, 0 at rest to 1 under the pointer; the card writes it
/// as it draws, the rings read it.
pub type Lit = Rc<Cell<f32>>;

/// `factor` of `base` and the rest of `other`.
pub fn mix(base: Color, other: Color, factor: f32) -> Color {
    let blend = |a: f32, b: f32| a * factor + b * (1.0 - factor);
    Color {
        r: blend(base.r, other.r),
        g: blend(base.g, other.g),
        b: blend(base.b, other.b),
        a: blend(base.a, other.a),
    }
}

/// A round disc of `size` in the ring's colour.
pub struct Ring {
    size: f32,
    lit: Lit,
}

pub fn ring(size: f32, lit: Lit) -> Ring {
    Ring { size, lit }
}

impl widget::Meta for Ring {}

impl<M> widget::Widget<M, Theme, Renderer> for Ring {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.size), Length::Fixed(self.size))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let side = Length::Fixed(self.size);
        tree.size = limits
            .width(side)
            .height(side)
            .resolve(side, side, Size::ZERO);
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let palette = theme.palette();
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: Border::default().rounded(self.size / 2.0),
                ..Quad::default()
            },
            mix(palette.hover, palette.raised, 1.0 - self.lit.get()),
        );
    }
}
