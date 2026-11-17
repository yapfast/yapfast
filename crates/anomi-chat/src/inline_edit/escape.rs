//! Escape for the field of the message being edited. The composer field
//! leaves an Escape it has no use for to the window, whose own undoes what
//! the channel's composer is writing (`focus::Scope::on_escape`); the
//! edit's cancels the edit instead (`escaped => { InlineEdit.cancel();
//! return true; }`, `ui/messages.slint:2913`). So this wraps the field and,
//! when Escape reached it while it has the keys and nothing in it took the
//! key (the suggestions over it close first), says so and keeps the key
//! from the window.
//!
//! It keeps no state and asks for no frames.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{self, Id, Tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::keyboard::{self, key};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, Theme};

/// `content`, whose unclaimed Escape says `message` while it has the keys.
pub fn escape<'a, M: Clone + 'a>(content: Element<'a, M>, message: M) -> Element<'a, M> {
    Escape { content, message }.boxed()
}

struct Escape<'a, M> {
    content: Element<'a, M>,
    message: M,
}

/// Whether the first field inside has the keys.
struct Focused(Option<bool>);

impl Operation for Focused {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if self.0.is_none() {
            self.0 = Some(state.is_focused());
        }
    }
}

fn is_escape(event: &Event) -> bool {
    matches!(
        event,
        Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
            if matches!(key.as_ref(), keyboard::Key::Named(key::Named::Escape))
    )
}

impl<M> widget::Meta for Escape<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Escape<'_, M> {
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child, event, layout, cursor, renderer, shell, viewport);
        if !is_escape(event) || shell.is_event_captured() {
            return;
        }
        let mut focused = Focused(None);
        self.content
            .operate(child, layout, viewport, renderer, &mut focused);
        if focused.0 == Some(true) {
            shell.publish(self.message.clone());
            shell.capture_event();
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
        let (layout, child) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(child, renderer, theme, style, layout, cursor, viewport);
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

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(child, layout, renderer, viewport, translation, window)
    }
}
