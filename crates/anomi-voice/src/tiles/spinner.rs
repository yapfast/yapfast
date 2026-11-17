//! An icon turning once a second, as `StreamWaiting`'s loader does
//! (`transform-rotation: animation-tick() / 1ms * 0.36deg`,
//! `ui/voice.slint:665`), while someone may be looking
//! (`Motion::watched`) and it shows; held upright otherwise, asking for no
//! frame.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::svg::{self as vector, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Color, Event, Length, Radians, Rectangle, Size, window};

use anomi_theme::{Motion, Renderer, Theme};
use anomi_widgets::icon::Icon;

use super::tick;

/// A turn a second.
const TURN: Duration = Duration::from_secs(1);

pub struct Spinner {
    icon: Icon,
    side: f32,
    color: Color,
    motion: Motion,
}

/// `icon`, `side` px square, in `color`, turning while `motion` is watched.
pub fn spinner(icon: Icon, side: f32, color: Color, motion: Motion) -> Spinner {
    Spinner {
        icon,
        side,
        color,
        motion,
    }
}

/// How far round it is after `elapsed`: upright unless someone may look.
pub(super) fn angle(motion: Motion, elapsed: Duration) -> Radians {
    if !motion.watched() {
        return Radians(0.0);
    }
    let turn = (elapsed.as_secs_f32() / TURN.as_secs_f32()).fract();
    Radians(turn * std::f32::consts::TAU)
}

/// The frame it's drawn at.
#[derive(Default)]
struct State {
    now: Option<Instant>,
}

impl widget::Meta for Spinner {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Spinner {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.side), Length::Fixed(self.side))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let side = Size::new(self.side, self.side);
        tree.size = limits.resolve(self.side, self.side, side);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            tree.state.downcast_mut::<State>().now = Some(*now);
            if self.motion.watched() && layout.bounds().intersects(viewport) {
                shell.request_redraw();
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let elapsed = tree
            .state
            .downcast_ref::<State>()
            .now
            .map(tick)
            .unwrap_or_default();
        let svg = vector::Svg {
            handle: self.icon.handle(),
            color: Some(self.color),
            rotation: angle(self.motion, elapsed),
            opacity: 1.0,
        };
        renderer.draw_svg(svg, layout.bounds(), *viewport);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_turns_only_while_watched() {
        let front = Motion {
            front: true,
            ..Motion::default()
        };
        let quarter = Duration::from_millis(250);
        assert!((angle(front, quarter).0 - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        assert_eq!(angle(Motion::default(), quarter), Radians(0.0));
        let idle = Motion {
            idle: true,
            ..front
        };
        assert_eq!(angle(idle, quarter), Radians(0.0));
    }
}
