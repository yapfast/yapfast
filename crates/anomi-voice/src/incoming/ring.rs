//! The ring around the caller's avatar: 92 px across, a 3 px line in the
//! live colour that pulses while the call rings (`ui/voice.slint:1243`).
//! The opacity follows `0.45 + 0.4 * sin(t * 0.36deg per ms)`, a breath a
//! second between 0.05 and 0.85; a still window holds it at 0.85. The widget
//! asks for frames only while `motion` plays, so a window behind another
//! draws nothing and wakes nothing.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Background, Border, Color, Event, Length, Rectangle, Size, window};

use anomi_theme::{Motion, Renderer, Theme};

/// The ring's outer size and its line (`:1241`, `:1244`).
pub const SIZE: f32 = 92.0;
const LINE: f32 = 3.0;
/// One breath, and the opacity's middle, swing and rest (`:1245`).
const PERIOD: Duration = Duration::from_millis(1000);
const MIDDLE: f32 = 0.45;
const SWING: f32 = 0.4;
const REST: f32 = 0.85;

/// A ring, pulsing while `motion` plays.
pub fn ring(motion: Motion) -> Ring {
    Ring { motion }
}

pub struct Ring {
    motion: Motion,
}

/// When it began pulsing, and the last frame drawn.
struct State {
    began: Option<Instant>,
    now: Instant,
}

/// The ring's opacity `at` into its breath, or its rest while still.
pub fn opacity(playing: bool, at: Duration) -> f32 {
    if !playing {
        return REST;
    }
    let turn = at.as_secs_f32() / PERIOD.as_secs_f32();
    MIDDLE + SWING * (turn * std::f32::consts::TAU).sin()
}

impl widget::Meta for Ring {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Ring {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            began: None,
            now: Instant::now(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(SIZE), Length::Fixed(SIZE))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(
            Length::Fixed(SIZE),
            Length::Fixed(SIZE),
            Size::new(SIZE, SIZE),
        );
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        state.now = *now;
        if self.motion.playing() {
            state.began.get_or_insert(*now);
            shell.request_redraw();
        } else {
            state.began = None;
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
        use iced::advanced::Renderer as _;
        let state = tree.state.downcast_ref::<State>();
        let at = state.began.map_or(Duration::ZERO, |began| {
            state.now.saturating_duration_since(began)
        });
        let alpha = opacity(self.motion.playing(), at);
        let live = theme.palette().live;
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: Border {
                    color: Color {
                        a: live.a * alpha,
                        ..live
                    },
                    width: LINE,
                    radius: (SIZE / 2.0).into(),
                },
                ..Quad::default()
            },
            Background::Color(Color::TRANSPARENT),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_rests_when_still_and_breathes_when_playing() {
        assert_eq!(opacity(false, Duration::from_millis(250)), REST);
        assert!((opacity(true, Duration::ZERO) - MIDDLE).abs() < 1e-5);
        assert!((opacity(true, Duration::from_millis(250)) - 0.85).abs() < 1e-4);
        assert!((opacity(true, Duration::from_millis(750)) - 0.05).abs() < 1e-4);
    }
}
