//! A part of a menu that opens and closes in place, growing from nothing to
//! its height over `SLIDE` on `GLIDE` and clipping what is not yet shown: the
//! device list under its head (`list-height`, `ui/controls.slint:1618`).
//!
//! The animation lives in the widget's tree state and asks for frames only
//! while it plays. A window behind another, or animations turned off, land at
//! once ([`Motion::still`]); what is closed takes no events and no Tab.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Event, Length, Rectangle, Size, window};

use anomi_theme::motion::{GLIDE, SLIDE};
use anomi_theme::{Element, Motion, Renderer, Theme, View};

/// How far open it is: nothing under this draws, hears or focuses.
const CLOSED: f32 = 0.001;

pub struct Reveal<'a, M> {
    content: Element<'a, M>,
    open: bool,
    motion: Motion,
}

/// `content`, shown while `open`.
pub fn reveal<'a, M>(content: impl View<M> + 'a, open: bool, motion: Motion) -> Reveal<'a, M> {
    Reveal {
        content: content.boxed(),
        open,
        motion,
    }
}

struct State {
    slide: Animation<bool>,
    now: Instant,
}

impl State {
    fn amount(&self) -> f32 {
        self.slide.interpolate(0.0f32, 1.0, self.now)
    }
}

impl<M> widget::Meta for Reveal<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Reveal<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            slide: Animation::new(self.open).duration(SLIDE).easing(GLIDE),
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.content.size().width, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let amount = tree.state.downcast_ref::<State>().amount();
        self.content.layout(&mut tree.children[0], renderer, limits);
        let child = tree.children[0].size;
        tree.size = Size::new(child.width, child.height * amount);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if tree.state.downcast_ref::<State>().amount() < CLOSED {
            return;
        }
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        let viewport = layout.bounds().intersection(viewport).unwrap_or_default();
        self.content
            .operate(tree, layout, &viewport, renderer, operation);
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
        let state = tree.state.downcast_mut::<State>();
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            if self.motion.still() {
                state.slide = Animation::new(self.open).duration(SLIDE).easing(GLIDE);
            } else if state.slide.value() != self.open {
                state.slide = state.slide.clone().go(self.open, *now);
            }
            if state.slide.is_animating(*now) {
                shell.invalidate_layout();
                shell.request_redraw();
            }
        }
        // What isn't shown can't be pointed at.
        if state.amount() < CLOSED {
            return;
        }
        let cursor = if cursor.is_over(layout.bounds()) {
            cursor
        } else {
            mouse::Cursor::Unavailable
        };
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        let viewport = layout.bounds().intersection(viewport).unwrap_or_default();
        self.content
            .update(tree, event, layout, cursor, renderer, shell, &viewport);
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
        if tree.state.downcast_ref::<State>().amount() < CLOSED {
            return;
        }
        let bounds = layout.bounds();
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        let cursor = if cursor.is_over(bounds) {
            cursor
        } else {
            mouse::Cursor::Unavailable
        };
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        renderer.with_layer(clip, |renderer| {
            self.content
                .draw(tree, renderer, theme, style, layout, cursor, &clip);
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
        if tree.state.downcast_ref::<State>().amount() < CLOSED || !cursor.is_over(layout.bounds())
        {
            return mouse::Interaction::None;
        }
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }
}
