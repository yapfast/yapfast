//! Ctrl with +, - or 0, or with the wheel, caught ahead of everything in the
//! window, as the Slint window takes them before Slint sees them
//! (`zoom::Keys`, `src/zoom.rs`): a list under the pointer doesn't scroll
//! as the window zooms, and a field doesn't take the key. The release of a
//! key that zoomed is kept from the window too. Ctrl let go, or the window
//! losing the focus, forgets the wheel's part of a step.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::keyboard::{self, key};
use iced::{Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::Renderer;
use anomi_theme::prelude::*;
use anomi_vm::shell::zoom::{self, Wheel};

use crate::message::Message;

/// The window's content, with the keys that zoom taken first.
pub fn keys<'a>(content: impl View<Message> + 'a) -> Keys<'a> {
    Keys {
        content: content.boxed(),
    }
}

pub struct Keys<'a> {
    content: Element<'a, Message>,
}

#[derive(Default)]
struct Held {
    modifiers: keyboard::Modifiers,
    wheel: Wheel,
    /// The keys whose press zoomed, so their release is kept too.
    pressed: Vec<key::Physical>,
}

impl Held {
    /// What the event asks of the zoom: taken (with a step or without), or
    /// left to the window.
    fn take(&mut self, event: &Event) -> Option<Option<zoom::Step>> {
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                self.modifiers = *modifiers;
                if !modifiers.control() {
                    self.wheel.forget();
                }
                None
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                ..
            }) => {
                let step = zoom::key_step(key, *physical_key, *modifiers)?;
                if !self.pressed.contains(physical_key) {
                    self.pressed.push(*physical_key);
                }
                Some(Some(step))
            }
            Event::Keyboard(keyboard::Event::KeyReleased { physical_key, .. }) => {
                let index = self.pressed.iter().position(|key| key == physical_key)?;
                self.pressed.swap_remove(index);
                Some(None)
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if self.modifiers.control() => {
                Some(self.wheel.scroll(*delta))
            }
            Event::Window(window::Event::Unfocused) => {
                // Ctrl may be let go elsewhere.
                *self = Self::default();
                None
            }
            _ => None,
        }
    }
}

impl widget::Meta for Keys<'_> {}

impl widget::Widget<Message, Theme, Renderer> for Keys<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Held>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Held::default())
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
        let (layout, tree) = layout.iter(&tree.children).next().expect("the content");
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, tree) = layout
            .iter_mut(&mut tree.children)
            .next()
            .expect("the content");
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
        if let Some(taken) = tree.state.downcast_mut::<Held>().take(event) {
            if let Some(step) = taken {
                shell.publish(Message::Zoom(step));
            }
            shell.capture_event();
            return;
        }
        let (layout, tree) = layout
            .iter_mut(&mut tree.children)
            .next()
            .expect("the content");
        self.content
            .update(tree, event, layout, cursor, renderer, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().expect("the content");
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
        let (layout, tree) = layout
            .iter_mut(&mut tree.children)
            .next()
            .expect("the content");
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
