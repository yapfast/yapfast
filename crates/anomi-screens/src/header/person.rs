//! The person's button in a DM's title (`dm-title`, `ui/yapfast.slint:3161`):
//! the overlay at 10 % easing in under the pointer over `QUICK`, a 2 px
//! accent ring while it has the keyboard focus, and Enter or Space asking
//! for what a click asks for. It draws the fill and the ring and hands
//! everything else to its content, so a click reaches the content's
//! `opener`. Task N07's (docs/iced/tasks/shell.md).

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::operation::Focusable;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::keyboard::{self, key::Named};
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::{ARRIVE, QUICK};
use anomi_theme::{Element, Motion, Renderer, Theme, View, metrics};

/// The hover's overlay (`transparent.mix(Theme.overlay, 0.9)`).
const LIFT: f32 = 0.1;
const RING: f32 = 2.0;

pub(super) struct Person<'a, M> {
    content: Element<'a, M>,
    motion: Motion,
    keys: Box<dyn Fn(Rectangle) -> M + 'a>,
}

/// `content` that lights under the pointer, takes the focus and asks for
/// `keys(bounds)` on Enter or Space.
pub(super) fn person<'a, M>(
    content: impl View<M> + 'a,
    motion: Motion,
    keys: impl Fn(Rectangle) -> M + 'a,
) -> Person<'a, M> {
    Person {
        content: content.boxed(),
        motion,
        keys: Box::new(keys),
    }
}

struct State {
    hovered: bool,
    focused: bool,
    translation: Vector,
    now: Instant,
    glow: Animation<bool>,
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

impl<M> widget::Meta for Person<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Person<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            focused: false,
            translation: Vector::ZERO,
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
        let state = tree.state.downcast_mut::<State>();
        let over = cursor.is_over(layout.bounds());
        match event {
            // A click elsewhere takes the focus away; a click on it does not
            // give it, as Slint's touch area does not.
            Event::Mouse(mouse::Event::ButtonPressed(_)) if state.focused && !over => {
                state.focused = false;
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(Named::Enter | Named::Space),
                ..
            }) if state.focused && !shell.is_event_captured() => {
                let bounds = layout.bounds();
                let shown = Rectangle::new(bounds.position() + state.translation, bounds.size());
                shell.publish((self.keys)(shown));
                shell.capture_event();
            }
            _ => {}
        }
        if over != state.hovered {
            state.hovered = over;
            shell.request_redraw();
        }
        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            state.now = *now;
            let target = state.hovered;
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
        let amount = state.glow.interpolate(0.0f32, 1.0, state.now);
        let palette = theme.palette();
        if amount > 0.0 || state.focused {
            renderer.fill_quad(
                Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: palette.accent,
                        width: if state.focused { RING } else { 0.0 },
                        radius: metrics::RADIUS_ROW.into(),
                    },
                    ..Quad::default()
                },
                if amount > 0.0 {
                    palette.lift(LIFT * amount)
                } else {
                    Color::TRANSPARENT
                },
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
        operation.focusable(None, layout.bounds(), tree.state.downcast_mut::<State>());
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
