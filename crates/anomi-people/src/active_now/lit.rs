//! A fill under its content that fades in under the pointer over `QUICK`
//! (`animate background { duration: Theme.quick; }`, `ui/active_now.slint:113`,
//! `:150`). A button's style can't animate, so the fill is a tree state: it
//! lands at once while the motion is still and asks for frames only while it
//! plays. It takes no clicks; wrap it in what does.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::border::Radius;
use iced::{Border, Event, Length, Rectangle, Size, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Element, Motion, Renderer, Theme, View};

struct State {
    over: bool,
    lit: Animation<bool>,
    now: Instant,
}

pub struct Lit<'a, M> {
    content: Element<'a, M>,
    radius: Radius,
    motion: Motion,
}

/// `content` over a fill of the hover colour, with these corners.
pub fn lit<'a, M: 'a>(
    content: impl View<M> + 'a,
    radius: impl Into<Radius>,
    motion: Motion,
) -> Lit<'a, M> {
    Lit {
        content: content.boxed(),
        radius: radius.into(),
        motion,
    }
}

impl<M> widget::Meta for Lit<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Lit<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            over: false,
            lit: Animation::new(false).duration(self.motion.duration(QUICK)),
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
        tree.children[0].translation = iced::Vector::ZERO;
        tree.size = tree.children[0].size;
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.traverse(&mut |operation| {
            let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
            self.content
                .operate(tree, layout, viewport, renderer, operation);
        });
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
        let over = cursor.is_over(layout.bounds());
        if over != state.over {
            state.over = over;
            shell.request_redraw();
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            if state.lit.value() != state.over {
                let duration = self.motion.duration(QUICK);
                state.lit = std::mem::replace(&mut state.lit, Animation::new(false))
                    .duration(duration)
                    .go(state.over, *now);
            }
            if state.lit.is_animating(*now) {
                shell.request_redraw();
            }
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
        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        if lit > 0.0 {
            let mut fill = theme.palette().hover;
            fill.a *= lit;
            renderer.fill_quad(
                Quad {
                    bounds: layout.bounds(),
                    border: Border::default().rounded(self.radius),
                    ..Quad::default()
                },
                fill,
            );
        }
        let (layout, child) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(child, renderer, theme, style, layout, cursor, viewport);
    }
}
