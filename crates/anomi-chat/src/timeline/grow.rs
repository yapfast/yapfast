//! How the timeline's bars arrive: grown from 96% at the edge they hang
//! from, over `POPUP`, as `PopupEntry` brings them in (`enter`,
//! `transform-scale` and `transform-origin` of `UnreadBar` and `PresentBar`,
//! `ui/messages.slint:2477`, `:2503`; `ui/theme.slint:234`). They don't
//! fade: tiny-skia would draw a fading bar into a layer as large as the
//! window (see `anomi_theme::motion::Popup`). A bar shows whole at once
//! while the interface holds still, and asks for frames only while it
//! grows. It leaves at once, as `PopupEntry` has no way out.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Renderer as _, Shell, mouse, overlay};
use iced::{Event, Length, Point, Rectangle, Size, Transformation, Vector, window};

use anomi_theme::motion::{self, Pace, Popup};
use anomi_theme::{Element, Motion, Renderer, Theme, View};

/// The edge a bar grows from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// It hangs under the top of the timeline.
    Top,
    /// It sits on the bottom.
    Bottom,
}

pub struct Grow<'a, M> {
    content: Element<'a, M>,
    edge: Edge,
    motion: Motion,
}

/// `content`, arriving from `edge` as it first shows.
pub fn grow<'a, M>(content: impl View<M> + 'a, edge: Edge, motion: Motion) -> Grow<'a, M> {
    Grow {
        content: content.boxed(),
        edge,
        motion,
    }
}

struct State {
    entry: Popup,
    now: Instant,
}

impl<M> widget::Meta for Grow<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Grow<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        let now = Instant::now();
        tree::State::new(State {
            entry: Popup::entering(Pace::Popover, self.motion, now),
            now,
        })
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
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let state = tree.state.downcast_mut::<State>();
            state.now = *now;
            if self.motion.still() {
                state.entry.land();
            } else if state.entry.is_animating(*now) {
                shell.request_redraw();
            }
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
        let state = tree.state.downcast_ref::<State>();
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        let amount = state.entry.amount(state.now);
        if amount >= 1.0 {
            self.content
                .draw(child_tree, renderer, theme, style, child, cursor, viewport);
            return;
        }
        let bounds = child.bounds();
        let origin = Point::new(
            bounds.center_x(),
            match self.edge {
                Edge::Top => bounds.y,
                Edge::Bottom => bounds.y + bounds.height,
            },
        );
        let scale = motion::popup_scale(amount);
        let transformation = Transformation::translate(origin.x, origin.y)
            * Transformation::scale(scale)
            * Transformation::translate(-origin.x, -origin.y);
        renderer.with_transformation(transformation, |renderer| {
            self.content
                .draw(child_tree, renderer, theme, style, child, cursor, viewport);
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
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(child_tree, child, cursor, viewport, renderer)
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
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(child_tree, child, renderer, viewport, translation, window)
    }
}
