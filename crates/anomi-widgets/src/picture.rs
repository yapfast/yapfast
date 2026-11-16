//! A picture with rounded corners.
//!
//! iced's tiny-skia renderer ignores an image's border radius at the pinned
//! commit (`TODO: Border radius` in `tiny_skia/src/engine.rs`), so a round
//! avatar would show square. Until the pictures task makes rounded copies
//! (docs/iced/tasks/foundation.md, F05), this draws the picture and then covers
//! its corners, clipped to the picture, with a ring of the ground's colour:
//! right on a plain ground, a step off on a hovered row.

use iced::advanced::image::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree};
use iced::advanced::{image as raster, mouse};
use iced::widget::image;
use iced::{Border, Color, Length, Rectangle, Size};

use anomi_theme::{Palette, Renderer, Theme};

pub struct Rounded {
    handle: image::Handle,
    side: f32,
    radius: f32,
    ground: fn(&Palette) -> Color,
}

/// `handle` drawn `side` square with corners of `radius`, on a ground of
/// the colour `ground` picks from the palette.
pub fn rounded(
    handle: image::Handle,
    side: f32,
    radius: f32,
    ground: fn(&Palette) -> Color,
) -> Rounded {
    Rounded {
        handle,
        side,
        radius,
        ground,
    }
}

impl widget::Meta for Rounded {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Rounded {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.side), Length::Fixed(self.side))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(self.side, self.side, Size::new(self.side, self.side));
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let mut picture = raster::Image::new(self.handle.clone());
        picture.border_radius = self.radius.into();
        renderer.draw_image(picture, bounds, *viewport);
        // A band as wide as the corner's gap, whose inner edge is the
        // rounded outline, clipped to the picture: it paints the corners.
        let band = (self.radius * (std::f32::consts::SQRT_2 - 1.0)).ceil() + 1.0;
        let ground = (self.ground)(theme.palette());
        renderer.with_layer(bounds, |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds: bounds.expand(band),
                    border: Border {
                        color: ground,
                        width: band,
                        radius: (self.radius + band).into(),
                    },
                    ..Quad::default()
                },
                Color::TRANSPARENT,
            );
        });
    }
}
