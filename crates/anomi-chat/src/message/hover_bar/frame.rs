//! The ground under a message, and the pointer over it. The Slint side: the
//! row's tint and its marigold tint (`ui/yapfast.slint:3804`-`:3824`) and
//! the `row-pointer` around it (`:3831`). The lit message lies on the row
//! hover's colour, fading in over `QUICK`; one that calls on this account
//! on marigold, a little stronger under the pointer, with a marigold edge.
//! Both reach up over the space above the message to the row's top, as the
//! Slint row's do, and down over the pixel under it. The pointer moving
//! on the message sends `on_enter`; it asks for frames only while the
//! tint fades.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::{Color, Event, Length, Point, Rectangle, Size, Vector, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Element, Motion, Renderer, Theme, View};

/// A message calling on this account: Discord's marigold at 12%, 16% under
/// the pointer (`Theme.mentioned`, `mentioned-hover`, `ui/theme.slint:63`).
const MENTIONED: Color = Color::from_rgba8(0xf8, 0xa5, 0x00, 0x1f as f32 / 255.0);
const MENTIONED_HOVER: Color = Color::from_rgba8(0xf8, 0xa5, 0x00, 0x29 as f32 / 255.0);
/// Its marigold edge (`ui/yapfast.slint:3823`).
const EDGE: f32 = 2.0;

/// How the ground under a message looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ground {
    /// The lit message: the row hover's colour.
    pub lit: bool,
    /// It calls on this account: marigold, with its edge.
    pub highlighted: bool,
    /// How far the ground reaches over the message's top edge, to the
    /// row's, and under its bottom edge.
    pub above: f32,
    pub below: f32,
}

pub struct Frame<'a, M> {
    content: Element<'a, M>,
    ground: Ground,
    on_enter: Option<M>,
    motion: Motion,
}

/// `content` on `ground`, sending `on_enter` as the pointer moves on it.
pub fn frame<'a, M>(
    content: impl View<M> + 'a,
    ground: Ground,
    on_enter: Option<M>,
    motion: Motion,
) -> Frame<'a, M> {
    Frame {
        content: content.boxed(),
        ground,
        on_enter,
        motion,
    }
}

struct State {
    hovered: bool,
    /// The lit tint's way in or out: 0 is clear, 1 lit.
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
    now: Instant,
}

impl State {
    fn amount(&self) -> f32 {
        let along = if self.duration.is_zero() {
            1.0
        } else {
            let elapsed = self.now.saturating_duration_since(self.start);
            (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0)
        };
        self.from + (self.to - self.from) * along
    }

    fn playing(&self) -> bool {
        (self.amount() - self.to).abs() > f32::EPSILON
    }
}

impl<M> widget::Meta for Frame<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Frame<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        let now = Instant::now();
        let lit = if self.ground.lit { 1.0 } else { 0.0 };
        tree::State::new(State {
            hovered: false,
            from: lit,
            to: lit,
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
        let bounds = layout.bounds();
        let (child, child_tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .update(child_tree, event, child, cursor, renderer, shell, viewport);
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let over = cursor.is_over(bounds);
                if over != state.hovered {
                    state.hovered = over;
                    if self.ground.highlighted {
                        shell.request_redraw();
                    }
                }
                // Any move on a message that isn't lit lights it: one that
                // went dark under a resting pointer (the pointer left the
                // timeline over a menu) lights again as it moves.
                if let Some(enter) = self.on_enter.as_ref().filter(|_| over) {
                    shell.publish(enter.clone());
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                let lit = if self.ground.lit { 1.0 } else { 0.0 };
                if lit != state.to {
                    state.from = state.amount();
                    state.to = lit;
                    state.start = *now;
                    state.duration = self.motion.duration(QUICK);
                }
                if self.motion.still() {
                    state.duration = Duration::ZERO;
                }
                if state.playing() {
                    shell.request_redraw();
                }
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
        let ground = Rectangle::new(
            Point::new(bounds.x, bounds.y - self.ground.above),
            Size::new(
                bounds.width,
                bounds.height + self.ground.above + self.ground.below,
            ),
        );
        let fill = |renderer: &mut Renderer, bounds: Rectangle, colour: Color| {
            renderer.fill_quad(
                Quad {
                    bounds,
                    ..Quad::default()
                },
                colour,
            );
        };
        let lit = state.amount();
        if lit > 0.0 {
            fill(renderer, ground, theme.palette().row_hover.scale_alpha(lit));
        }
        if self.ground.highlighted {
            let tint = if state.hovered {
                MENTIONED_HOVER
            } else {
                MENTIONED
            };
            fill(renderer, ground, tint);
            let edge = Rectangle {
                width: EDGE,
                ..ground
            };
            fill(renderer, edge, theme.palette().accent);
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
