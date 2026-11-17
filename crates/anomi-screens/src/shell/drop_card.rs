//! The card of files dragged over the window, as a widget of its own
//! (`ui/yapfast.slint:5492`-`:5528`): the card on its ground with its
//! shadow, a 2 px frame drawn 8 px in behind its lines, and the lines laid
//! 8 px in. It fills the layer and sits at the height's middle.
//!
//! Its left edge eases with an ease-out as the files move between the
//! thread beside and the rest, and its ground from the accent to the danger
//! colour, over `QUICK`, as Slint's `animate x` and `animate background`.
//! Both are tree state: frames are asked for only while one moves, and both
//! land at once while the motion is still.

use std::time::{Duration, Instant};

use iced::advanced::Shell;
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::{Border, Color, Event, Length, Rectangle, Shadow, Size, Vector, window};

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, motion};

/// The card's width and corners, its frame 8 px in with 6 px corners and a
/// 2 px line of the text colour at 60% (`:5501`-`:5507`).
pub const WIDTH: f32 = 310.0;
const RADIUS: f32 = 10.0;
const INSET: f32 = 8.0;
const FRAME_RADIUS: f32 = 6.0;
const FRAME_LINE: f32 = 2.0;
const FRAME_ALPHA: f32 = 0.6;
const SHADOW: Shadow = Shadow {
    color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
    offset: Vector::new(0.0, 8.0),
    blur_radius: 24.0,
};

pub struct DropCard<'a, Message> {
    lines: Element<'a, Message>,
    x: f32,
    refused: bool,
    motion: Motion,
}

/// The card with `lines` in it, its left edge at `x` in the layer, on the
/// danger colour if `refused`.
pub fn drop_card<'a, Message>(
    lines: impl View<Message> + 'a,
    x: f32,
    refused: bool,
    motion: Motion,
) -> DropCard<'a, Message> {
    DropCard {
        lines: lines.boxed(),
        x,
        refused,
        motion,
    }
}

struct State {
    x: Tween,
    /// From the accent's 0 to the danger colour's 1.
    refused: Tween,
    now: Option<Instant>,
}

impl<Message> DropCard<'_, Message> {
    fn danger(&self) -> f32 {
        if self.refused { 1.0 } else { 0.0 }
    }
}

impl<Message> widget::Meta for DropCard<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for DropCard<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            x: Tween::at(self.x, ease_out),
            refused: Tween::at(self.danger(), linear),
            now: None,
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.lines));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let area = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        let inner = WIDTH - 2.0 * INSET;
        let room = layout::Limits::new(Size::new(inner, 0.0), Size::new(inner, area.height));
        let lines = &mut tree.children[0];
        self.lines.layout(lines, renderer, &room);
        let height = lines.size.height + 2.0 * INSET;
        lines.translation = Vector::new(self.x + INSET, (area.height - height) / 2.0 + INSET);
        tree.size = area;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.traverse(&mut |operation| {
            if let Some((layout, lines)) = layout.iter_mut(&mut tree.children).next() {
                self.lines
                    .operate(lines, layout, viewport, renderer, operation);
            }
        });
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
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let (x, danger) = (self.x, self.danger());
            let quick = self.motion.duration(motion::QUICK);
            let state = tree.state.downcast_mut::<State>();
            state.now = Some(*now);
            state.x.go(x, quick, *now);
            state.refused.go(danger, quick, *now);
            if state.x.moving(*now) || state.refused.moving(*now) {
                shell.request_redraw();
            }
        }
        if let Some((layout, lines)) = layout.iter_mut(&mut tree.children).next() {
            self.lines
                .update(lines, event, layout, cursor, renderer, shell, viewport);
        }
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
            .map_or(mouse::Interaction::None, |(layout, lines)| {
                self.lines
                    .mouse_interaction(lines, layout, cursor, viewport, renderer)
            })
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
        let now = state.now.unwrap_or_else(Instant::now);
        let Some((lines_layout, lines)) = layout.iter(&tree.children).next() else {
            return;
        };
        let inner = lines_layout.bounds();
        let card = Rectangle {
            x: inner.x - INSET,
            y: inner.y - INSET,
            width: WIDTH,
            height: inner.height + 2.0 * INSET,
        };
        let palette = theme.palette();
        let ground = blend(palette.accent, palette.danger, state.refused.value(now));
        let frame = palette.on_accent.scale_alpha(FRAME_ALPHA);
        // Laid out where it's going, drawn where it is on its way.
        let shift = Vector::new(state.x.value(now) - self.x, 0.0);
        renderer.with_translation(shift, |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds: card,
                    border: Border::default().rounded(RADIUS),
                    shadow: SHADOW,
                    ..Quad::default()
                },
                ground,
            );
            renderer.fill_quad(
                Quad {
                    bounds: card.shrink(INSET),
                    border: Border::default()
                        .rounded(FRAME_RADIUS)
                        .width(FRAME_LINE)
                        .color(frame),
                    ..Quad::default()
                },
                Color::TRANSPARENT,
            );
            self.lines.draw(
                lines,
                renderer,
                theme,
                style,
                lines_layout,
                cursor - shift,
                viewport,
            );
        });
    }
}

/// `amount` of the way from `from` to `to`.
fn blend(from: Color, to: Color, amount: f32) -> Color {
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    Color {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
}

/// A number on its way to another over a duration, eased; a zero duration
/// lands at once.
#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f32,
    to: f32,
    start: Option<Instant>,
    duration: Duration,
    ease: fn(f32) -> f32,
}

impl Tween {
    fn at(value: f32, ease: fn(f32) -> f32) -> Self {
        Self {
            from: value,
            to: value,
            start: None,
            duration: Duration::ZERO,
            ease,
        }
    }

    /// Heads for `to` from where it is at `now`; a still motion's zero
    /// `duration` lands it there, even on its way.
    fn go(&mut self, to: f32, duration: Duration, now: Instant) {
        if duration.is_zero() {
            *self = Self::at(to, self.ease);
        } else if to != self.to {
            self.from = self.value(now);
            self.to = to;
            self.start = Some(now);
            self.duration = duration;
        }
    }

    fn value(&self, now: Instant) -> f32 {
        let Some(start) = self.start else {
            return self.to;
        };
        if self.duration.is_zero() {
            return self.to;
        }
        let done = (now.saturating_duration_since(start).as_secs_f32()
            / self.duration.as_secs_f32())
        .clamp(0.0, 1.0);
        self.from + (self.to - self.from) * (self.ease)(done)
    }

    fn moving(&self, now: Instant) -> bool {
        self.start
            .is_some_and(|start| !self.duration.is_zero() && now < start + self.duration)
    }
}

/// CSS `ease-out`, Slint's `easing: ease-out`.
fn ease_out(x: f32) -> f32 {
    motion::cubic_bezier(0.0, 0.0, 0.58, 1.0, x)
}

/// Slint's `animate` without an easing.
fn linear(x: f32) -> f32 {
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tween_eases_on_its_way_and_lands_at_once_when_still() {
        let start = Instant::now();
        let mut x = Tween::at(0.0, ease_out);
        x.go(100.0, motion::QUICK, start);
        let half = x.value(start + motion::QUICK / 2);
        assert!(
            half > 50.0 && half < 100.0,
            "ease-out is past half way: {half}"
        );
        assert!(x.moving(start));
        assert_eq!(x.value(start + motion::QUICK), 100.0);
        assert!(!x.moving(start + motion::QUICK));
        // Turned round half way, it starts from where it is.
        x.go(0.0, motion::QUICK, start + motion::QUICK / 2);
        assert_eq!(x.value(start + motion::QUICK / 2), half);
        // Still, it lands.
        x.go(40.0, Duration::ZERO, start + motion::QUICK / 2);
        assert_eq!(x.value(start), 40.0);
        assert!(!x.moving(start));
    }
}
