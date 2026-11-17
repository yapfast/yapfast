//! Escape in the search field. In Slint, `HeaderSearch`'s key handler calls
//! `clear()` when the field has text (`ui/header.slint:117`-`:122`) and the
//! forum's `clear` empties the query and searches "" (`ui/forum.slint:550`).
//! This wraps the field: while it has the keys and holds text, Escape says
//! `message` and is taken; in an empty field it passes on.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{self, Id, Tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::keyboard::{self, key};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme};

pub struct OnClear<'a, Message> {
    content: Element<'a, Message>,
    message: Option<Message>,
}

impl<'a, Message> OnClear<'a, Message> {
    /// `message` is what Escape says; `None` when the field is empty.
    pub fn new(content: Element<'a, Message>, message: Option<Message>) -> Self {
        Self { content, message }
    }
}

/// Whether the first focusable under it has the keys.
struct Probe(Option<bool>);

impl Operation for Probe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if self.0.is_none() {
            self.0 = Some(state.is_focused());
        }
    }
}

impl<Message> widget::Meta for OnClear<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for OnClear<'_, Message> {
    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let child = &mut tree.children[0];
        self.content.layout(child, renderer, limits);
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(child, layout, viewport, renderer, operation);
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        if let (Some(message), Event::Keyboard(keyboard::Event::KeyPressed { key, .. })) =
            (&self.message, event)
            && matches!(key.as_ref(), keyboard::Key::Named(key::Named::Escape))
        {
            let mut probe = Probe(None);
            self.content
                .operate(child, layout, viewport, renderer, &mut probe);
            if probe.0 == Some(true) {
                shell.publish(message.clone());
                shell.capture_event();
                return;
            }
        }
        self.content
            .update(child, event, layout, cursor, renderer, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, child) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(child, layout, cursor, viewport, renderer)
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
        let (layout, child) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(child, renderer, theme, style, layout, cursor, viewport);
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(child, layout, renderer, viewport, translation, window)
    }
}
