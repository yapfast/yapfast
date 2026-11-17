//! Pressing and moving on the bar moves the window; a double click maximizes
//! it. The Slint side: the `TouchArea` under the bar's content
//! (`ui/yapfast/window-bar.slint:93`, `ui/title_bar.slint:41`).
//!
//! [`Drag`] wraps the bar and lets what is on it, the buttons, take the
//! pointer first. A press arms it, so that one press moves the window once:
//! the desktop takes the pointer for the move and the release never comes.
//! The move starts once the pointer is more than 3 px off the press in
//! either direction, so a click and a double click stay clicks.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay, renderer};
use iced::{Event, Length, Point, Rectangle, Size, Vector};

use anomi_theme::Renderer;
use anomi_theme::prelude::*;

/// How far the pointer may stray from the press before it is a move
/// (`window-bar.slint:101`).
pub const SLOP: f32 = 3.0;
/// A second press within this long of the first, and less than 10 px from it
/// (squared distance under 100), is a double click: Slint's
/// `DEFAULT_CLICK_INTERVAL` (`vendor/i-slint-core/input.rs:1419`) and its
/// distance check (`:1462`). It fires on that press's release inside the bar
/// (`items/input_items.rs:160-168`), so a second press that turns into a move
/// moves the window instead.
const DOUBLE: Duration = Duration::from_millis(500);
const DOUBLE_DISTANCE_SQUARED: f32 = 100.0;

pub struct Drag<'a, M> {
    content: Element<'a, M>,
    on_move: M,
    on_double: M,
}

pub fn drag<'a, M>(content: impl View<M> + 'a, on_move: M, on_double: M) -> Drag<'a, M> {
    Drag {
        content: content.boxed(),
        on_move,
        on_double,
    }
}

#[derive(Default)]
struct State {
    /// Where the press that may become a move began.
    armed: Option<Point>,
    last_press: Option<(Instant, Point)>,
    /// The armed press is the second of a double click.
    second: bool,
}

/// Whether the pointer has left the press by more than the slop.
pub fn strayed(from: Point, to: Point) -> bool {
    (to.x - from.x).abs() > SLOP || (to.y - from.y).abs() > SLOP
}

impl<M> widget::Meta for Drag<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Drag<'_, M> {
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
        let over = cursor.is_over(layout.bounds());
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        if shell.is_event_captured() {
            return;
        }
        let state = tree.state.downcast_mut::<State>();
        // The bar's touch area grabs every press over it, so the layers
        // under the bar never see one.
        if over && let Event::Mouse(mouse::Event::ButtonPressed(button)) = event {
            shell.capture_event();
            if *button != mouse::Button::Left {
                return;
            }
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                let now = Instant::now();
                let at = cursor.position();
                state.second = match (state.last_press, at) {
                    (Some((then, from)), Some(at)) => {
                        let (dx, dy) = (at.x - from.x, at.y - from.y);
                        now - then < DOUBLE && dx * dx + dy * dy < DOUBLE_DISTANCE_SQUARED
                    }
                    _ => false,
                };
                state.last_press = if state.second {
                    None
                } else {
                    at.map(|at| (now, at))
                };
                state.armed = at;
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.second && over && state.armed.is_some() {
                    shell.publish(self.on_double.clone());
                }
                state.second = false;
                state.armed = None;
            }
            Event::Mouse(mouse::Event::CursorLeft) => state.armed = None,
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(from) = state.armed
                    && strayed(from, *position)
                {
                    state.armed = None;
                    state.second = false;
                    state.last_press = None;
                    shell.publish(self.on_move.clone());
                }
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
        let interaction = self
            .content
            .mouse_interaction(tree, layout, cursor, viewport, renderer);
        // Over the bar the layers under it see no pointer.
        if interaction == mouse::Interaction::None && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Idle
        } else {
            interaction
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slop_is_three_pixels_either_way() {
        let from = Point::new(100.0, 10.0);
        assert!(!strayed(from, Point::new(103.0, 13.0)));
        assert!(strayed(from, Point::new(103.5, 10.0)));
        assert!(strayed(from, Point::new(100.0, 6.0)));
    }
}
