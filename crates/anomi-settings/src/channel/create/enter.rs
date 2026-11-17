//! Enter anywhere in the dialog confirms it, as the `FocusScope` of Slint's
//! `ChannelDialog` does (`ui/server.slint:545`, `key-pressed` on Return). A
//! wrapper as wide as what it holds: the content has the key first (a focused
//! button presses itself on Enter, a field submits), and one it left is the
//! dialog's.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::keyboard::{self, Key, key::Named};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};

struct Enter<'a, M> {
    content: Element<'a, M>,
    message: Option<M>,
}

/// `content`, sending `message` on an Enter nothing inside took; no
/// message, no reaction (a dialog already waiting).
pub fn enter<'a, M: Clone + 'a>(
    content: impl View<M> + 'a,
    message: Option<M>,
) -> impl View<M> + 'a {
    Enter {
        content: content.boxed(),
        message,
    }
}

impl<M> widget::Meta for Enter<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Enter<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
    }

    fn state(&self) -> tree::State {
        tree::State::None
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
        let (child, tree) = layout.iter(&tree.children).next().expect("the content");
        self.content
            .draw(tree, renderer, theme, style, child, cursor, viewport);
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
            let (child, tree) = layout
                .iter_mut(&mut tree.children)
                .next()
                .expect("the content");
            self.content
                .operate(tree, child, viewport, renderer, operation);
        });
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
        let (child, tree) = layout
            .iter_mut(&mut tree.children)
            .next()
            .expect("the content");
        self.content
            .update(tree, event, child, cursor, renderer, shell, viewport);
        if shell.is_event_captured() {
            return;
        }
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(Named::Enter),
            modifiers,
            repeat: false,
            ..
        }) = event
            && !modifiers.control()
            && !modifiers.alt()
            && !modifiers.logo()
            && let Some(message) = &self.message
        {
            shell.publish(message.clone());
            shell.capture_event();
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
        let (child, tree) = layout.iter(&tree.children).next().expect("the content");
        self.content
            .mouse_interaction(tree, child, cursor, viewport, renderer)
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
        let (child, tree) = layout
            .iter_mut(&mut tree.children)
            .next()
            .expect("the content");
        self.content
            .overlay(tree, child, renderer, viewport, translation, window)
    }
}
