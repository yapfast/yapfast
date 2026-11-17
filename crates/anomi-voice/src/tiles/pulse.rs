//! What beats while it waits: "Starting camera..." over this account's tile
//! fades from whole to a fifth and back once a second while the window
//! plays (`opacity: 0.6 + 0.4 * sin(animation-tick() / 1ms * 0.36deg)`,
//! `ui/voice.slint:1097`), and stays whole otherwise, asking for no frame.
//!
//! iced draws no layer at an opacity, so the pill is drawn whole and a veil
//! of the ground it lies on, the tile's raised ground, laid over it at what
//! it lacks: on one colour, the same pixels.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Border, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::{Element, Motion, Renderer, Theme, View};

use super::tick;

/// How strong it is: 60% and 40% either way, a beat a second.
const MIDDLE: f32 = 0.6;
const SWING: f32 = 0.4;
const BEAT: Duration = Duration::from_secs(1);

pub struct Pulse<'a, M> {
    content: Element<'a, M>,
    radius: f32,
    motion: Motion,
}

/// `content`, a shape of corners `radius`, beating while `motion` plays.
pub fn pulse<'a, M>(content: impl View<M> + 'a, radius: f32, motion: Motion) -> Pulse<'a, M> {
    Pulse {
        content: content.boxed(),
        radius,
        motion,
    }
}

/// How strong it is after `elapsed`: whole unless the window plays.
pub(super) fn strength(motion: Motion, elapsed: Duration) -> f32 {
    if !motion.playing() {
        return 1.0;
    }
    let beat = (elapsed.as_secs_f32() / BEAT.as_secs_f32()).fract();
    MIDDLE + SWING * (beat * std::f32::consts::TAU).sin()
}

/// The frame it's drawn at.
#[derive(Default)]
struct State {
    now: Option<Instant>,
}

impl<M> widget::Meta for Pulse<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Pulse<'_, M> {
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
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            tree.state.downcast_mut::<State>().now = Some(*now);
            if self.motion.playing() && layout.bounds().intersects(viewport) {
                shell.request_redraw();
            }
        }
        if let Some((child, tree)) = layout.iter_mut(&mut tree.children).next() {
            self.content
                .update(tree, event, child, cursor, renderer, shell, viewport);
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
        let elapsed = tree
            .state
            .downcast_ref::<State>()
            .now
            .map(tick)
            .unwrap_or_default();
        if let Some((child, tree)) = layout.iter(&tree.children).next() {
            self.content
                .draw(tree, renderer, theme, style, child, cursor, viewport);
        }
        let lack = 1.0 - strength(self.motion, elapsed);
        if lack <= 0.0 {
            return;
        }
        renderer.with_layer(bounds, |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds,
                    border: Border::default().rounded(self.radius),
                    ..Quad::default()
                },
                theme.palette().raised.scale_alpha(lack),
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
        layout
            .iter(&tree.children)
            .next()
            .map(|(child, tree)| {
                self.content
                    .mouse_interaction(tree, child, cursor, viewport, renderer)
            })
            .unwrap_or_default()
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if let Some((child, tree)) = layout.iter_mut(&mut tree.children).next() {
            self.content
                .operate(tree, child, viewport, renderer, operation);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: iced::Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        match layout.iter_mut(&mut tree.children).next() {
            Some((child, tree)) => {
                self.content
                    .overlay(tree, child, renderer, viewport, translation, window)
            }
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_beats_only_while_the_window_plays() {
        let front = Motion {
            front: true,
            ..Motion::default()
        };
        let quarter = Duration::from_millis(250);
        assert!((strength(front, quarter) - 1.0).abs() < 1e-4);
        assert!((strength(front, Duration::from_millis(750)) - 0.2).abs() < 1e-4);
        assert_eq!(strength(Motion::default(), quarter), 1.0);
        let never = Motion {
            never: true,
            ..front
        };
        assert_eq!(strength(never, quarter), 1.0);
    }
}
