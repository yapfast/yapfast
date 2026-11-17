//! The name's chevron: Lucide's `chevron-down` stroked as a path, because
//! the renderer at this pin draws nothing for an svg turned by a half turn
//! (`tiny_skia/src/vector.rs`). It turns over on `QUICK` with `ARRIVE` while
//! the menu opens, and lands at once while motion is still.

use std::f32::consts::PI;
use std::time::Instant;

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::widget::canvas::{Frame, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Event, Length, Point, Radians, Rectangle, Size, Vector, window};

use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::{Motion, Renderer, Theme};

/// The chevron's square, and the box its path is given in.
pub const SIDE: f32 = 16.0;
const BOX: f32 = 24.0;
const STROKE: f32 = 2.0;

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
        let mut frame = Frame::new(renderer, Size::new(SIDE, SIDE));
        frame.translate(Vector::new(SIDE / 2.0, SIDE / 2.0));
        frame.rotate(Radians(PI * turn));
        frame.scale(SIDE / BOX);
        frame.translate(Vector::new(-BOX / 2.0, -BOX / 2.0));
        // `m6 9 6 6 6-6`.
        let path = Path::new(|path| {
            path.move_to(Point::new(6.0, 9.0));
            path.line_to(Point::new(12.0, 15.0));
            path.line_to(Point::new(18.0, 9.0));
        });
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(ink)
                .with_width(STROKE * SIDE / BOX)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round),
        );
        let geometry = frame.into_geometry();
        let at = layout.position();
        renderer.with_translation(at - Point::ORIGIN, |renderer| {
            renderer.draw_geometry(geometry)
        });
    }
}

impl widget::Meta for Chevron {}
