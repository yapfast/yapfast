//! Discord's outline round the sidebar: its top and left edges meeting in a
//! 12 px rounded corner, the left line running down to the account card,
//! and no line between the sidebar and the conversation, whose grounds
//! alone differ. The Slint window draws a bordered rectangle 16 px larger
//! than the sheet, which clips its right and bottom sides away
//! (`ui/yapfast.slint:2978`); this draws the same, clipped to its bounds.

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree};
use iced::border::Radius;
use iced::{Border, Color, Length, Rectangle, Size};

use anomi_theme::{Renderer, Theme};

/// How far the bordered rectangle reaches past the sheet, and its corner.
const BEYOND: f32 = 16.0;
const CORNER: f32 = 12.0;

/// The outline over a sheet of the size it fills.
pub struct Outline;

impl widget::Meta for Outline {}

impl<M> widget::Widget<M, Theme, Renderer> for Outline {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
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
        let bounds = layout.bounds();
        let outline = theme.palette().outline;
        renderer.with_layer(bounds, |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        width: bounds.width + BEYOND,
                        height: bounds.height + BEYOND,
                        ..bounds
                    },
                    border: Border {
                        color: outline,
                        width: 1.0,
                        radius: Radius::from(CORNER),
                    },
                    ..Quad::default()
                },
                Color::TRANSPARENT,
            );
        });
    }
}
