//! A name underlined while the pointer is on it, as a link is, with the
//! pointing hand (`author-touch`, `ui/messages.slint:3233`-`:3237`): the
//! line 1 px thick, 1 px under the middle of the text's line and half its
//! size.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::{Color, Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Palette, Renderer, Theme, View};

pub struct Underline<'a, M> {
    content: Element<'a, M>,
    /// The text's size, which places the line.
    size: f32,
    color: Box<dyn Fn(&Palette) -> Color + 'a>,
}

/// `content`, a line of text at `size`, underlined in `color` under the
/// pointer.
pub fn underline<'a, M>(
    content: impl View<M> + 'a,
    size: f32,
    color: impl Fn(&Palette) -> Color + 'a,
) -> Underline<'a, M> {
    Underline {
        content: content.boxed(),
        size,
        color: Box::new(color),
    }
}

#[derive(Default)]
struct Hovered(bool);

impl<M> widget::Meta for Underline<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Underline<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Hovered>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Hovered::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
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
        let bounds = layout.bounds();
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        if let Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) = event {
            let over = cursor.is_over(bounds);
            let hovered = tree.state.downcast_mut::<Hovered>();
            if hovered.0 != over {
                hovered.0 = over;
                shell.request_redraw();
            }
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
        let bounds = layout.bounds();
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(child_tree, renderer, theme, style, child, cursor, viewport);
        if tree.state.downcast_ref::<Hovered>().0 {
            let line = Rectangle {
                y: bounds.y + ((bounds.height + self.size) / 2.0 + 1.0).round(),
                height: 1.0,
                ..bounds
            };
            renderer.fill_quad(
                Quad {
                    bounds: line,
                    ..Quad::default()
                },
                (self.color)(theme.palette()),
            );
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
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
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(child_tree, child, viewport, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(child_tree, child, renderer, viewport, translation, window)
    }
}
