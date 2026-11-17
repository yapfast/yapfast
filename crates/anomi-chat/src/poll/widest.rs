//! The poll dialog's width (`max(500px, durations.natural + 40px)`,
//! `ui/dialogs.slint:963`): at least a width, or wider when the durations,
//! laid out at their natural width, need more; and never taller than the
//! room less 48 px (`min(root.height - 48px, ..)`), the form scrolling
//! instead. The probe is only measured: it is never drawn and takes no
//! events.

use iced::advanced::layout;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Layout, Shell, Widget};
use iced::{Event, Length, Rectangle, Size, Vector, mouse};

use anomi_theme::{Renderer, Theme};

/// What the card leaves of the room's height (`root.height - 48px`).
const CLEAR: f32 = 48.0;

pub struct Widest<W> {
    /// The card, then the probe.
    children: [W; 2],
    least: f32,
    beside: f32,
}

/// `content` `least` wide, or as wide as `probe` and `beside` more.
pub fn widest<W>(content: W, probe: W, least: f32, beside: f32) -> Widest<W> {
    Widest {
        children: [content, probe],
        least,
        beside,
    }
}

impl<W> widget::Meta for Widest<W> {}

impl<W, Message> Widget<Message, Theme, Renderer> for Widest<W>
where
    W: Widget<Message, Theme, Renderer>,
{
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children[..]);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, self.children[0].size().height)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let room = limits.bounds();
        let [content, probe] = &mut self.children;
        let [card, measured] = &mut tree.children[..] else {
            return;
        };
        probe.layout(
            measured,
            renderer,
            &layout::Limits::new(Size::ZERO, Size::INFINITE),
        );
        let width = self
            .least
            .max(measured.size.width + self.beside)
            .min(room.width);
        content.layout(
            card,
            renderer,
            &layout::Limits::new(
                Size::new(width, 0.0),
                Size::new(width, (room.height - CLEAR).max(0.0)),
            ),
        );
        card.translation = Vector::ZERO;
        tree.size = Size::new(width, card.size.height);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.children[0].operate(tree, layout, viewport, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.children[0].update(tree, event, layout, cursor, renderer, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.children[0].mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.children[0].draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.children[0].overlay(tree, layout, renderer, viewport, translation, window)
    }
}
