//! A text input whose caret holds, shown, in an idle window, as the
//! composer's does (`Motion::idle`): the well of every [`super::Field`]
//! does it through [`update`], and [`Steady`] for an input of other looks,
//! the sidebar's search.

use iced::advanced::Widget as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Shell, mouse, overlay, shell};
use iced::{Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::{Element, Motion, Renderer, Theme, View};

/// Hands `event` to `content`, an input laid out at `layout`. While `idle`
/// the input hears the window come to the front before each frame, which
/// shows its caret anew, and the blink it then asks for is dropped: a
/// focused input in an idle window asks for no frame.
#[allow(clippy::too_many_arguments)]
pub(super) fn update<Message>(
    content: &mut Element<'_, Message>,
    tree: &mut Tree,
    event: &Event,
    layout: Layout,
    cursor: mouse::Cursor,
    renderer: &Renderer,
    shell: &mut Shell<'_, Message>,
    viewport: &Rectangle,
    idle: bool,
) {
    let held = idle && matches!(event, Event::Window(window::Event::RedrawRequested(_)));
    if held {
        let mut told = shell::Bus::new();
        let mut quiet = shell.local(&mut told);
        let front = Event::Window(window::Event::Focused);
        content.update(tree, &front, layout, cursor, renderer, &mut quiet, viewport);
    }
    let before = shell.redraw_request();
    content.update(tree, event, layout, cursor, renderer, shell, viewport);
    if held {
        Shell::replace_redraw_request(shell, before);
    }
}

/// An input of its own looks; see [`steady`].
pub struct Steady<'a, Message> {
    content: Element<'a, Message>,
    idle: bool,
}

/// `content`, a text input, its caret held in an idle window.
pub fn steady<'a, Message>(
    content: impl View<Message> + 'a,
    motion: Motion,
) -> Steady<'a, Message> {
    Steady {
        content: content.boxed(),
        idle: motion.idle,
    }
}

impl<Message> widget::Meta for Steady<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Steady<'_, Message> {
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let child = &mut tree.children[0];
        self.content.layout(child, renderer, limits);
        child.translation = Vector::ZERO;
        tree.size = child.size;
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
        self.content
            .operate(tree, layout, viewport, renderer, operation);
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
        update(
            &mut self.content,
            tree,
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
            self.idle,
        );
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
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
