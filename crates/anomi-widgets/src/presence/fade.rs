//! A pressable line whose words warm from muted to the text colour under the
//! pointer, over [`QUICK`] (`ui/controls.slint:966`). A button's style can't
//! animate, so the colour is a tree state: it lands at once while the motion
//! is still, and asks for no frame once it has landed.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::{Color, Event, Length, Rectangle, Size, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Element, Motion, Renderer, Theme};

struct State {
    over: bool,
    pressed: bool,
    lit: Animation<bool>,
    /// What `lit` was built with, to rebuild it only when it changes.
    duration: Duration,
    now: Instant,
}

pub struct Fade<'a, Message> {
    content: Element<'a, Message>,
    on_press: Message,
    motion: Motion,
}

impl<'a, Message> Fade<'a, Message> {
    pub fn new(content: Element<'a, Message>, on_press: Message, motion: Motion) -> Self {
        Self {
            content,
            on_press,
            motion,
        }
    }
}

impl<Message> widget::Meta for Fade<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for Fade<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            over: false,
            pressed: false,
            lit: Animation::new(false)
                .duration(self.motion.duration(QUICK))
                .easing(Easing::EaseOut),
            duration: self.motion.duration(QUICK),
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
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let over = cursor.is_over(layout.bounds());
        if over != state.over {
            state.over = over;
            shell.request_redraw();
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                state.pressed = true;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.pressed => {
                state.pressed = false;
                if over {
                    shell.publish(self.on_press.clone());
                }
                shell.capture_event();
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                if state.lit.value() != state.over {
                    // From where the colour is, not from the old target.
                    let duration = self.motion.duration(QUICK);
                    if duration != state.duration {
                        state.duration = duration;
                        state.lit = std::mem::replace(&mut state.lit, Animation::new(false))
                            .duration(duration);
                    }
                    state.lit.go_mut(state.over, *now);
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
        _style: &renderer::Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let lit = state.lit.interpolate(0.0f32, 1.0, state.now);
        let palette = theme.palette();
        let style = renderer::Style {
            text_color: Color {
                r: palette.muted.r + (palette.text.r - palette.muted.r) * lit,
                g: palette.muted.g + (palette.text.g - palette.muted.g) * lit,
                b: palette.muted.b + (palette.text.b - palette.muted.b) * lit,
                a: palette.muted.a + (palette.text.a - palette.muted.a) * lit,
            },
        };
        let (layout, child) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(child, renderer, theme, &style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}
