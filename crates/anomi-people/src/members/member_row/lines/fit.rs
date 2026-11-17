//! The name's line: the name, then its marks, in a row that keeps every
//! mark at its own width and gives the name the room left, as Slint's
//! `HorizontalLayout` does (`ui/people.slint:300`-`:337`). An iced row lays
//! its children out in order, so a long name would take the room first and
//! push the marks out. The name is capped at its natural width, so the
//! nameplate's `extent` around the lines still sees where the text ends.
//! After `rail_item`: the children's calls are forwarded.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme};

/// The name's line; see [`fit`].
pub struct Fit<'a, Message> {
    /// The name first, then the marks.
    children: Vec<Element<'a, Message>>,
    spacing: f32,
}

/// `name` and `marks` in a line `spacing` apart; the name gives way.
pub fn fit<'a, Message>(
    name: Element<'a, Message>,
    marks: Vec<Element<'a, Message>>,
    spacing: f32,
) -> Fit<'a, Message> {
    let mut children = vec![name];
    children.extend(marks);
    Fit { children, spacing }
}

impl<Message> widget::Meta for Fit<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Fit<'_, Message> {
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let bounds = limits.bounds();
        let loose = limits.loose();
        let (name, marks) = self.children.split_first_mut().unwrap();
        let (name_tree, mark_trees) = tree.children.split_first_mut().unwrap();
        // The marks first, at their natural width.
        let mut taken = 0.0;
        for (mark, mark_tree) in marks.iter_mut().zip(mark_trees.iter_mut()) {
            mark.layout(mark_tree, renderer, &loose);
            taken += self.spacing + mark_tree.size.width;
        }
        // The name in the room left: it is `Shrink`, so it comes out as wide
        // as its text, or the room if that is less.
        let room = (bounds.width - taken).max(0.0);
        let narrow = layout::Limits::new(Size::ZERO, Size::new(room, bounds.height));
        name.layout(name_tree, renderer, &narrow);
        let height = tree
            .children
            .iter()
            .map(|child| child.size.height)
            .fold(0.0, f32::max);
        let mut x = 0.0;
        for child in &mut tree.children {
            child.translation = Vector::new(x, (height - child.size.height) / 2.0);
            x += child.size.width + self.spacing;
        }
        tree.size = Size::new((x - self.spacing).max(0.0), height);
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
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}
