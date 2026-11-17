//! The ground of a post's card and its click: a rounded box with a rim that
//! steps off the list when the pointer is over it (`background:
//! pointer.has-hover ? Theme.raised : Theme.hover`, `ui/forum.slint:109`),
//! over any content. A click asks to open the post beside the forum; with
//! Shift held at the press, in its place (`:116`-`:122`).

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse, overlay};
use iced::animation::Animation;
use iced::{Background, Border, Color, Event, Length, Rectangle, Size, Vector, keyboard, window};

use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;
use anomi_theme::{Renderer, metrics};

/// `border-width: 1px`.
const RIM: f32 = 1.0;

/// Content on the card's ground, sending `on_open(beside)` when clicked.
pub struct Press<'a, Message> {
    content: Element<'a, Message>,
    on_open: Box<dyn Fn(bool) -> Message + 'a>,
    motion: Motion,
}

impl<'a, Message> Press<'a, Message> {
    pub fn new(
        content: impl View<Message> + 'a,
        motion: Motion,
        on_open: impl Fn(bool) -> Message + 'a,
    ) -> Self {
        Self {
            content: content.boxed(),
            on_open: Box::new(on_open),
            motion,
        }
    }
}

struct State {
    hovered: bool,
    /// The left button went down over the card, with Shift or without.
    pressed: Option<bool>,
    shift: bool,
    lit: Animation<bool>,
    now: Instant,
}

/// `base` towards `other`, `by` of the way.
fn blend(base: Color, other: Color, by: f32) -> Color {
    let step = |a: f32, b: f32| a + (b - a) * by;
    Color {
        r: step(base.r, other.r),
        g: step(base.g, other.g),
        b: step(base.b, other.b),
        a: step(base.a, other.a),
    }
}

impl<Message> widget::Meta for Press<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for Press<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            pressed: None,
            shift: false,
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

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .operate(&mut tree.children[0], layout, viewport, renderer, operation);
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
        self.content.update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            shell,
            viewport,
        );
        if shell.is_event_captured() {
            return;
        }
        let over = cursor.is_over(layout.bounds());
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
                if over != state.hovered =>
            {
                state.hovered = over;
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.shift = modifiers.shift();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { modifiers, .. }) => {
                state.shift = modifiers.shift();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                state.pressed = Some(state.shift);
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(shift) = state.pressed.take()
                    && over
                {
                    shell.publish((self.on_open)(!shift));
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                let lit = state.hovered;
                if self.motion.still() {
                    state.lit = Animation::new(lit);
                } else if state.lit.value() != lit {
                    state.lit = state.lit.clone().duration(QUICK).go(lit, *now);
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
        let palette = theme.palette();
        let lit = state.lit.interpolate(0.0f32, 1.0, state.now);
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: Border {
                    color: blend(palette.line, palette.active, lit),
                    width: RIM,
                    radius: metrics::RADIUS_CARD.into(),
                },
                ..Quad::default()
            },
            Background::Color(blend(palette.hover, palette.raised, lit)),
        );
        self.content.draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let inner =
            self.content
                .mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer);
        if inner == mouse::Interaction::None && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            inner
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
        window: Size,
    ) -> Vec<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}
