//! A row's head as one button (`ui/moderation.slint:378`-`:417`): under the
//! pointer the overlay's 4 % fades in over `QUICK` (`animate background`,
//! the kit's [`Hot`]),
//! and the arrow at its end turns from right to down over `QUICK` on
//! `ARRIVE` as the row opens (`animate transform-rotation`). A row with
//! nothing to open neither lights nor presses and has no arrow. The arrow is
//! a path: the renderer at this pin draws no rotated svg
//! (`anomi_widgets::button::chevron`).

use std::f32::consts::FRAC_PI_2;
use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Event, Length, Point, Radians, Rectangle, Size, window};

use anomi_theme::Renderer;
use anomi_theme::metrics;
use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::prelude::*;
use anomi_vm::settings::audit::Intent;
use anomi_widgets::button::chevron;

use crate::kit::{Hot, fill};

/// The arrow's side: the content keeps this much room at its end for it.
pub const ARROW: f32 = 18.0;

struct Head<'a> {
    content: Element<'a, Intent>,
    /// What a click asks for; none, and the head neither lights nor turns.
    on_press: Option<Intent>,
    open: bool,
    /// The content's padding, between the arrow and the head's end.
    pad: f32,
    motion: Motion,
}

/// `content`, padded by `pad` and keeping [`ARROW`] at its end when
/// `on_press` is some, as a head that sends `on_press` and shows `open`.
pub fn head<'a>(
    content: Element<'a, Intent>,
    on_press: Option<Intent>,
    open: bool,
    pad: f32,
    motion: Motion,
) -> Element<'a, Intent> {
    Head {
        content,
        on_press,
        open,
        pad,
        motion,
    }
    .boxed()
}

/// The kit's hover fade, and the arrow's turn.
struct State {
    hot: Hot,
    turn: Animation<bool>,
}

impl State {
    /// Moves both to the frame at `now`; whether another frame is due.
    fn advance(&mut self, now: Instant, open: bool, motion: Motion) -> bool {
        let lighting = self.hot.advance(now, self.hot.hovered, motion);
        if motion.still() {
            self.turn = Animation::new(open);
            return false;
        }
        let quick = motion.duration(QUICK);
        if self.turn.value() != open {
            self.turn = self
                .turn
                .clone()
                .duration(quick)
                .easing(ARRIVE)
                .go(open, now);
        }
        lighting || self.turn.is_animating(now)
    }
}

impl widget::Meta for Head<'_> {}

impl widget::Widget<Intent, Theme, Renderer> for Head<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hot: Hot::new(),
            turn: Animation::new(self.open),
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

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, child) = layout.iter_mut(&mut tree.children).next().expect("child");
        self.content
            .operate(child, layout, viewport, renderer, operation);
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
        let (child_layout, child) = layout.iter_mut(&mut tree.children).next().expect("child");
        self.content.update(
            child,
            event,
            child_layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
        let Some(on_press) = &self.on_press else {
            return;
        };
        let over = cursor.is_over(layout.bounds());
        let state = tree.state.downcast_mut::<State>();
        let (open, motion) = (self.open, self.motion);
        let hot = &mut state.hot;
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                if over != hot.hovered {
                    hot.hovered = over;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if over && !shell.is_event_captured() =>
            {
                hot.pressed = true;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if hot.pressed => {
                hot.pressed = false;
                if over {
                    shell.publish(on_press.clone());
                }
                shell.capture_event();
            }
            Event::Window(window::Event::RedrawRequested(now))
                if state.advance(*now, open, motion) =>
            {
                shell.request_redraw();
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
        let palette = theme.palette();
        let lit = state.hot.amount();
        if lit > 0.0 {
            let ground = palette.lift(0.04 * lit);
            fill(renderer, bounds, metrics::RADIUS_CARD, ground);
        }
        let (child_layout, child) = layout.iter(&tree.children).next().expect("child");
        self.content.draw(
            child,
            renderer,
            theme,
            style,
            child_layout,
            cursor,
            viewport,
        );
        if self.on_press.is_some() {
            let turn = state.turn.interpolate(0.0f32, 1.0, state.hot.now);
            let at = Point::new(
                bounds.x + bounds.width - self.pad - ARROW,
                bounds.center_y() - ARROW / 2.0,
            );
            // Down turned a quarter back points right.
            let angle = Radians(-FRAC_PI_2 * (1.0 - turn));
            chevron(renderer, at, ARROW, angle, palette.muted);
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.on_press.is_some() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}
