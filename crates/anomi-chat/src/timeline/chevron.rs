//! The unread bar's chevron, which points to where the "New" divider is and
//! turns half round over `QUICK` as it goes from below the view to above
//! (`transform-rotation`, `ui/messages.slint:2488`-`:2489`). Drawn as a
//! path: the renderer at this pin draws no rotated svg (see
//! `anomi_widgets::button`'s caret, which strokes the same `chevron-down`).

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

/// Lucide's box, in which `chevron-down`'s path and stroke are given
/// (`ui/icons/chevron-down.svg`).
const BOX: f32 = 24.0;
const STROKE: f32 = 2.0;

pub struct Chevron {
    side: f32,
    up: bool,
    ink: fn(&Theme) -> Color,
    motion: Motion,
}

/// A chevron `side` square in `ink`, pointing up when `up`.
pub fn chevron(side: f32, up: bool, ink: fn(&Theme) -> Color, motion: Motion) -> Chevron {
    Chevron {
        side,
        up,
        ink,
        motion,
    }
}

struct State {
    turn: Animation<bool>,
    now: Instant,
}

impl widget::Meta for Chevron {}

impl<M> widget::Widget<M, Theme, Renderer> for Chevron {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            turn: Animation::new(self.up).easing(ARRIVE),
            now: Instant::now(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.side), Length::Fixed(self.side))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let side = Size::new(self.side, self.side);
        tree.size = limits.resolve(side.width, side.height, side);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        state.now = *now;
        if self.motion.still() {
            state.turn = Animation::new(self.up).easing(ARRIVE);
            return;
        }
        if state.turn.value() != self.up {
            state.turn = state
                .turn
                .clone()
                .duration(self.motion.duration(QUICK))
                .go(self.up, *now);
        }
        if state.turn.is_animating(*now) {
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let turn = state.turn.interpolate(0.0, 1.0, state.now);
        let side = self.side;
        let mut frame = Frame::new(renderer, Size::new(side, side));
        frame.translate(Vector::new(side / 2.0, side / 2.0));
        frame.rotate(Radians(PI * turn));
        frame.scale(side / BOX);
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
                .with_color((self.ink)(theme))
                .with_width(STROKE * side / BOX)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round),
        );
        let geometry = frame.into_geometry();
        let at = layout.bounds().position();
        renderer.with_translation(at - Point::ORIGIN, |renderer| {
            renderer.draw_geometry(geometry);
        });
    }
}
