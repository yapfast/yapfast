//! A line in two parts: the body, which gives way when the room is short
//! (its text elides), and the tail after it, which never does. A thread's
//! line is the person and the message, then " • 5m ago": the message is as
//! wide as it is, short of the row, and the time follows it (`ui/header.slint`
//! `:350`-`:366`). A row of two can't say that: it lays out the first
//! child's whole width before the second's.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};

pub struct Ends<'a, M> {
    parts: [Element<'a, M>; 2],
}

/// `body` that may be cut short, then `tail`, whole.
pub fn ends<'a, M>(body: impl View<M> + 'a, tail: impl View<M> + 'a) -> Ends<'a, M> {
    Ends {
        parts: [body.boxed(), tail.boxed()],
    }
}

impl<M> widget::Meta for Ends<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Ends<'_, M> {
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.parts);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let loose = limits.loose();
        let [body, tail] = &mut self.parts;
        let [body_tree, tail_tree] = &mut tree.children[..] else {
            return;
        };
        tail.layout(tail_tree, renderer, &loose);
        let tail_size = tail_tree.size;
        body.layout(
            body_tree,
            renderer,
            &loose.shrink(Size::new(tail_size.width, 0.0)),
        );
        let body_size = body_tree.size;
        layout::next_to_each_other(tree, body_size, tail_size, 0.0);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        for (part, (layout, tree)) in self
            .parts
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            part.update(tree, event, layout, cursor, renderer, shell, viewport);
        }
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
        for (part, (layout, tree)) in self.parts.iter().zip(layout.iter(&tree.children)) {
            part.draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        for (part, (layout, tree)) in self
            .parts
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            part.operate(tree, layout, viewport, renderer, operation);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        _tree: &'b mut Tree,
        _layout: Layout,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        _translation: Vector,
        _window: iced::Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        Vec::new()
    }
}
