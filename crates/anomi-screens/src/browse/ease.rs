//! The two things in a Browse Channels row that ease over `QUICK`, as Slint's
//! `BrowseItem` does (`ui/browse.slint:34` and `:88`): the row's raised fill
//! under the pointer ([`raised`]) and the box ([`tick_box`]).

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::svg::Renderer as _;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Renderer as _, Shell, mouse, svg};
use iced::animation::Animation;
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::{Element, Motion, Renderer, Theme, View};
use anomi_widgets::icon::Icon;

/// `content` over a raised fill that eases in under the pointer, never while
/// `locked`.
pub fn raised<'a, M>(
    content: impl View<M> + 'a,
    locked: bool,
    radius: f32,
    motion: Motion,
) -> Raised<'a, M> {
    Raised {
        content: content.boxed(),
        locked,
        radius,
        motion,
    }
}

pub struct Raised<'a, M> {
    content: Element<'a, M>,
    locked: bool,
    radius: f32,
    motion: Motion,
}

struct Hover {
    over: bool,
    now: Instant,
    glow: Animation<bool>,
}

impl<M> widget::Meta for Raised<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Raised<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Hover>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Hover {
            over: false,
            now: Instant::now(),
            glow: Animation::new(false).duration(QUICK).easing(ARRIVE),
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
        let state = tree.state.downcast_mut::<Hover>();
        let over = cursor.is_over(layout.bounds());
        if over != state.over {
            state.over = over;
            shell.request_redraw();
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            let target = state.over && !self.locked;
            if self.motion.still() {
                state.glow = Animation::new(target).duration(QUICK).easing(ARRIVE);
            } else if state.glow.value() != target {
                state.glow = state.glow.clone().go(target, *now);
            }
            if state.glow.is_animating(*now) {
                shell.request_redraw();
            }
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
        let state = tree.state.downcast_ref::<Hover>();
        let amount = state.glow.interpolate(0.0f32, 1.0, state.now);
        if amount > 0.0 {
            renderer.fill_quad(
                Quad {
                    bounds: layout.bounds(),
                    border: Border::default().rounded(self.radius),
                    ..Quad::default()
                },
                theme.palette().raised.scale_alpha(amount),
            );
        }
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
        window: Size,
    ) -> Vec<overlay::Element<'b, M, Theme, Renderer>> {
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}

/// The box: a ring, filled with the accent and ticked while `followed`, at
/// half strength while `locked`. Fill, ring and tick ease together.
pub fn tick_box(followed: bool, locked: bool, size: f32, tick: f32, motion: Motion) -> TickBox {
    TickBox {
        followed,
        locked,
        size,
        tick,
        motion,
    }
}

/// A locked box fades to this.
const LOCKED: f32 = 0.5;
const RING: f32 = 1.5;
const RADIUS: f32 = 4.0;

pub struct TickBox {
    followed: bool,
    locked: bool,
    size: f32,
    tick: f32,
    motion: Motion,
}

struct Face {
    now: Instant,
    followed: Animation<bool>,
    locked: Animation<bool>,
}

impl widget::Meta for TickBox {}

impl<M> widget::Widget<M, Theme, Renderer> for TickBox {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Face>()
    }

    fn state(&self) -> tree::State {
        let ease = |on| Animation::new(on).duration(QUICK).easing(ARRIVE);
        tree::State::new(Face {
            now: Instant::now(),
            followed: ease(self.followed),
            locked: ease(self.locked),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.size), Length::Fixed(self.size))
    }

    fn layout(&mut self, tree: &mut Tree, _: &Renderer, limits: &layout::Limits) {
        let side = Length::Fixed(self.size);
        tree.size = limits.resolve(side, side, Size::new(self.size, self.size));
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _: Layout,
        _: mouse::Cursor,
        _: &Renderer,
        shell: &mut Shell<'_, M>,
        _: &Rectangle,
    ) {
        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        let state = tree.state.downcast_mut::<Face>();
        state.now = *now;
        for (animation, target) in [
            (&mut state.followed, self.followed),
            (&mut state.locked, self.locked),
        ] {
            if self.motion.still() {
                *animation = Animation::new(target).duration(QUICK).easing(ARRIVE);
            } else if animation.value() != target {
                *animation = animation.clone().go(target, *now);
            }
            if animation.is_animating(*now) {
                shell.request_redraw();
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _: &renderer::Style,
        layout: Layout,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<Face>();
        let on = state.followed.interpolate(0.0f32, 1.0, state.now);
        let locked = state.locked.interpolate(0.0f32, 1.0, state.now);
        let fade = 1.0 - (1.0 - LOCKED) * locked;
        let palette = theme.palette();
        let bounds = layout.bounds();
        if on > 0.0 {
            // No ring over the fill: Slint fades the whole square at once.
            renderer.fill_quad(
                Quad {
                    bounds,
                    border: Border::default().rounded(RADIUS),
                    ..Quad::default()
                },
                palette.accent.scale_alpha(on * fade),
            );
        }
        if on < 1.0 {
            renderer.fill_quad(
                Quad {
                    bounds,
                    border: Border {
                        color: palette.muted.scale_alpha((1.0 - on) * fade),
                        width: RING,
                        radius: RADIUS.into(),
                    },
                    ..Quad::default()
                },
                Color::TRANSPARENT,
            );
        }
        if on > 0.0 {
            let side = self.tick;
            let at = Rectangle::new(
                bounds.center() - Vector::new(side / 2.0, side / 2.0),
                Size::new(side, side),
            );
            // tiny-skia keeps only the colour's rgb: fade through opacity.
            renderer.draw_svg(
                svg::Svg::new(Icon::Check.handle())
                    .color(palette.on_accent)
                    .opacity(on * fade),
                at,
                bounds,
            );
        }
    }
}
