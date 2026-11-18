//! The well around a field's `text_input`: it draws the sunken ground and
//! the rim, fading them over `Theme.quick` as Slint's `animate background`
//! and `animate border-color` do (`ui/controls.slint:1021`, `:1025`), and it
//! tells when the input gains or loses the keys (`out property <bool>
//! focused`, `:1012`).
//!
//! The input inside is styled with no ground and no border; everything it
//! draws over is the well's. Focus is read with an [`Operation`] over the
//! input before and after each event, so a change made by a focus task is
//! seen at the next event, and one made by a press or Tab at once. In an
//! idle window its caret stops blinking, shown.

use std::time::Instant;

use iced::advanced::Widget as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{self, Id, Tree, tree};
use iced::advanced::{Shell, mouse, overlay, shell};
use iced::animation::{Animation, Easing};
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector, window};

use anomi_theme::motion::QUICK;
use anomi_theme::{Element, Motion, Renderer, Theme};

/// What the well is drawn from, apart from the theme.
#[derive(Clone, Copy)]
pub(super) struct Ground {
    /// The ground (`fill`); the sunken colour when none.
    pub fill: Option<Color>,
    /// The accent ring while it has the keys.
    pub ring: bool,
    /// The line rim at rest.
    pub rim: bool,
    pub radius: f32,
}

pub(super) struct Well<'a, Message> {
    content: Element<'a, Message>,
    on_focus: Option<Box<dyn Fn(bool) -> Message + 'a>>,
    ground: Ground,
    motion: Motion,
}

impl<'a, Message> Well<'a, Message> {
    pub fn new(
        content: Element<'a, Message>,
        ground: Ground,
        motion: Motion,
        on_focus: Option<Box<dyn Fn(bool) -> Message + 'a>>,
    ) -> Self {
        Self {
            content,
            on_focus,
            ground,
            motion,
        }
    }
}

/// Finds whether the first focusable under it has the keys.
struct Probe(Option<bool>);

impl Operation for Probe {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn focusable(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Focusable) {
        if self.0.is_none() {
            self.0 = Some(state.is_focused());
        }
    }
}

/// What the well last saw, and the two fades.
struct State {
    focused: bool,
    /// The ring's fade: `true` is the focused look.
    ring: Animation<bool>,
    /// The ground's: `false` is `from`, `true` is `to`.
    swap: Animation<bool>,
    from: Option<Color>,
    to: Option<Color>,
    /// A fade that was cut short: the target it was heading for and how far
    /// it had got, so the new one starts from the colour on screen.
    cut: Option<(Option<Color>, f32)>,
    now: Instant,
}

impl State {
    fn new(ground: &Ground) -> Self {
        Self {
            focused: false,
            ring: Animation::new(false),
            swap: Animation::new(true),
            from: ground.fill,
            to: ground.fill,
            cut: None,
            now: Instant::now(),
        }
    }

    /// Moves the fades to the frame at `now`, starting one where the target
    /// changed; whether a frame after this one is wanted. Still, they land
    /// at once.
    fn advance(&mut self, now: Instant, ground: &Ground, motion: Motion) -> bool {
        self.now = now;
        if motion.still() {
            self.ring = Animation::new(self.focused);
            self.swap = Animation::new(true);
            self.from = ground.fill;
            self.to = ground.fill;
            self.cut = None;
            return false;
        }
        let fade = |from: bool| {
            Animation::new(from)
                .duration(motion.duration(QUICK))
                .easing(Easing::Linear)
        };
        if self.ring.value() != self.focused {
            if self.ring.is_animating(now) {
                self.ring.go_mut(self.focused, now);
            } else {
                self.ring = fade(self.ring.value()).go(self.focused, now);
            }
        }
        if self.to != ground.fill {
            if self.swap.is_animating(now) {
                let t = self.swap.interpolate(0.0f32, 1.0, now);
                // Of a fade already cut once, the nearer end stands in.
                if let Some((was, at)) = self.cut
                    && at >= 0.5
                {
                    self.from = was;
                }
                self.cut = Some((self.to, t));
            } else {
                self.from = self.to;
                self.cut = None;
            }
            self.to = ground.fill;
            self.swap = fade(false).go(true, now);
        }
        self.ring.is_animating(now) || self.swap.is_animating(now)
    }
}

impl State {
    /// The ground's colour on screen: the fade from `from` to `to`, itself
    /// started from the colour a cut one had reached.
    fn fill(&self, sunken: Color) -> Color {
        let sunken = |fill: Option<Color>| fill.unwrap_or(sunken);
        let start = match self.cut {
            Some((was, at)) => blend(sunken(self.from), sunken(was), at),
            None => sunken(self.from),
        };
        blend(
            start,
            sunken(self.to),
            self.swap.interpolate(0.0f32, 1.0, self.now),
        )
    }
}

fn blend(from: Color, to: Color, amount: f32) -> Color {
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    Color {
        r: mix(from.r, to.r),
        g: mix(from.g, to.g),
        b: mix(from.b, to.b),
        a: mix(from.a, to.a),
    }
}

impl<Message> widget::Meta for Well<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Well<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new(&self.ground))
    }

    fn size(&self) -> Size<Length> {
        self.content.size()
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let child = &mut tree.children[0];
        self.content.layout(child, renderer, limits);
        tree.size = child.size;
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
        let state = tree.state.downcast_mut::<State>();
        let (layout, child) = layout.iter_mut(&mut tree.children).next().unwrap();
        // Focus changes with a press, a key or a task, never with a move.
        let probing = !matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. }));

        if probing {
            let seen = self.focused(child, layout, viewport, renderer);
            self.sync(state, seen, shell);
        }
        // In an idle window the caret holds, shown, as the composer's does
        // (`Motion::idle`): the input hears the window come to the front,
        // which shows its caret anew, and the blink it then asks for is
        // dropped, so a focused field in an idle window asks for no frame.
        let held =
            self.motion.idle && matches!(event, Event::Window(window::Event::RedrawRequested(_)));
        if held {
            let mut told = shell::Bus::new();
            let mut quiet = shell.local(&mut told);
            let front = Event::Window(window::Event::Focused);
            self.content.update(
                child, &front, layout, cursor, renderer, &mut quiet, viewport,
            );
        }
        let before = shell.redraw_request();
        self.content
            .update(child, event, layout, cursor, renderer, shell, viewport);
        if held {
            Shell::replace_redraw_request(shell, before);
        }
        if probing {
            let seen = self.focused(child, layout, viewport, renderer);
            self.sync(state, seen, shell);
        }

        if let Event::Window(window::Event::RedrawRequested(now)) = event
            && state.advance(*now, &self.ground, self.motion)
        {
            shell.request_redraw();
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
        let palette = theme.palette();
        let ground = &self.ground;
        let fill = state.fill(palette.sunken);
        // Slint's rim colour and width (`ui/controls.slint:1022`): only the
        // alpha fades, and the width drops as the keys leave.
        let rest = if ground.rim {
            palette.line
        } else {
            palette.accent.scale_alpha(0.0)
        };
        let lit = if ground.ring {
            palette.accent.scale_alpha(0.8)
        } else {
            rest
        };
        let rim = blend(rest, lit, state.ring.interpolate(0.0f32, 1.0, state.now));
        let width = if (ground.ring && state.focused)
            || (ground.rim && state.to.unwrap_or(palette.sunken).a > 0.0)
        {
            1.0
        } else {
            0.0
        };
        renderer.fill_quad(
            Quad {
                bounds: layout.bounds(),
                border: Border::default()
                    .rounded(ground.radius)
                    .width(width)
                    .color(rim),
                ..Quad::default()
            },
            fill,
        );

        let (layout, tree) = layout.iter(&tree.children).next().unwrap();
        self.content
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
        let (layout, tree) = layout.iter_mut(&mut tree.children).next().unwrap();
        self.content
            .overlay(tree, layout, renderer, viewport, translation, window)
    }
}

impl<Message> Well<'_, Message> {
    fn focused(
        &mut self,
        child: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> bool {
        let mut probe = Probe(None);
        self.content
            .operate(child, layout, viewport, renderer, &mut probe);
        probe.0.unwrap_or(false)
    }

    /// Takes in what the input shows: on a change, tells the app and asks
    /// for the frame the fade starts on.
    fn sync(&self, state: &mut State, seen: bool, shell: &mut Shell<'_, Message>) {
        if seen == state.focused {
            return;
        }
        state.focused = seen;
        if let Some(on_focus) = &self.on_focus {
            shell.publish(on_focus(seen));
        }
        shell.request_redraw();
    }
}

#[cfg(test)]
mod tests;
