//! The card's one widget: what a press, a drag and the pointer's being on it
//! ask (`StreamPip`'s `TouchArea`, `ui/voice.slint:704`-`:747`).
//!
//! It holds three layers, each filling it: the `base` (the picture, or what
//! it waits for), the `marks` always over it (the LIVE tag, the viewers) and
//! the `hover` that shows only while the pointer is on it and it isn't being
//! dragged (the shade, the title, the stop button). Marks and hover each
//! open a layer of their own, as iced draws quads, then pictures, then text
//! within one, and these must lie over the picture. A press on a button in
//! them stops there; any other left press is the card's own: a release after
//! moving less than 4 px asks for the call, and moving more is a drag, which
//! asks for how far the card has gone, as a whole offset from its corner.
//! The pointer is a hand on it, a fist while it is dragged.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Point, Rectangle, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, View};
use anomi_vm::voice::pip::Intent;

/// How far the pointer must go with the button down before it is a drag
/// rather than a click (`:714`).
const SLOP: f32 = 4.0;

pub struct Grab<'a> {
    /// The base, the marks, the hover layer.
    children: [Element<'a, Intent>; 3],
    /// Where a drag has taken the card, which the layer is drawn at: how
    /// far the pointer has gone is added to it.
    offset: Vector,
}

pub fn grab<'a>(
    base: impl View<Intent> + 'a,
    marks: impl View<Intent> + 'a,
    hover: impl View<Intent> + 'a,
    offset: Vector,
) -> Grab<'a> {
    Grab {
        children: [base.boxed(), marks.boxed(), hover.boxed()],
        offset,
    }
}

#[derive(Default)]
pub struct State {
    hovered: bool,
    /// Where the left button went down, in the card's own coordinates.
    pressed: Option<Point>,
    dragging: bool,
}

impl State {
    /// The hover layer shows.
    fn raised(&self) -> bool {
        self.hovered && !self.dragging
    }
}

impl widget::Meta for Grab<'_> {}

impl widget::Widget<Intent, Theme, Renderer> for Grab<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn size(&self) -> Size<Length> {
        widget::Widget::size(&self.children[0])
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.children[0].layout(&mut tree.children[0], renderer, limits);
        let size = tree.children[0].size;
        let inside = layout::Limits::new(size, size);
        for (child, tree) in self.children.iter_mut().zip(&mut tree.children).skip(1) {
            child.layout(tree, renderer, &inside);
        }
        tree.size = size;
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
        let over = cursor.is_over(bounds);
        let state = tree.state.downcast_mut::<State>();
        if state.hovered != over {
            state.hovered = over;
            shell.request_redraw();
        }
        let raised = state.raised();
        // The layers over the picture have the first say: a button or the
        // viewers take their press.
        let shown = if raised { 3 } else { 2 };
        for ((child, tree), element) in layout
            .iter_mut(&mut tree.children)
            .zip(self.children.iter_mut())
            .skip(1)
            .take(shown - 1)
        {
            element.update(tree, event, child, cursor, renderer, shell, viewport);
        }
        // The base captures nothing, but its picture and the spinner need
        // every redraw to follow a feed and turn.
        if let Some(((child, tree), element)) = layout
            .iter_mut(&mut tree.children)
            .zip(self.children.iter_mut())
            .next()
        {
            element.update(tree, event, child, cursor, renderer, shell, viewport);
        }
        if shell.is_event_captured() {
            return;
        }
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                state.pressed = cursor.position();
                state.dragging = false;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let (Some(from), Some(now)) = (state.pressed, cursor.position()) else {
                    return;
                };
                let moved = now - from;
                if !state.dragging && moved.x.abs() <= SLOP && moved.y.abs() <= SLOP {
                    return;
                }
                if !state.dragging {
                    state.dragging = true;
                    shell.request_redraw();
                }
                // The card follows the pointer, so what the pointer has gone
                // from where it took hold is what the card has to go, from
                // where it is now.
                shell.publish(Intent::Drag(moved + self.offset));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.pressed.take().is_none() {
                    return;
                }
                if std::mem::take(&mut state.dragging) {
                    shell.publish(Intent::Dropped);
                    shell.request_redraw();
                } else if over {
                    shell.publish(Intent::Open);
                }
                shell.capture_event();
            }
            _ => {}
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
        let bounds = layout.bounds();
        let raised = tree.state.downcast_ref::<State>().raised();
        let mut layers = layout.iter(&tree.children).zip(&self.children);
        if let Some(((layout, tree), base)) = layers.next() {
            base.draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
        for (index, ((layout, tree), layer)) in layers.enumerate() {
            if index == 1 && !raised {
                break;
            }
            renderer.with_layer(bounds, |renderer| {
                layer.draw(tree, renderer, theme, style, layout, cursor, viewport);
            });
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
        let state = tree.state.downcast_ref::<State>();
        if state.dragging {
            return mouse::Interaction::Grabbing;
        }
        if !cursor.is_over(layout.bounds()) {
            return mouse::Interaction::None;
        }
        // A button in the layers over it has its own.
        let shown = if state.raised() { 3 } else { 2 };
        let own = layout
            .iter(&tree.children)
            .zip(&self.children)
            .skip(1)
            .take(shown - 1)
            .map(|((layout, tree), layer)| {
                layer.mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .find(|interaction| *interaction != mouse::Interaction::None);
        own.unwrap_or(mouse::Interaction::Pointer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let shown = if tree.state.downcast_ref::<State>().raised() {
            3
        } else {
            2
        };
        for ((layout, tree), layer) in layout
            .iter_mut(&mut tree.children)
            .zip(&mut self.children)
            .take(shown)
        {
            layer.operate(tree, layout, viewport, renderer, operation);
        }
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
        let raised = tree.state.downcast_ref::<State>().raised();
        let shown = if raised { 3 } else { 2 };
        // The viewers' tip opens below them.
        layout
            .iter_mut(&mut tree.children)
            .zip(self.children.iter_mut())
            .skip(1)
            .take(shown - 1)
            .flat_map(|((layout, tree), layer)| {
                layer.overlay(tree, layout, renderer, viewport, translation, window)
            })
            .collect()
    }
}
