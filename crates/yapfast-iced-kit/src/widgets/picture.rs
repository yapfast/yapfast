//! Pictures drawn one to one: [`Pic`].

use iced::{
    Element, Length, Rectangle, Size, Theme, Vector,
    advanced::{Layout, Widget, image::Handle, layout, mouse, renderer, widget::Tree},
    widget::{Container, container},
};

use super::{Renderer, draw_picture};

/// A picture drawn one to one (the patched renderer copies it in), with
/// round corners and an opacity. It takes `size` in the layout but may be
/// drawn elsewhere around it (`at`, from the layout's corner), as a
/// decoration overhangs its avatar.
pub struct Pic {
    handle: Handle,
    size: Size,
    at: Rectangle,
    radius: [f32; 4],
    opacity: f32,
}

impl Pic {
    pub fn new(handle: Handle, size: Size) -> Self {
        Self {
            handle,
            size,
            at: Rectangle::with_size(size),
            radius: [0.0; 4],
            opacity: 1.0,
        }
    }

    /// A square picture, `side` across.
    pub fn square(handle: Handle, side: f32) -> Self {
        Self::new(handle, Size::new(side, side))
    }

    /// Centred in a box `side` across, as an icon on a button's face or a
    /// badge; the box can be styled further.
    pub fn boxed<'a, M: 'a>(self, side: impl Into<Length>) -> Container<'a, M, Theme, Renderer> {
        container(self).center(side)
    }

    pub fn at(mut self, at: Rectangle) -> Self {
        self.at = at;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = [radius; 4];
        self
    }

    pub fn radius_each(mut self, radius: [f32; 4]) -> Self {
        self.radius = radius;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

impl<M> Widget<M, Theme, Renderer> for Pic {
    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(self.size.width),
            Length::Fixed(self.size.height),
        )
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(self.size)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = self.at + Vector::new(layout.bounds().x, layout.bounds().y);
        draw_picture(renderer, &self.handle, bounds, self.radius, self.opacity);
    }
}

impl<'a, M: 'a> From<Pic> for Element<'a, M> {
    fn from(picture: Pic) -> Self {
        Element::new(picture)
    }
}
