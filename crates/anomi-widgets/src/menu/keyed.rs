//! A menu a screen lays out itself, taking the keys while its layer has
//! them: the arrows, Enter and Space, and Escape while it's told a submenu
//! shows (else Escape is the layer's, which closes the menu), each sent as
//! a [`Key`] for the view-model to move by. Keys a widget inside took, a
//! focused slider's arrows or a focused row's Enter, aren't its, nor keys
//! with Ctrl, Alt or Super held.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Shell, mouse};
use iced::keyboard::{self, key::Named};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};
use anomi_vm::menu::Key;

pub struct Keyed<'a, M> {
    content: Element<'a, M>,
    on_key: Box<dyn Fn(Key) -> M + 'a>,
    escape: bool,
}

/// `content` taking the keys, each sent through `on_key`.
pub fn keyed<'a, M>(content: impl View<M> + 'a, on_key: impl Fn(Key) -> M + 'a) -> Keyed<'a, M> {
    Keyed {
        content: content.boxed(),
        on_key: Box::new(on_key),
        escape: false,
    }
}

impl<M> Keyed<'_, M> {
    /// Escape is its own: a submenu shows, which Escape closes first.
    pub fn escape(mut self, escape: bool) -> Self {
        self.escape = escape;
        self
    }

    /// The key `event` presses, if it's one of the menu's.
    fn key(&self, event: &Event) -> Option<Key> {
        let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event else {
            return None;
        };
        if modifiers.control() || modifiers.alt() || modifiers.logo() {
            return None;
        }
        Some(match key.as_ref() {
            keyboard::Key::Named(Named::ArrowUp) => Key::Up,
            keyboard::Key::Named(Named::ArrowDown) => Key::Down,
            keyboard::Key::Named(Named::ArrowRight) => Key::Right,
            keyboard::Key::Named(Named::ArrowLeft) => Key::Left,
            keyboard::Key::Named(Named::Enter | Named::Space) | keyboard::Key::Character(" ") => {
                Key::Enter
            }
            keyboard::Key::Named(Named::Escape) if self.escape => Key::Escape,
            _ => return None,
        })
    }
}

impl<M> widget::Meta for Keyed<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Keyed<'_, M> {
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
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        if shell.is_event_captured() {
            return;
        }
        if let Some(key) = self.key(event) {
            shell.publish((self.on_key)(key));
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

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
