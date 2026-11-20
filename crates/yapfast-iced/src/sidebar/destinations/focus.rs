//! iced's button handles pointer input; this wrapper gives these browser
//! actions focus, a visible focus ring, and Enter/Space activation too.

use iced::{
    Color, Element, Event, Length, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Id, Operation, Tree, operation::Focusable as _, tree},
    },
    keyboard::{self, key::Named},
};

use crate::app::Message;

use super::Destination;

pub(super) struct Focusable<'a> {
    face: Element<'a, Message>,
    destination: &'static Destination,
    ring: Color,
}

impl<'a> Focusable<'a> {
    pub fn new(face: Element<'a, Message>, destination: &'static Destination, ring: Color) -> Self {
        Self {
            face,
            destination,
            ring,
        }
    }
}

#[derive(Default)]
struct Focus(bool);

impl iced::advanced::widget::operation::Focusable for Focus {
    fn is_focused(&self) -> bool {
        self.0
    }
    fn focus(&mut self) {
        self.0 = true;
    }
    fn unfocus(&mut self) {
        self.0 = false;
    }
}

impl Widget<Message, Theme, iced::Renderer> for Focusable<'_> {
    fn size(&self) -> Size<Length> {
        self.face.as_widget().size()
    }
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Focus>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(Focus::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.face)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.face));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.face
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.focusable(
            Some(&Id::new(self.destination.id)),
            layout.bounds(),
            tree.state.downcast_mut::<Focus>(),
        );
        self.face
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let focus = tree.state.downcast_mut::<Focus>();
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            focus.0 = !shell.is_event_captured()
                && cursor
                    .position()
                    .is_some_and(|at| layout.bounds().contains(at) && viewport.contains(at));
            shell.request_redraw();
        }
        if shell.is_event_captured() {
            return;
        }
        if focus.0
            && let Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                repeat,
                ..
            }) = event
            && matches!(key, keyboard::Key::Named(Named::Enter | Named::Space))
            && !modifiers.command()
            && !modifiers.control()
            && !modifiers.alt()
        {
            if !repeat {
                shell.publish(self.destination.message());
            }
            shell.capture_event();
            return;
        }
        self.face.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.face.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if tree.state.downcast_ref::<Focus>().is_focused() {
            use iced::advanced::Renderer as _;
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds().shrink(2.0),
                    border: iced::Border {
                        color: self.ring,
                        width: 2.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                },
                Color::TRANSPARENT,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.face.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
}

impl<'a> From<Focusable<'a>> for Element<'a, Message> {
    fn from(button: Focusable<'a>) -> Self {
        Element::new(button)
    }
}
