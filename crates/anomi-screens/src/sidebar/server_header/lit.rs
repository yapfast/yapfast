//! A box that lights under the pointer, or while what it opens is open: the
//! fill `ServerHeader` has (`ui/rail.slint:952`, `Theme.hover` easing over
//! `QUICK`). It draws the fill and hands everything else to its content, so
//! a press reaches whatever the content does with one (an `opener`).

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::keyboard::{self, key::Named};
use iced::{Border, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::{Element, Motion, Renderer, Theme, View, metrics};

pub struct Lit<'a, M> {
    content: Element<'a, M>,
    open: bool,
    motion: Motion,
    keys: Option<Box<dyn Fn(Rectangle) -> M + 'a>>,
}

/// `content` over a fill that shows under the pointer and while `open`.
pub fn lit<'a, M>(content: impl View<M> + 'a, open: bool, motion: Motion) -> Lit<'a, M> {
    Lit {
        content: content.boxed(),
        open,
        motion,
        keys: None,
    }
}

impl<'a, M> Lit<'a, M> {
    /// Takes the keyboard focus, shows a ring while it has it, and asks
    /// for `message` (given its bounds in the window) on Enter, Space, the
    /// Menu key and Shift+F10, as `ServerHeader` does (`ui/rail.slint:962`).
    pub fn keys(mut self, message: impl Fn(Rectangle) -> M + 'a) -> Self {
        self.keys = Some(Box::new(message));
        self
    }
}

struct State {
    hovered: bool,
    focused: bool,
    translation: Vector,
    now: Instant,
    glow: Animation<bool>,
}

impl State {
    fn amount(&self) -> f32 {
        self.glow.interpolate(0.0f32, 1.0, self.now)
    }
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl<M> widget::Meta for Lit<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Lit<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            focused: false,
            translation: Vector::ZERO,
            now: Instant::now(),
            glow: Animation::new(self.open).duration(QUICK).easing(ARRIVE),
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
        let over = cursor.is_over(layout.bounds());
        if self.keys.is_some() {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                    if state.focused != over =>
                {
                    state.focused = over;
                    shell.request_redraw();
                }
                Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                    if state.focused && !shell.is_event_captured() =>
                {
                    let named = match key.as_ref() {
                        keyboard::Key::Named(named) => Some(named),
                        _ => None,
                    };
                    let asks = matches!(
                        named,
                        Some(Named::Enter | Named::Space | Named::ContextMenu)
                    ) || (named == Some(Named::F10) && modifiers.shift());
                    if let (true, Some(keys)) = (asks, &self.keys) {
                        let bounds = layout.bounds();
                        let shown =
                            Rectangle::new(bounds.position() + state.translation, bounds.size());
                        shell.publish(keys(shown));
                        shell.capture_event();
                    }
                }
                _ => {}
            }
        }
        if over != state.hovered {
            state.hovered = over;
            shell.request_redraw();
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            let target = state.hovered || self.open;
            if self.motion.still() {
                state.glow = Animation::new(target).duration(QUICK).easing(ARRIVE);
            } else if state.glow.value() != target {
                state.glow = state
                    .glow
                    .clone()
                    .duration(QUICK)
                    .easing(ARRIVE)
                    .go(target, *now);
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
        use iced::advanced::Renderer as _;
        let state = tree.state.downcast_ref::<State>();
        let amount = state.amount();
        if amount > 0.0 {
            renderer.fill_quad(
                Quad {
                    bounds: layout.bounds(),
                    border: Border::default().rounded(metrics::RADIUS_ROW),
                    ..Quad::default()
                },
                theme.palette().hover.scale_alpha(amount),
            );
        }
        if state.focused && !self.open {
            renderer.fill_quad(
                Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: theme.palette().accent,
                        width: 2.0,
                        radius: metrics::RADIUS_ROW.into(),
                    },
                    ..Quad::default()
                },
                iced::Color::TRANSPARENT,
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
        if self.keys.is_some() {
            operation.focusable(None, layout.bounds(), tree.state.downcast_mut::<State>());
        }
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
        tree.state.downcast_mut::<State>().translation = translation;
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}
