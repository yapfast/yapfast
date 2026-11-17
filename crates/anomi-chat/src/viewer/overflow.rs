//! A box that lays its content out as large as it wants and shows its middle:
//! the zoomed picture, centred in the room and clipped to it. A stock
//! container would hand the content the room's limits, and a fixed size
//! larger than the room would come back cut down to it.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};

/// `content`, centred in the room it is given and clipped to it, whatever
/// size it asks for.
pub fn overflow<'a, Message: 'a>(content: impl View<Message> + 'a) -> Overflow<'a, Message> {
    Overflow {
        content: content.boxed(),
    }
}

pub struct Overflow<'a, Message> {
    content: Element<'a, Message>,
}

/// The pointer as the content sees it: only inside the room, so the part of a
/// large picture that is clipped away can't be pressed.
fn inside(cursor: mouse::Cursor, room: Rectangle) -> mouse::Cursor {
    if cursor.is_over(room) {
        cursor
    } else {
        mouse::Cursor::Unavailable
    }
}

impl<Message> widget::Meta for Overflow<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Overflow<'_, Message> {
    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let room = limits.max;
        let child = &mut tree.children[0];
        let open = layout::Limits::with_flags(
            Size::ZERO,
            room,
            Size::new(false, false),
            Size::new(true, true),
        );
        self.content.layout(child, renderer, &open);
        child.translation = Vector::new(
            (room.width - child.size.width) / 2.0,
            (room.height - child.size.height) / 2.0,
        );
        tree.size = room;
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
        let room = layout.bounds();
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        let Some(seen) = room.intersection(viewport) else {
            return;
        };
        self.content.update(
            child_tree,
            event,
            child,
            inside(cursor, room),
            renderer,
            shell,
            &seen,
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
        let room = layout.bounds();
        let Some(seen) = room.intersection(viewport) else {
            return;
        };
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        renderer.with_layer(room, |renderer| {
            self.content.draw(
                child_tree,
                renderer,
                theme,
                style,
                child,
                inside(cursor, room),
                &seen,
            );
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let room = layout.bounds();
        let Some(seen) = room.intersection(viewport) else {
            return mouse::Interaction::None;
        };
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(child_tree, child, inside(cursor, room), &seen, renderer)
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
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(child_tree, child, renderer, viewport, translation, window)
    }
}
