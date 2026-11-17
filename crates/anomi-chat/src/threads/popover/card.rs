//! A thread's card: a 72 px well with a 1 px border that lights under the
//! pointer over `QUICK` (`ThreadEntry`, `ui/header.slint:319`). A click
//! opens it, a click with Shift beside the channel; the card tells which,
//! which `mouse_area` cannot.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::{Border, Event, Length, Rectangle, Size, Vector, keyboard, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Element, Motion, Renderer, Theme, View};

use super::ring::{Lit, mix};

/// The card's height and its corners.
pub const HEIGHT: f32 = 72.0;
const RADIUS: f32 = 8.0;

pub struct Card<'a, M> {
    content: Element<'a, M>,
    on_press: Box<dyn Fn(bool) -> M + 'a>,
    motion: Motion,
    lit: Lit,
}

/// `content` as a card; `on_press` is told whether Shift was held. The card
/// writes how lit it is to `lit` as it draws, for what it holds to follow.
pub fn card<'a, M>(
    content: impl View<M> + 'a,
    on_press: impl Fn(bool) -> M + 'a,
    motion: Motion,
    lit: Lit,
) -> Card<'a, M> {
    Card {
        content: content.boxed(),
        on_press: Box::new(on_press),
        motion,
        lit,
    }
}

struct State {
    hovered: bool,
    pressed: bool,
    shift: bool,
    /// Whether the card is lit, easing.
    lit: Animation<bool>,
    now: Instant,
}

impl<M> widget::Meta for Card<'_, M> {}

impl<M> widget::Widget<M, Theme, Renderer> for Card<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            pressed: false,
            shift: false,
            lit: Animation::new(false).duration(self.motion.duration(QUICK)),
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(HEIGHT))
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let limits = limits.width(Length::Fill).height(Length::Fixed(HEIGHT));
        self.content
            .layout(&mut tree.children[0], renderer, &limits);
        tree.size = limits.resolve(Length::Fill, Length::Fixed(HEIGHT), Size::ZERO);
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
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.shift = modifiers.shift();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
                if over != state.hovered =>
            {
                state.hovered = over;
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                state.pressed = true;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if std::mem::take(&mut state.pressed) && over {
                    shell.publish((self.on_press)(state.shift));
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                if state.lit.value() != state.hovered {
                    if !state.lit.is_animating(*now) {
                        state.lit = state.lit.clone().duration(self.motion.duration(QUICK));
                    }
                    state.lit.go_mut(state.hovered, *now);
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
        let lit = state.lit.interpolate(0.0, 1.0, state.now);
        self.lit.set(lit);
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: Border::default().rounded(RADIUS).width(1).color(mix(
                    palette.active,
                    palette.line,
                    lit,
                )),
                ..Quad::default()
            },
            mix(palette.raised, palette.hover, lit),
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
        if cursor.is_over(layout.bounds()) {
            return mouse::Interaction::Pointer;
        }
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
