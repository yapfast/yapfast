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
    /// Degrees it turns about its centre, as a theme's leaning tile.
    turn: f32,
    /// Rounds, besides `radius`, the corners it shares with the rounded
    /// pane it's drawn in (a theme's floating pane), so it never shows
    /// past the pane's arc.
    pane: bool,
}

impl Pic {
    pub fn new(handle: Handle, size: Size) -> Self {
        Self {
            handle,
            size,
            at: Rectangle::with_size(size),
            radius: [0.0; 4],
            opacity: 1.0,
            turn: 0.0,
            pane: false,
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

    /// Also rounds the corners it shares with its rounded pane.
    pub fn pane_corners(mut self) -> Self {
        self.pane = true;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Turned `degrees` about its centre (square corners, then).
    pub fn turn(mut self, degrees: f32) -> Self {
        self.turn = degrees;
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
        if self.turn != 0.0 {
            use iced::advanced::image::Renderer as _;
            renderer.draw_image(
                iced::advanced::image::Image {
                    handle: self.handle.clone(),
                    filter_method: iced::advanced::image::FilterMethod::Linear,
                    rotation: iced::Radians(self.turn.to_radians()),
                    border_radius: Default::default(),
                    opacity: self.opacity,
                    snap: false,
                },
                bounds,
                *_viewport,
            );
            return;
        }
        let radius = if self.pane {
            let pane = super::glass::pane_radii(bounds, 0.0);
            std::array::from_fn(|corner| self.radius[corner].max(pane[corner]))
        } else {
            self.radius
        };
        draw_picture(renderer, &self.handle, bounds, radius, self.opacity);
    }
}

impl<'a, M: 'a> From<Pic> for Element<'a, M> {
    fn from(picture: Pic) -> Self {
        Element::new(picture)
    }
}
