//! A role's handle: two columns of three dots at the row's left, which the
//! list picks the role up by (`handle`, `ui/roles.slint:168`-`:194`). Half
//! strong at rest, whole under the pointer or while its role is dragged,
//! fading between the two over `QUICK`; not there at all for a role that
//! can't move. A press on it is the list's: the row under it doesn't open.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::{Border, Event, Length, Rectangle, Size, window};

use anomi_theme::Renderer;
use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;

/// Its width (`width: 20px`), and the dots: 3 px, 6 px apart, the first
/// column 6 px in, the middle row at the row's middle.
pub const WIDTH: f32 = 20.0;
const DOT: f32 = 3.0;
const PITCH: f32 = 6.0;
const LEFT: f32 = 6.0;
const ABOVE: f32 = 7.0;
/// How strong it is at rest (`opacity: ... ? 1 : 0.5`).
const REST: f32 = 0.5;

/// The handle of a role that may move (`usable`), lit while `held`.
pub fn handle(usable: bool, held: bool, motion: Motion) -> Handle {
    Handle {
        usable,
        held,
        motion,
    }
}

pub struct Handle {
    usable: bool,
    held: bool,
    motion: Motion,
}

struct State {
    hovered: bool,
    now: Instant,
    lit: Animation<bool>,
}

impl State {
    /// How lit it is, 0 at rest and 1 under the pointer.
    fn amount(&self) -> f32 {
        self.lit.interpolate(0.0f32, 1.0, self.now)
    }
}

impl widget::Meta for Handle {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Handle {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hovered: false,
            now: Instant::now(),
            lit: Animation::new(self.held),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(WIDTH), Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(WIDTH, Length::Fill, Size::new(WIDTH, 0.0));
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
        let over = self.usable && cursor.is_over(layout.bounds());
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                if over != state.hovered {
                    state.hovered = over;
                    shell.request_redraw();
                }
            }
            // The list picks the role up; the row mustn't open under it.
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                shell.capture_event();
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                let lit = self.usable && (state.hovered || self.held);
                if self.motion.still() {
                    state.lit = Animation::new(lit);
                } else if state.lit.value() != lit {
                    state.lit = state
                        .lit
                        .clone()
                        .duration(QUICK)
                        .easing(Easing::Linear)
                        .go(lit, *now);
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
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        if !self.usable {
            return;
        }
        let state = tree.state.downcast_ref::<State>();
        let strength = REST + (1.0 - REST) * state.amount();
        let colour = theme.palette().muted.scale_alpha(strength);
        let bounds = layout.bounds();
        let top = bounds.y + (bounds.height / 2.0 - ABOVE).floor();
        for dot in 0..6u8 {
            let x = bounds.x + LEFT + f32::from(dot % 2) * PITCH;
            let y = top + f32::from(dot / 2) * PITCH;
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle::new(iced::Point::new(x, y), Size::new(DOT, DOT)),
                    border: Border::default().rounded(DOT / 2.0),
                    ..Quad::default()
                },
                colour,
            );
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.usable && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::None
        }
    }
}
