//! The pill a row of badges sits in (`BadgeRows`, `ui/people.slint:1201`):
//! a grey wash with a faint 1px edge and 8px corners that brightens under
//! the pointer over `QUICK`. It draws the wash under its content and
//! forwards everything else to it; frames only while the wash changes.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Element, Motion, Renderer, Theme, View, metrics, rgba};

/// The wash at rest and under the pointer (`#97979f1f`, `#97979f33`) and the
/// edge (`#97979f0a`).
const REST: u32 = 0x97979f1f;
const LIT: u32 = 0x97979f33;
const EDGE: u32 = 0x97979f0a;

/// `from` towards `to` by `by`, in 0..=1.
fn blend(from: Color, to: Color, by: f32) -> Color {
    let step = |a: f32, b: f32| a + (b - a) * by;
    Color {
        r: step(from.r, to.r),
        g: step(from.g, to.g),
        b: step(from.b, to.b),
        a: step(from.a, to.a),
    }
}

pub struct Pill<'a, M> {
    content: Element<'a, M>,
    motion: Motion,
}

pub fn pill<'a, M: 'a>(content: impl View<M> + 'a, motion: Motion) -> Pill<'a, M> {
    Pill {
        content: content.boxed(),
        motion,
    }
}

struct State {
    hovered: bool,
    lit: Animation<bool>,
    now: Instant,
}

impl<M> widget::Meta for Pill<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Pill<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            lit: Animation::new(false),
            now: Instant::now(),
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
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let over = cursor.is_over(layout.bounds());
                if over != state.hovered {
                    state.hovered = over;
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                if self.motion.still() {
                    state.lit = Animation::new(state.hovered);
                    return;
                }
                if state.lit.value() != state.hovered {
                    state.lit = state.lit.clone().duration(QUICK).go(state.hovered, *now);
                }
                if state.lit.is_animating(*now) {
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
        let lit = state.lit.interpolate(0.0f32, 1.0, state.now);
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: Border {
                    color: rgba(EDGE),
                    width: 1.0,
                    radius: metrics::RADIUS_CARD.into(),
                },
                ..Quad::default()
            },
            blend(rgba(REST), rgba(LIT), lit),
        );
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .operate(tree, layout, viewport, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        _window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, _window)
    }
}
