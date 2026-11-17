//! A pill of a forum post's bar: what it shows on the raised ground, the
//! field's under the pointer, the fill fading between them over `QUICK` as
//! Slint's `animate background { duration: Theme.quick; }` does
//! (`ui/yapfast.slint:3562`, `:3587`, `:3604`).
//!
//! A button's style can't animate, so the fill is tree state: frames are
//! asked for only while it fades, and it lands at once while the motion is
//! still, even on its way. A pill given a press sends it as a button does,
//! on a release over it; one without leaves the press to what wraps it
//! (the React pill's `opener`).

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;
use anomi_theme::{Element, Renderer};

/// The pills' corners (`border-radius: 8px`, `:3561`).
const RADIUS: f32 = 8.0;

pub struct Pill<'a, Message> {
    content: Element<'a, Message>,
    on_press: Option<Message>,
    motion: Motion,
}

/// A pill around `content`, which sets its size.
pub fn pill<'a, Message>(content: impl View<Message> + 'a, motion: Motion) -> Pill<'a, Message> {
    Pill {
        content: content.boxed(),
        on_press: None,
        motion,
    }
}

impl<Message> Pill<'_, Message> {
    /// A click on it sends this.
    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message);
        self
    }
}

struct State {
    over: bool,
    pressed: bool,
    /// The fill, from the raised ground's `false` to the field's `true`.
    lit: Animation<bool>,
    /// The motion `lit` was built for was still.
    still: bool,
    now: Instant,
}

impl State {
    fn new(motion: Motion) -> Self {
        Self {
            over: false,
            pressed: false,
            lit: fade(false, motion),
            still: motion.still(),
            now: Instant::now(),
        }
    }

    /// A frame at `now`: the fill heads for the pointer's state, landing
    /// where it was going when the motion stills or plays again. Whether
    /// it still fades.
    fn frame(&mut self, motion: Motion, now: Instant) -> bool {
        self.now = now;
        if motion.still() != self.still {
            self.still = motion.still();
            self.lit = fade(self.lit.value(), motion);
        }
        self.lit.go_mut(self.over, now);
        self.lit.is_animating(now)
    }

    /// How far the fill is from the raised ground to the field's.
    fn amount(&self, motion: Motion) -> f32 {
        if motion.still() {
            f32::from(self.over)
        } else {
            self.lit.interpolate(0.0f32, 1.0, self.now)
        }
    }
}

/// The fill settled at `lit`, fading linearly as Slint's `animate` does.
fn fade(lit: bool, motion: Motion) -> Animation<bool> {
    motion.animation(lit, QUICK, Easing::Linear)
}

impl<Message> widget::Meta for Pill<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for Pill<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new(self.motion))
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        self.content.layout(&mut tree.children[0], renderer, limits);
        tree.children[0].translation = Vector::ZERO;
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
        operation.traverse(&mut |operation| {
            if let Some((layout, child)) = layout.iter_mut(&mut tree.children).next() {
                self.content
                    .operate(child, layout, viewport, renderer, operation);
            }
        });
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
        let over = cursor.is_over(layout.bounds());
        if let Some((layout, child)) = layout.iter_mut(&mut tree.children).next() {
            self.content
                .update(child, event, layout, cursor, renderer, shell, viewport);
        }
        let state = tree.state.downcast_mut::<State>();
        if over != state.over {
            state.over = over;
            shell.request_redraw();
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if over && self.on_press.is_some() =>
            {
                state.pressed = true;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.pressed => {
                state.pressed = false;
                if let Some(message) = self.on_press.as_ref().filter(|_| over) {
                    shell.publish(message.clone());
                }
                shell.capture_event();
            }
            // Each frame moves the fill on; one still fading asks for the next.
            Event::Window(window::Event::RedrawRequested(now))
                if state.frame(self.motion, *now) =>
            {
                shell.request_redraw();
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
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: Border::default().rounded(RADIUS),
                ..Quad::default()
            },
            blend(palette.raised, palette.field, state.amount(self.motion)),
        );
        if let Some((layout, child)) = layout.iter(&tree.children).next() {
            self.content
                .draw(child, renderer, theme, style, layout, cursor, viewport);
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
        if self.on_press.is_some() && cursor.is_over(layout.bounds()) {
            return mouse::Interaction::Pointer;
        }
        layout
            .iter(&tree.children)
            .next()
            .map_or(mouse::Interaction::None, |(layout, child)| {
                self.content
                    .mouse_interaction(child, layout, cursor, viewport, renderer)
            })
    }
}

/// `amount` of the way from `from` to `to`.
fn blend(from: Color, to: Color, amount: f32) -> Color {
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    Color {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYING: Motion = Motion {
        front: true,
        never: false,
        idle: false,
    };
    const BEHIND: Motion = Motion {
        front: false,
        ..PLAYING
    };

    #[test]
    fn the_fill_fades_under_the_pointer_and_back_from_where_it_is() {
        let start = Instant::now();
        let mut state = State::new(PLAYING);
        state.over = true;
        assert!(state.frame(PLAYING, start), "it asks for the next frame");
        assert!(state.frame(PLAYING, start + QUICK / 2));
        let half = state.amount(PLAYING);
        assert!((half - 0.5).abs() < 0.01, "linear: {half}");
        // Left half way, it turns round from there.
        state.over = false;
        state.frame(PLAYING, start + QUICK / 2);
        assert!((state.amount(PLAYING) - half).abs() < 0.01);
        assert!(!state.frame(PLAYING, start + QUICK * 2), "landed, no frame");
        assert_eq!(state.amount(PLAYING), 0.0);
    }

    #[test]
    fn behind_another_window_the_fill_lands_at_once() {
        let start = Instant::now();
        let mut state = State::new(BEHIND);
        state.over = true;
        assert!(!state.frame(BEHIND, start), "no frame while still");
        assert_eq!(state.amount(BEHIND), 1.0);
        // Stilled on its way, it lands where it was going.
        let mut state = State::new(PLAYING);
        state.over = true;
        state.frame(PLAYING, start);
        assert!(!state.frame(BEHIND, start + QUICK / 4));
        assert_eq!(state.amount(BEHIND), 1.0);
        // In front again, it stays there without asking for frames.
        assert!(!state.frame(PLAYING, start + QUICK / 2));
        assert_eq!(state.amount(PLAYING), 1.0);
    }
}
