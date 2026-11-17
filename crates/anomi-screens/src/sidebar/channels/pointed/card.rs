//! The card beside a row, drawn over the window: where `VoiceCardView` goes
//! (`ui/rail.slint:749`, `:750`), 8 px right of the row and kept 8 px inside
//! the window, centred on the row's middle and kept 8 px from the window's
//! top and foot; and how it grows from its left edge's middle (`:754`,
//! `:756`), as `PopupEntry` has it. iced draws no layer at an opacity, so
//! it grows without fading in.

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::overlay::{self, Overlay};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{Operation, Tree, Widget as _};
use iced::{Point, Rectangle, Size, Transformation};

use anomi_theme::{Element, Renderer, Theme, motion};

/// Between the row and the card, and the least between the card and the
/// window's edges.
const GAP: f32 = 8.0;
const MARGIN: f32 = 8.0;
/// Over the window's popups, under its tooltip.
const INDEX: f32 = 2.0;

/// Where the card goes, and how far it has arrived.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Place {
    /// The row, in the window.
    pub anchor: Rectangle,
    pub window: Size,
    pub amount: f32,
}

impl Place {
    /// The card's rectangle for its size.
    pub fn bounds(&self, size: Size) -> Rectangle {
        let anchor = self.anchor;
        let x = (anchor.x + anchor.width + GAP).min(self.window.width - size.width - MARGIN);
        let y = (anchor.center_y() - size.height / 2.0)
            .min(self.window.height - size.height - MARGIN)
            .max(MARGIN);
        Rectangle::new(Point::new(x, y), size)
    }
}

/// The card as an overlay, laid out here at its own size.
pub(super) fn overlay<'a, 'b, M>(
    card: &'b mut Element<'a, M>,
    tree: &'b mut Tree,
    place: Place,
    renderer: &Renderer,
) -> overlay::Element<'b, M, Theme, Renderer>
where
    'a: 'b,
{
    card.layout(
        tree,
        renderer,
        &layout::Limits::new(Size::ZERO, place.window),
    );
    let bounds = place.bounds(tree.size);
    overlay::Element::new(Box::new(Card {
        card,
        tree,
        place,
        bounds,
    }))
}

struct Card<'a, 'b, M> {
    card: &'b mut Element<'a, M>,
    tree: &'b mut Tree,
    place: Place,
    bounds: Rectangle,
}

impl<M> Overlay<M, Theme, Renderer> for Card<'_, '_, M> {
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        cursor: mouse::Cursor,
    ) {
        let window = Rectangle::with_size(self.place.window);
        let origin = Point::new(self.bounds.x, self.bounds.center_y());
        let scale = motion::popup_scale(self.place.amount);
        let grow = Transformation::translate(origin.x, origin.y)
            * Transformation::scale(scale)
            * Transformation::translate(-origin.x, -origin.y);
        let layout = Layout::new(self.tree.size).move_to(self.bounds.position());
        renderer.with_layer(window, |renderer| {
            renderer.with_transformation(grow, |renderer| {
                self.card
                    .draw(self.tree, renderer, theme, style, layout, cursor, &window);
            });
        });
    }

    fn operate(&mut self, renderer: &Renderer, operation: &mut dyn Operation) {
        let layout = Layout::new(self.tree.size).move_to(self.bounds.position());
        let window = Rectangle::with_size(self.place.window);
        self.card
            .operate(self.tree, layout, &window, renderer, operation);
    }

    fn index(&self) -> f32 {
        INDEX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(1400.0, 860.0);
    const CARD: Size = Size::new(232.0, 140.0);

    fn place(anchor: Rectangle) -> Place {
        Place {
            anchor,
            window: WINDOW,
            amount: 1.0,
        }
    }

    #[test]
    fn beside_its_row_it_centres_on_its_middle() {
        let row = Rectangle::new(Point::new(80.0, 250.0), Size::new(288.0, 32.0));
        let card = place(row).bounds(CARD);
        assert_eq!(card.position(), Point::new(376.0, 266.0 - 70.0));
    }

    #[test]
    fn it_keeps_inside_the_window() {
        let low = Rectangle::new(Point::new(80.0, 840.0), Size::new(288.0, 32.0));
        assert_eq!(place(low).bounds(CARD).y, 860.0 - 140.0 - 8.0);
        let high = Rectangle::new(Point::new(80.0, 0.0), Size::new(288.0, 32.0));
        assert_eq!(place(high).bounds(CARD).y, 8.0);
        let wide = Rectangle::new(Point::new(1200.0, 300.0), Size::new(288.0, 32.0));
        assert_eq!(place(wide).bounds(CARD).x, 1400.0 - 232.0 - 8.0);
    }
}
