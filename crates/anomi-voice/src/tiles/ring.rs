//! A tile's outline and corners, over what fills it. The Slint side: the
//! `Rectangle` drawn over the camera's picture (`ui/voice.slint:1091`), a
//! line of 1 px that becomes a ring of 3 px in the live colour while they
//! speak, its colour easing over `QUICK`; and `clip: true` with the
//! tile's `border-radius` (`:769`).
//!
//! A camera's frames are drawn square, as they come, and the corners are
//! rounded over them: a band of the stage's colour around the tile whose
//! inner edge is the tile's rounded outline. A tile always lies on the stage (the grid's
//! `Theme.stage`, `ui/yapfast.slint:3480`), and over the whole window it is
//! square.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Border, Color, Event, Length, Rectangle, Size, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Motion, Renderer, Theme};
use anomi_vm::voice::tiles::Outline;

/// The outline's width, and the ring's while they speak (`:1091`).
const LINE: f32 = 1.0;
const SPEAKING: f32 = 3.0;
/// How far the band that rounds the corners reaches past the tile: enough
/// to cover a corner of 12 px (12√2 is under 12 + 6), and no more than half
/// the 12 px between two tiles.
const BAND: f32 = 6.0;

pub struct Ring {
    outline: Outline,
    radius: f32,
    motion: Motion,
}

/// The outline `outline` asks for and corners of `radius`.
pub fn ring(outline: Outline, radius: f32, motion: Motion) -> Ring {
    Ring {
        outline,
        radius,
        motion,
    }
}

struct State {
    speaking: Animation<bool>,
    now: Instant,
}

/// `a` blended `amount` of the way towards `b`.
fn blend(a: Color, b: Color, amount: f32) -> Color {
    let mix = |x: f32, y: f32| x + (y - x) * amount;
    Color {
        r: mix(a.r, b.r),
        g: mix(a.g, b.g),
        b: mix(a.b, b.b),
        a: mix(a.a, b.a),
    }
}

impl widget::Meta for Ring {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Ring {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            speaking: Animation::new(self.outline == Outline::Speaking),
            now: Instant::now(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
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
        let speaking = self.outline == Outline::Speaking;
        if self.motion.still() {
            state.speaking = Animation::new(speaking);
            return;
        }
        if state.speaking.value() != speaking {
            state.speaking = state.speaking.clone().duration(QUICK).go(speaking, *now);
        }
        if state.speaking.is_animating(*now) {
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
        let palette = theme.palette();
        let bounds = layout.bounds();
        if self.radius > 0.0 {
            renderer.fill_quad(
                Quad {
                    bounds: bounds.expand(BAND),
                    border: Border {
                        color: palette.stage,
                        width: BAND,
                        radius: (self.radius + BAND).into(),
                    },
                    ..Quad::default()
                },
                Color::TRANSPARENT,
            );
        }
        if self.outline == Outline::None {
            return;
        }
        let state = tree.state.downcast_ref::<State>();
        let lit = state.speaking.interpolate(0.0f32, 1.0, state.now);
        let width = if self.outline == Outline::Speaking {
            SPEAKING
        } else {
            LINE
        };
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border {
                    color: blend(palette.line, palette.live, lit),
                    width,
                    radius: self.radius.into(),
                },
                ..Quad::default()
            },
            Color::TRANSPARENT,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_covers_a_corner_and_stays_in_the_gap() {
        let (corner, gap) = (super::super::RADIUS, 12.0_f32);
        assert!(corner * std::f32::consts::SQRT_2 < corner + BAND - 0.5);
        assert!(BAND <= gap / 2.0);
    }

    #[test]
    fn blending_goes_from_one_colour_to_the_other() {
        assert_eq!(blend(Color::BLACK, Color::WHITE, 0.0), Color::BLACK);
        assert_eq!(blend(Color::BLACK, Color::WHITE, 1.0), Color::WHITE);
    }
}
