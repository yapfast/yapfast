//! A bar that shakes red when leaving was refused for unsaved changes, as
//! Discord's does: each time `nudge` goes up, its content sways six beats of
//! 60 ms, 6 px each way, and its ground turns the danger colour for 1.2 s,
//! coming and going over `QUICK`. With a still interface only the red says
//! it. It asks for frames only while it shakes. The Slint side: the
//! `nudge` of `UnsavedBar` (`ui/settings.slint:311`), which the settings'
//! bar and the profile editor's draw with.
//!
//! The ground is the raised colour under a 1 px line on its top, with the
//! corners and the shadow the owner gives it.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Style};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Renderer as _, Shell, mouse};
use iced::{Border, Color, Event, Length, Rectangle, Shadow, Size, Vector, window};

use anomi_theme::Renderer;
use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;

/// Six shakes 60 ms apart, 6 px each way, and the red for 1.2 s.
const SHAKES: u128 = 6;
const BEAT: Duration = Duration::from_millis(60);
const RED: Duration = Duration::from_millis(1200);
const SWAY: f32 = 6.0;

/// The bar around `content`, which shakes as `nudge` goes up.
pub struct Shake<'a, M> {
    content: Element<'a, M>,
    nudge: u32,
    motion: Motion,
    height: f32,
    radius: f32,
    shadow: Shadow,
}

/// `content` on a bar 60 px tall, square, with no shadow.
pub fn shake<M>(content: Element<'_, M>, nudge: u32, motion: Motion) -> Shake<'_, M> {
    Shake {
        content,
        nudge,
        motion,
        height: 60.0,
        radius: 0.0,
        shadow: Shadow::default(),
    }
}

impl<M> Shake<'_, M> {
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Its corners; the line on its top stops where they begin.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = shadow;
        self
    }
}

struct State {
    /// The last nudge seen, and when the shake it started began.
    seen: u32,
    since: Option<Instant>,
    now: Instant,
}

impl State {
    fn elapsed(&self) -> Option<Duration> {
        self.since
            .map(|since| self.now.saturating_duration_since(since))
    }

    /// How red the bar is: it comes in and goes out over `QUICK`.
    fn red(&self, still: bool) -> f32 {
        let Some(elapsed) = self.elapsed().filter(|elapsed| *elapsed < RED) else {
            return 0.0;
        };
        if still {
            return 1.0;
        }
        let (rise, fall) = (
            elapsed.as_secs_f32() / QUICK.as_secs_f32(),
            (RED - elapsed).as_secs_f32() / QUICK.as_secs_f32(),
        );
        rise.min(fall).clamp(0.0, 1.0)
    }

    /// How far it is moved aside: a shake is over after six beats.
    fn sway(&self) -> f32 {
        match self.elapsed() {
            Some(elapsed) if elapsed.as_millis() / BEAT.as_millis() < SHAKES => {
                let left = SHAKES - elapsed.as_millis() / BEAT.as_millis();
                if left.is_multiple_of(2) { -SWAY } else { SWAY }
            }
            _ => 0.0,
        }
    }
}

impl<M> widget::Meta for Shake<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Shake<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            seen: self.nudge,
            since: None,
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);
        tree.size = Size::new(limits.max.width, self.height);
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
        shell: &mut Shell<'_, M>,
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
        let state = tree.state.downcast_mut::<State>();
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            if state.seen != self.nudge {
                state.seen = self.nudge;
                state.since = Some(*now);
            }
            if let Some(since) = state
                .since
                .filter(|_| state.elapsed().is_some_and(|e| e < RED))
            {
                if self.motion.still() {
                    // A step: one frame at the end takes the red off.
                    shell.request_redraw_at(since + RED);
                } else {
                    shell.request_redraw();
                }
            } else {
                state.since = None;
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let palette = theme.palette();
        let red = state.red(self.motion.still());
        let bounds = layout.bounds();
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default().rounded(self.radius),
                shadow: self.shadow,
                ..Quad::default()
            },
            mix(palette.danger, palette.raised, red),
        );
        renderer.fill_quad(
            Quad {
                bounds: Rectangle {
                    x: bounds.x + self.radius,
                    width: (bounds.width - 2.0 * self.radius).max(0.0),
                    height: 1.0,
                    ..bounds
                },
                ..Quad::default()
            },
            palette.edge,
        );
        let sway = if self.motion.still() {
            0.0
        } else {
            state.sway()
        };
        let ink = renderer::Style {
            text_color: mix(palette.on_danger, palette.text, red),
        };
        let (child_layout, child) = layout.iter(&tree.children).next().expect("child");
        renderer.with_translation(Vector::new(sway, 0.0), |renderer| {
            self.content
                .draw(child, renderer, theme, &ink, child_layout, cursor, viewport);
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, child) = layout.iter(&tree.children).next().expect("child");
        self.content
            .mouse_interaction(child, layout, cursor, viewport, renderer)
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
        let (layout, child) = layout.iter_mut(&mut tree.children).next().expect("child");
        self.content
            .overlay(child, layout, renderer, viewport, translation, window)
    }
}

/// `from` at 0 and `to` at 1.
fn mix(to: Color, from: Color, amount: f32) -> Color {
    let blend = |x: f32, y: f32| x + (y - x) * amount.clamp(0.0, 1.0);
    Color {
        r: blend(from.r, to.r),
        g: blend(from.g, to.g),
        b: blend(from.b, to.b),
        a: blend(from.a, to.a),
    }
}
