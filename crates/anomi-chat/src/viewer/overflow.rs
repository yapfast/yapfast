//! A box that lays its content out as large as it wants and shows part of it:
//! the zoomed picture, centred in the room and clipped to it, moved by a left
//! drag. A stock container would hand the content the room's limits, and a
//! fixed size larger than the room would come back cut down to it.
//!
//! The pan lives in the tree. It is clamped so that on an axis where the
//! content is larger than the room its edges never come inside the room, and
//! scales with the content when that changes size, so the spot in the middle
//! stays in the middle; at the fit it is zero. A press and release that
//! moved less than [`SLOP`] reach the content as a click; a drag's release
//! does not.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Point, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};

/// How far the pointer may go between a press and its release for it to
/// still be a click.
const SLOP: f32 = 4.0;

#[derive(Default)]
struct State {
    /// How far the content is moved from the room's middle.
    pan: Vector,
    /// The content's size at the last layout.
    content: Size,
    drag: Option<Drag>,
}

/// A left press on the content: where the pointer was and the pan then.
struct Drag {
    from: Point,
    pan: Vector,
    moved: bool,
    /// Whether the content is larger than the room, so the drag pans it.
    larger: bool,
}

/// Where the content's origin sits in the room, pan included.
fn place(room: Size, content: Size, pan: Vector) -> Vector {
    Vector::new(
        (room.width - content.width) / 2.0 + pan.x,
        (room.height - content.height) / 2.0 + pan.y,
    )
}

/// `pan` kept where the content's edges stay out of the room on every axis
/// on which it is larger than the room; zero on the others.
pub fn clamp(pan: Vector, content: Size, room: Size) -> Vector {
    let axis = |pan: f32, content: f32, room: f32| {
        let limit = ((content - room) / 2.0).max(0.0);
        pan.clamp(-limit, limit)
    };
    Vector::new(
        axis(pan.x, content.width, room.width),
        axis(pan.y, content.height, room.height),
    )
}

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
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

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
        let size = child.size;
        let state = tree.state.downcast_mut::<State>();
        if state.content.width > 0.0 && state.content.height > 0.0 && state.content != size {
            state.pan.x *= size.width / state.content.width;
            state.pan.y *= size.height / state.content.height;
        }
        state.content = size;
        state.pan = clamp(state.pan, size, room);
        child.translation = place(room, size, state.pan);
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
        let state = tree.state.downcast_mut::<State>();
        let mut unpressed = false;
        match event {
            Event::Mouse(mouse::Event::CursorMoved { position })
                if state.drag.as_ref().is_some_and(|drag| drag.larger) =>
            {
                let drag = state.drag.as_mut().unwrap();
                let by = *position - drag.from;
                drag.moved |= by.x.hypot(by.y) > SLOP;
                if drag.moved {
                    state.pan = clamp(drag.pan + by, state.content, room.size());
                    child_tree.translation = place(room.size(), state.content, state.pan);
                    shell.request_redraw();
                    shell.capture_event();
                    return;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                match state.drag.take() {
                    Some(drag) if drag.moved => {
                        shell.capture_event();
                        return;
                    }
                    // A release whose press began elsewhere is not a click
                    // on the content.
                    None => unpressed = true,
                    Some(_) => {}
                }
            }
            _ => {}
        }
        let Some(seen) = room.intersection(viewport) else {
            return;
        };
        self.content.update(
            child_tree,
            event,
            child,
            if unpressed {
                mouse::Cursor::Unavailable
            } else {
                inside(cursor, room)
            },
            renderer,
            shell,
            &seen,
        );
        // A press on the content is the content's, never the window's behind
        // it (which closes); on a picture larger than the room it starts a drag.
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && !shell.is_event_captured()
            && let Some(at) = cursor.position()
            && room.contains(at)
            && child.bounds().contains(at)
        {
            let state = tree.state.downcast_mut::<State>();
            let larger = state.content.width > room.width || state.content.height > room.height;
            state.drag = Some(Drag {
                from: at,
                pan: state.pan,
                moved: false,
                larger,
            });
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
        if tree
            .state
            .downcast_ref::<State>()
            .drag
            .as_ref()
            .is_some_and(|drag| drag.moved)
        {
            return mouse::Interaction::Grabbing;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pan_keeps_the_content_over_the_room() {
        let content = Size::new(2000.0, 1000.0);
        let room = Size::new(800.0, 600.0);
        // Larger axes stop where the edge meets the room's.
        assert_eq!(
            clamp(Vector::new(900.0, -900.0), content, room),
            Vector::new(600.0, -200.0)
        );
        assert_eq!(
            clamp(Vector::new(100.0, 100.0), content, room),
            Vector::new(100.0, 100.0)
        );
        // An axis that fits in the room is centred.
        assert_eq!(
            clamp(Vector::new(50.0, 50.0), Size::new(500.0, 1000.0), room),
            Vector::new(0.0, 50.0)
        );
        assert_eq!(clamp(Vector::new(50.0, 50.0), room, room), Vector::ZERO);
    }
}
