//! The bar of an update being prepared: the accent over the sidebar's
//! colour, its fill gliding to each new amount in 200 ms as Slint's `animate
//! width` did, and landing at once while the window holds still.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::{Border, Event, Length, Rectangle, Size, window};

use anomi_theme::{Motion, Renderer, Theme};

/// Its height; the ends are round.
const HEIGHT: f32 = 6.0;
/// How long the fill takes to reach a new amount (`animate width`).
const GLIDE: Duration = Duration::from_millis(200);

/// How much is done, 0 to 1.
pub fn bar(done: f32, motion: Motion) -> Bar {
    Bar {
        done: done.clamp(0.0, 1.0),
        motion,
    }
}

pub struct Bar {
    done: f32,
    motion: Motion,
}

struct State {
    fill: Animation<f32>,
    now: Instant,
}

impl widget::Meta for Bar {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Bar {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            fill: Animation::new(self.done),
            now: Instant::now(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(HEIGHT))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fill, HEIGHT, Size::new(0.0, HEIGHT));
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        state.now = *now;
        if self.motion.still() {
            state.fill = Animation::new(self.done);
            return;
        }
        if state.fill.value() != self.done {
            let from = state.fill.interpolate(*now);
            state.fill = Animation::new(from)
                .duration(GLIDE)
                .easing(Easing::Linear)
                .go(self.done, *now);
        }
        if state.fill.is_animating(*now) {
            shell.request_redraw();
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
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let palette = theme.palette();
        let quad = |bounds: Rectangle| Quad {
            bounds,
            border: Border::default().rounded(HEIGHT / 2.0),
            ..Quad::default()
        };
        renderer.fill_quad(quad(bounds), palette.sidebar);
        let width = bounds.width * state.fill.interpolate(state.now).clamp(0.0, 1.0);
        if width > 0.0 {
            renderer.fill_quad(quad(Rectangle { width, ..bounds }), palette.accent);
        }
    }
}
