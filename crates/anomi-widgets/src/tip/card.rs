//! The tooltip's card, drawn over everything: where it goes against what
//! it names (`tip-card`'s `x` and `y`, `ui/yapfast.slint:5636`; the badge
//! card's, `:5649`), and how it grows from the edge facing it.

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::overlay::{self, Overlay};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{Operation, Tree, Widget as _};
use iced::{Color, Point, Rectangle, Shadow, Size, Transformation, Vector};

use anomi_theme::{Element, Renderer, Theme, metrics, motion, typography};

use super::Side;
use crate::menu::text;

/// The text card's height, and its text's room at each side (`:5635`).
const HEIGHT: f32 = 32.0;
const PAD: f32 = 10.0;
/// The closest a card comes to the window's sides and to its top.
const MARGIN: f32 = 8.0;
const MARGIN_TOP: f32 = 4.0;
/// The text card's shadow (`:5645`).
const SHADOW: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);
const SHADOW_BLUR: f32 = 12.0;
const SHADOW_DROP: f32 = 3.0;
const TEXT: f32 = typography::size::SMALL;
/// Over every other overlay, as the window's tooltip is over every popup.
const INDEX: f32 = 3.0;

/// What the card shows: text, measured by [`text_width`], or a card of its
/// own.
pub(super) enum Face<'a, 'b, M> {
    Text(&'a str, f32),
    Card(&'b mut Element<'a, M>, &'b mut Tree),
}

/// The width the text card's text takes.
pub(super) fn text_width(content: &str) -> f32 {
    text::width(content, TEXT, typography::BOLD)
}

/// Where the card goes, and how far it has arrived.
pub(super) struct Place {
    pub anchor: Rectangle,
    pub side: Side,
    pub gap: f32,
    pub window: Size,
    pub amount: f32,
}

impl Place {
    /// The card's rectangle for its size.
    pub fn bounds(&self, size: Size) -> Rectangle {
        let anchor = self.anchor;
        let across = (anchor.center_x() - size.width / 2.0)
            .min(self.window.width - size.width - MARGIN)
            .max(MARGIN);
        let at = match self.side {
            Side::Above => Point::new(across, (anchor.y - size.height - self.gap).max(MARGIN_TOP)),
            Side::Below => Point::new(across, anchor.y + anchor.height + self.gap),
            Side::Right => Point::new(
                anchor.x + anchor.width + self.gap,
                anchor.center_y() - size.height / 2.0,
            ),
        };
        Rectangle::new(at, size)
    }

    /// The edge facing what it names, which it grows from.
    fn origin(&self, card: Rectangle) -> Point {
        match self.side {
            Side::Above => Point::new(card.center_x(), card.y + card.height),
            Side::Below => Point::new(card.center_x(), card.y),
            Side::Right => Point::new(card.x, card.center_y()),
        }
    }
}

/// The card as an overlay; a card of its own is laid out here.
pub(super) fn overlay<'a, 'b, M>(
    mut face: Face<'a, 'b, M>,
    place: Place,
    renderer: &Renderer,
) -> overlay::Element<'b, M, Theme, Renderer>
where
    'a: 'b,
{
    let size = match &mut face {
        Face::Text(_, width) => Size::new(*width + 2.0 * PAD, HEIGHT),
        Face::Card(element, tree) => {
            let limits = layout::Limits::new(Size::ZERO, place.window);
            element.layout(tree, renderer, &limits);
            tree.size
        }
    };
    let card = place.bounds(size);
    overlay::Element::new(Box::new(Card { face, place, card }))
}

struct Card<'a, 'b, M> {
    face: Face<'a, 'b, M>,
    place: Place,
    card: Rectangle,
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
        let origin = self.place.origin(self.card);
        let scale = motion::popup_scale(self.place.amount);
        let grow = Transformation::translate(origin.x, origin.y)
            * Transformation::scale(scale)
            * Transformation::translate(-origin.x, -origin.y);
        renderer.with_layer(window, |renderer| {
            renderer.with_transformation(grow, |renderer| match &self.face {
                Face::Text(content, _) => draw_text(renderer, theme, content, self.card),
                Face::Card(element, tree) => {
                    let layout = Layout::new(tree.size).move_to(self.card.position());
                    element.draw(tree, renderer, theme, style, layout, cursor, &window);
                }
            });
        });
    }

    fn operate(&mut self, renderer: &Renderer, operation: &mut dyn Operation) {
        match &mut self.face {
            Face::Text(content, _) => operation.text(None, self.card, &mut (*content).to_owned()),
            Face::Card(element, tree) => {
                let layout = Layout::new(tree.size).move_to(self.card.position());
                let window = Rectangle::with_size(self.place.window);
                element.operate(tree, layout, &window, renderer, operation);
            }
        }
    }

    fn index(&self) -> f32 {
        INDEX
    }
}

fn draw_text(renderer: &mut Renderer, theme: &Theme, content: &str, card: Rectangle) {
    let palette = theme.palette();
    renderer.fill_quad(
        Quad {
            bounds: card,
            border: iced::Border::default().rounded(metrics::RADIUS_ROW),
            shadow: Shadow {
                color: SHADOW,
                offset: Vector::new(0.0, SHADOW_DROP),
                blur_radius: SHADOW_BLUR,
            },
            ..Quad::default()
        },
        palette.tooltip,
    );
    let line = text::line_height(TEXT);
    let at = Point::new(card.x + PAD, card.center_y() - line / 2.0);
    text::line(
        renderer,
        content,
        (TEXT, typography::BOLD),
        at,
        card.width - 2.0 * PAD + 1.0,
        palette.text,
        card,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(anchor: Rectangle, side: Side) -> Place {
        Place {
            anchor,
            side,
            gap: side.gap(),
            window: Size::new(1400.0, 860.0),
            amount: 1.0,
        }
    }

    const CARD: Size = Size::new(80.0, HEIGHT);

    #[test]
    fn above_it_centres_and_keeps_off_the_window_edges() {
        let button = Rectangle::new(Point::new(1384.0, 9.0), Size::new(16.0, 32.0));
        let card = place(button, Side::Above).bounds(CARD);
        assert_eq!(card.x, 1400.0 - MARGIN - CARD.width);
        assert_eq!(card.y, MARGIN_TOP);
    }

    #[test]
    fn right_of_a_rail_item_it_centres_on_its_height() {
        let item = Rectangle::new(Point::new(16.0, 100.0), Size::new(40.0, 40.0));
        let card = place(item, Side::Right).bounds(CARD);
        assert_eq!(card.position(), Point::new(66.0, 104.0));
    }

    #[test]
    fn below_a_header_button_it_hangs_6px_under() {
        let button = Rectangle::new(Point::new(1018.0, 9.0), Size::new(32.0, 32.0));
        let card = place(button, Side::Below).bounds(CARD);
        assert_eq!(card.position(), Point::new(994.0, 47.0));
    }
}
