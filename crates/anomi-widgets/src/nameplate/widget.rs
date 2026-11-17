//! The plate as a widget around its row's content: the content laid out as
//! it is, the plate drawn under it, the pointer followed for the plate's
//! looks, and the picture played only while it's pointed at.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::{Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::{Motion, Renderer, Theme};

use crate::picture::Player;

use super::{Nameplate, extent};

#[derive(Default)]
pub(super) struct State {
    /// The pointer is on the row.
    pub hovered: bool,
    /// The picture played since the pointer came: it shows the still again
    /// once the pointer leaves.
    played: bool,
    pub player: Player,
    /// Where the row's text lies, from the plate's content: its left and
    /// its width.
    pub extent: Option<(f32, f32)>,
}

impl<Message> Nameplate<'_, Message> {
    /// Takes a frame's time: the picture plays while the plate shows, the
    /// pointer is on it and `motion` plays, and holds its still otherwise.
    /// Says when the next frame is due.
    pub(super) fn advance(
        &self,
        state: &mut State,
        renderer: &Renderer,
        bounds: Rectangle,
        viewport: &Rectangle,
        now: Instant,
    ) -> Option<Instant> {
        let plays = state.hovered && !self.held;
        if !plays && state.played {
            state.player = Player::default();
            state.played = false;
        }
        let plate = self.plate_bounds(bounds);
        if !self.shows(self.lit(state)) {
            return None;
        }
        let still = self.picture.as_ref()?;
        let at = self.picture_bounds(renderer, plate)?;
        let motion = if plays {
            self.motion
        } else {
            Motion::default()
        };
        state.played |= plays && motion.playing();
        state.player.update(still, at, viewport, motion, now)
    }
}

impl<Message> widget::Meta for Nameplate<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Nameplate<'_, Message> {
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
        let child = &mut tree.children[0];
        self.content.layout(child, renderer, limits);
        child.translation = Vector::ZERO;
        tree.size = child.size;
        let found = extent::find(&tree.children[0]);
        tree.state.downcast_mut::<State>().extent = found;
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
        let bounds = layout.bounds();
        {
            let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
            self.content
                .update(tree, event, layout, cursor, renderer, shell, viewport);
        }
        let state = tree.state.downcast_mut::<State>();
        match event {
            // A wheel moves the row under a still pointer too.
            Event::Mouse(_) => {
                let hovered = cursor.is_over(bounds);
                if hovered != state.hovered {
                    state.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if let Some(due) = self.advance(state, renderer, bounds, viewport, *now) {
                    shell.request_redraw_at(due);
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
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let (layout, child) = layout.iter(&tree.children).next().unwrap();
        let content = |renderer: &mut Renderer| {
            self.content
                .draw(child, renderer, theme, style, layout, cursor, viewport);
        };
        self.paint(
            state,
            renderer,
            theme.palette(),
            (bounds, viewport),
            content,
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
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
