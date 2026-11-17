//! The poll's foot as Slint's `FlexboxLayout` lays it (`ui/messages.slint:757`):
//! the start at the start and the end at the end of one line when both fit,
//! otherwise the end on a line of its own at its end. A wrapping `row` aligns
//! its lines to the widest of them, not to its room, so the buttons wouldn't
//! reach the card's edge.

use iced::advanced::layout;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Layout, Shell, Widget};
use iced::{Event, Length, Rectangle, Size, Vector, mouse};

use anomi_theme::{Renderer, Theme};

pub struct Ends<W> {
    children: [W; 2],
    spacing: f32,
}

/// `start` and `end` on one line, centred on it, or `end` below `start`.
pub fn ends<W>(start: W, end: W, spacing: f32) -> Ends<W> {
    Ends {
        children: [start, end],
        spacing,
    }
}

impl<W> widget::Meta for Ends<W> {}

impl<W, Message> Widget<Message, Theme, Renderer> for Ends<W>
where
    W: Widget<Message, Theme, Renderer>,
{
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children[..]);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let bounds = limits.width(Length::Fill).bounds();
        let room = bounds.width;
        let loose = layout::Limits::new(Size::ZERO, bounds);
        for (child, tree) in self.children.iter_mut().zip(&mut tree.children) {
            child.layout(tree, renderer, &loose);
        }
        let [start, end] = &mut tree.children[..] else {
            return;
        };
        let (a, b) = (start.size, end.size);
        let height = if a.width + self.spacing + b.width <= room {
            let height = a.height.max(b.height);
            start.translation = Vector::new(0.0, (height - a.height) / 2.0);
            end.translation = Vector::new(room - b.width, (height - b.height) / 2.0);
            height
        } else {
            start.translation = Vector::ZERO;
            end.translation = Vector::new((room - b.width).max(0.0), a.height + self.spacing);
            a.height + self.spacing + b.height
        };
        tree.size = limits.resolve(Length::Fill, Length::Shrink, Size::new(room, height));
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            for (child, (layout, tree)) in self
                .children
                .iter_mut()
                .zip(layout.iter_mut(&mut tree.children))
            {
                child.operate(tree, layout, viewport, renderer, operation);
            }
        });
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
        for (child, (layout, tree)) in self
            .children
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            child.update(tree, event, layout, cursor, renderer, shell, viewport);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(layout.iter(&tree.children))
            .map(|(child, (layout, tree))| {
                child.mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        for (child, (layout, tree)) in self.children.iter().zip(layout.iter(&tree.children)) {
            child.draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
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
        overlay::from_children(
            &mut self.children[..],
            tree,
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}
