//! The name's chevron: the foundation's Lucide `chevron-down` stroked as a
//! path (`anomi_widgets::button::chevron`), because the renderer at this pin
//! draws nothing for an svg turned by a half turn (`tiny_skia/src/vector.rs`). It turns over on `QUICK` with `ARRIVE` while
//! the menu opens, and lands at once while motion is still.

use std::f32::consts::PI;
use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Color, Event, Length, Radians, Rectangle, Size, window};

use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::{Motion, Renderer, Theme};

/// The chevron's square.
pub const SIDE: f32 = 16.0;

pub struct Chevron {
    open: bool,
    white: bool,
    motion: Motion,
}

/// A chevron pointing down, up while `open`; white over a banner.
pub fn chevron(open: bool, white: bool, motion: Motion) -> Chevron {
    Chevron {
        open,
        white,
        motion,
    }
}

struct State {
    now: Instant,
    turn: Animation<bool>,
}

impl<M> widget::Widget<M, Theme, Renderer> for Chevron {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            now: Instant::now(),
            turn: Animation::new(self.open).duration(QUICK).easing(ARRIVE),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(SIDE), Length::Fixed(SIDE))
    }

    fn layout(&mut self, tree: &mut Tree, _: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(
            Length::Fixed(SIDE),
            Length::Fixed(SIDE),
            Size::new(SIDE, SIDE),
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _: Layout,
        _: mouse::Cursor,
        _: &Renderer,
        shell: &mut Shell<'_, M>,
        _: &Rectangle,
    ) {
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let state = tree.state.downcast_mut::<State>();
            state.now = *now;
            if self.motion.still() {
                state.turn = Animation::new(self.open).duration(QUICK).easing(ARRIVE);
            } else if state.turn.value() != self.open {
                state.turn = state
                    .turn
                    .clone()
                    .duration(QUICK)
                    .easing(ARRIVE)
                    .go(self.open, *now);
            }
            if state.turn.is_animating(*now) {
                shell.request_redraw();
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _: &renderer::Style,
        layout: Layout,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let turn = state.turn.interpolate(0.0f32, 1.0, state.now);
        let ink = if self.white {
            Color::WHITE
        } else {
            theme.palette().secondary
        };
        let angle = Radians(PI * turn);
        anomi_widgets::button::chevron(renderer, layout.position(), SIDE, angle, ink);
    }
}

impl widget::Meta for Chevron {}
