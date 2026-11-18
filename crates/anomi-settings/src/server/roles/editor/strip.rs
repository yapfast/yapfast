//! The tabs' row with the accent's line under the chosen one, which slides
//! to the next one chosen, its width with it, over `SLIDE` on `GLIDE`
//! (`animate x, width`, `ui/roles.slint:470`). It asks for frames only
//! while the line moves, and a still [`Motion`] puts it there at once.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{Quad, Renderer as _, Style};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::Renderer;
use anomi_theme::motion::{GLIDE, SLIDE};
use anomi_theme::prelude::*;

/// The line's thickness, at the row's foot.
const LINE: f32 = 2.0;

/// `tabs`, a row of them, with the line under the one at `chosen`.
pub fn strip<'a, M: 'a>(tabs: Element<'a, M>, chosen: usize, motion: Motion) -> Element<'a, M> {
    Strip {
        tabs,
        chosen,
        motion,
    }
    .boxed()
}

struct Strip<'a, M> {
    tabs: Element<'a, M>,
    chosen: usize,
    motion: Motion,
}

/// Where the line is, as its left from the row's and its width.
type Spot = (f32, f32);

struct State {
    /// Where it is going, once a frame saw it.
    to: Option<Spot>,
    /// Where it set out from.
    from: Spot,
    slide: Animation<bool>,
    now: Instant,
}

impl State {
    /// Where the line is at the last frame, on its way to `to`.
    fn at(&self, to: Spot) -> Spot {
        (
            self.slide.interpolate(self.from.0, to.0, self.now),
            self.slide.interpolate(self.from.1, to.1, self.now),
        )
    }
}

/// The chosen tab's place in the row, `children` being the strip's.
fn target(layout: Layout, children: &[Tree], chosen: usize) -> Spot {
    let left = layout.bounds().x;
    layout
        .iter(children)
        .next()
        .and_then(|(row, tree)| row.iter(&tree.children).nth(chosen))
        .map_or((0.0, 0.0), |(tab, _)| {
            let tab = tab.bounds();
            (tab.x - left, tab.width)
        })
}

impl<M> widget::Meta for Strip<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Strip<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            to: None,
            from: (0.0, 0.0),
            slide: Animation::new(true),
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.tabs));
    }

    fn size(&self) -> Size<Length> {
        self.tabs.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.tabs.layout(&mut tree.children[0], renderer, limits);
        tree.size = tree.children[0].size;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, child) = layout.iter_mut(&mut tree.children).next().expect("tabs");
        self.tabs
            .operate(child, layout, viewport, renderer, operation);
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
        let (child_layout, child) = layout.iter_mut(&mut tree.children).next().expect("tabs");
        self.tabs.update(
            child,
            event,
            child_layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let target = target(layout, &tree.children, self.chosen);
            let state = tree.state.downcast_mut::<State>();
            state.now = *now;
            if let Some(to) = state.to
                && to != target
            {
                state.from = state.at(to);
                state.slide = self.motion.animation(false, SLIDE, GLIDE).go(true, *now);
            }
            state.to = Some(target);
            if state.slide.is_animating(*now) {
                shell.request_redraw();
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let (child_layout, child) = layout.iter(&tree.children).next().expect("tabs");
        self.tabs.draw(
            child,
            renderer,
            theme,
            style,
            child_layout,
            cursor,
            viewport,
        );
        let state = tree.state.downcast_ref::<State>();
        // Until a frame sees a new choice the line stays where it was.
        let (x, width) = state.to.map_or_else(
            || target(layout, &tree.children, self.chosen),
            |to| state.at(to),
        );
        let bounds = layout.bounds();
        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: bounds.x + x,
                    y: bounds.y + bounds.height - LINE,
                    width,
                    height: LINE,
                },
                ..Quad::default()
            },
            theme.palette().accent,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, child) = layout.iter(&tree.children).next().expect("tabs");
        self.tabs
            .mouse_interaction(child, layout, cursor, viewport, renderer)
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().expect("tabs");
        self.tabs
            .overlay(child, layout, renderer, viewport, translation, window)
    }
}
