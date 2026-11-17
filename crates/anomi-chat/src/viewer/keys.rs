//! What the viewer hears of the keyboard and the wheel: the arrow keys step
//! through the message's pictures (`ui/messages.slint:2667`), `+`, `-` and
//! `0` and the wheel zoom. A key or a wheel turn that something inside took
//! (a button with the focus) is not heard. Escape is the layer's.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Shell, mouse};
use iced::keyboard::{self, Key, key::Named};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};
use anomi_vm::chat::viewer::{Intent, Zoom};

/// What of the keyboard and the wheel the viewer asks for.
pub fn heard(event: &Event, steps: bool, over: bool) -> Option<Intent> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modified_key,
            modifiers,
            ..
        }) if !modifiers.control() && !modifiers.alt() && !modifiers.logo() => {
            match (key.as_ref(), modified_key.as_ref()) {
                (Key::Named(Named::ArrowLeft), _) if steps => Some(Intent::Step(-1)),
                (Key::Named(Named::ArrowRight), _) if steps => Some(Intent::Step(1)),
                (_, Key::Character("+" | "=")) => Some(Intent::Zoom(Zoom::In)),
                (_, Key::Character("-" | "_")) => Some(Intent::Zoom(Zoom::Out)),
                (_, Key::Character("0")) => Some(Intent::Zoom(Zoom::Reset)),
                _ => None,
            }
        }
        Event::Mouse(mouse::Event::WheelScrolled { delta }) if over => {
            let (mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. }) =
                *delta;
            if y > 0.0 {
                Some(Intent::Zoom(Zoom::In))
            } else if y < 0.0 {
                Some(Intent::Zoom(Zoom::Out))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Wraps the viewer's content to hear [`heard`].
pub struct Keys<'a> {
    content: Element<'a, Intent>,
    steps: bool,
}

pub fn keys<'a>(content: impl View<Intent> + 'a, steps: bool) -> Keys<'a> {
    Keys {
        content: content.boxed(),
        steps,
    }
}

impl widget::Meta for Keys<'_> {}

impl widget::Widget<Intent, Theme, Renderer> for Keys<'_> {
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
        shell: &mut Shell<'_, Intent>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        if !shell.is_event_captured()
            && let Some(intent) = heard(event, self.steps, cursor.is_over(bounds))
        {
            shell.publish(intent);
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
    ) -> Vec<overlay::Element<'b, Intent, Theme, Renderer>> {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
