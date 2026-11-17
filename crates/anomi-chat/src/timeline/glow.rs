//! The ground under a message: lit while the keyboard is on it, with a
//! marigold edge at its left, and the accent at 14% for a moment after a
//! jump lands on it. The Slint side: the row's tint `Rectangle` and the
//! keyboard's edge (`ui/yapfast.slint:3804`-`:3809`, `:3826`): the tint
//! fades in over `LINGER` for a jump, over `QUICK` otherwise, and at once
//! while the keyboard steps, so stepping doesn't trail. A message under the
//! pointer, mentioning the person or edited in place is tinted by the
//! message itself (C02, C13).
//!
//! The fade is the widget's own: it asks for frames only while it plays,
//! none while the interface holds still, where it lands at once.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::{Color, Event, Length, Point, Rectangle, Size, Vector, window};

use anomi_theme::motion::{LINGER, QUICK};
use anomi_theme::{Element, Motion, Renderer, Theme, View};

/// How lit the jumped-to message is: the accent made 86% clear
/// (`Theme.accent.transparentize(0.86)`, `:3806`).
const JUMPED: f32 = 0.14;
/// The keyboard's edge (`:3826`).
const EDGE: f32 = 3.0;

/// What lies under a message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tint {
    #[default]
    None,
    /// The keyboard is on it: the row hover's colour.
    Lit,
    /// A jump landed on it.
    Jumped,
}

impl Tint {
    fn colour(self, theme: &Theme) -> Color {
        let palette = theme.palette();
        match self {
            Self::None => Color::TRANSPARENT,
            Self::Lit => palette.row_hover,
            Self::Jumped => palette.accent.scale_alpha(JUMPED),
        }
    }
}

/// The ground a message lies on now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ground {
    pub tint: Tint,
    /// The marigold edge of the keyboard's message.
    pub edge: bool,
    /// The keyboard steps through the messages: tints change at once.
    pub stepping: bool,
}

pub struct Glow<'a, M> {
    content: Element<'a, M>,
    ground: Ground,
    motion: Motion,
}

/// `content` over `ground`.
pub fn glow<'a, M>(content: impl View<M> + 'a, ground: Ground, motion: Motion) -> Glow<'a, M> {
    Glow {
        content: content.boxed(),
        ground,
        motion,
    }
}

/// The tint's way from one to another.
struct State {
    from: Tint,
    to: Tint,
    start: Instant,
    duration: Duration,
    now: Instant,
}

impl State {
    /// How far it is from `from` to `to`, from 0 to 1.
    fn amount(&self) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = self.now.saturating_duration_since(self.start);
        (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    fn playing(&self) -> bool {
        self.amount() < 1.0
    }
}

/// `from` blended towards `to` by `amount`, as colours with their alpha
/// already applied, so a fade from clear doesn't pass through grey.
fn blend(from: Color, to: Color, amount: f32) -> Color {
    let alpha = from.a + (to.a - from.a) * amount;
    if alpha <= 0.0 {
        return Color::TRANSPARENT;
    }
    let channel = |a: f32, b: f32| (a * from.a + (b * to.a - a * from.a) * amount) / alpha;
    Color {
        r: channel(from.r, to.r),
        g: channel(from.g, to.g),
        b: channel(from.b, to.b),
        a: alpha,
    }
}

impl<M> widget::Meta for Glow<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Glow<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        let now = Instant::now();
        tree::State::new(State {
            from: self.ground.tint,
            to: self.ground.tint,
            start: now,
            duration: Duration::ZERO,
            now,
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
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        state.now = *now;
        let tint = self.ground.tint;
        if tint != state.to {
            // A jump's light comes slowly; the keyboard's steps don't fade.
            let token = if tint == Tint::Jumped {
                LINGER
            } else if self.ground.stepping {
                Duration::ZERO
            } else {
                QUICK
            };
            // Turned round half way, it goes on from the nearer end.
            state.from = if state.amount() < 0.5 {
                state.from
            } else {
                state.to
            };
            state.to = tint;
            state.start = *now;
            state.duration = self.motion.duration(token);
        }
        if self.motion.still() {
            state.duration = Duration::ZERO;
        }
        if state.playing() {
            shell.request_redraw();
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
        let colour = blend(
            state.from.colour(theme),
            state.to.colour(theme),
            state.amount(),
        );
        if colour.a > 0.0 {
            renderer.fill_quad(
                Quad {
                    bounds,
                    ..Quad::default()
                },
                colour,
            );
        }
        if self.ground.edge {
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle::new(
                        Point::new(bounds.x, bounds.y),
                        Size::new(EDGE, bounds.height),
                    ),
                    ..Quad::default()
                },
                theme.palette().accent,
            );
        }
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(child_tree, renderer, theme, style, child, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (child, child_tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(child_tree, child, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(child_tree, child, viewport, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(child_tree, child, renderer, viewport, translation, window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fade_from_clear_keeps_its_colour() {
        let accent = Color::from_rgb(0.2, 0.4, 0.9);
        let half = blend(Color::TRANSPARENT, accent, 0.5);
        assert!((half.a - 0.5).abs() < 1e-6);
        assert!((half.r - accent.r).abs() < 1e-6 && (half.b - accent.b).abs() < 1e-6);
        assert_eq!(blend(accent, Color::TRANSPARENT, 1.0), Color::TRANSPARENT);
    }
}
