//! One noise option: a 28 px button as wide as it is given, its fill and its
//! words fading over `QUICK` on hover and when the choice moves (`animate
//! background`, `animate color`, `ui/settings.slint:182`, `:189`). It keeps
//! hover in its own state and asks for frames only while a fade plays.

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{Quad, Style};
use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{
    Alignment, Ellipsis, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::{Border, Color, Event, Font, Length, Pixels, Rectangle, Size, alignment, window};

use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;
use anomi_theme::{Renderer, typography};

const HEIGHT: f32 = 28.0;
const RADIUS: f32 = 4.0;

type Paragraph = Plain<<Renderer as iced::advanced::text::Renderer>::Paragraph>;

pub struct Pick<M> {
    words: &'static str,
    width: f32,
    current: bool,
    on_press: M,
    motion: Motion,
}

/// The option saying `words`, `width` wide, raised when `current`.
pub fn pick<M>(
    words: &'static str,
    width: f32,
    current: bool,
    on_press: M,
    motion: Motion,
) -> Pick<M> {
    Pick {
        words,
        width,
        current,
        on_press,
        motion,
    }
}

struct State {
    /// The words the paragraphs were shaped for, medium and bold.
    shaped: &'static str,
    medium: Paragraph,
    bold: Paragraph,
    hovered: bool,
    pressed: bool,
    lit: Animation<bool>,
    chosen: Animation<bool>,
    now: Instant,
}

fn shape(content: &str, font: Font) -> Text {
    Text {
        content: content.to_owned(),
        bounds: Size::new(f32::INFINITY, HEIGHT),
        size: Pixels(typography::size::SMALL),
        line_height: LineHeight::default(),
        font,
        align_x: Alignment::Center,
        align_y: alignment::Vertical::Center,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
        ellipsis: Ellipsis::None,
        hint_factor: None,
    }
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    if amount <= 0.0 {
        return from;
    }
    if amount >= 1.0 {
        return to;
    }
    let (a, b) = match (from.a == 0.0, to.a == 0.0) {
        (true, false) => (Color { a: 0.0, ..to }, to),
        (false, true) => (from, Color { a: 0.0, ..from }),
        _ => (from, to),
    };
    let blend = |x: f32, y: f32| x + (y - x) * amount;
    Color {
        r: blend(a.r, b.r),
        g: blend(a.g, b.g),
        b: blend(a.b, b.b),
        a: blend(a.a, b.a),
    }
}

impl<M> widget::Meta for Pick<M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Pick<M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            shaped: self.words,
            medium: Paragraph::new(shape(self.words, typography::MEDIUM)),
            bold: Paragraph::new(shape(self.words, typography::BOLD)),
            hovered: false,
            pressed: false,
            lit: Animation::new(false),
            chosen: Animation::new(self.current),
            now: Instant::now(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width), Length::Fixed(HEIGHT))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, _limits: &layout::Limits) {
        let state = tree.state.downcast_mut::<State>();
        if state.shaped != self.words {
            state.shaped = self.words;
            state.medium = Paragraph::new(shape(self.words, typography::MEDIUM));
            state.bold = Paragraph::new(shape(self.words, typography::BOLD));
        }
        tree.size = Size::new(self.width, HEIGHT);
    }

    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.text(None, layout.bounds(), &mut Words(self.words));
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let over = cursor.is_over(layout.bounds());
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                if over != state.hovered {
                    state.hovered = over;
                    shell.request_redraw();
                }
            }
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
                let motion = self.motion;
                let step = |animation: &mut Animation<bool>, target: bool| {
                    if motion.still() {
                        *animation = Animation::new(target);
                    } else if animation.value() != target {
                        *animation = animation
                            .clone()
                            .duration(motion.duration(QUICK))
                            .easing(Easing::Linear)
                            .go(target, *now);
                    }
                    animation.is_animating(*now)
                };
                let moving =
                    step(&mut state.lit, state.hovered) | step(&mut state.chosen, self.current);
                if moving {
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
        _style: &Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let palette = theme.palette();
        let bounds = layout.bounds();
        let lit = state.lit.interpolate(0.0f32, 1.0, state.now);
        let chosen = state.chosen.interpolate(0.0f32, 1.0, state.now);
        let rest = mix(Color::TRANSPARENT, palette.hover, lit);
        let fill = mix(rest, palette.active, chosen);
        if fill.a > 0.0 {
            iced::advanced::Renderer::fill_quad(
                renderer,
                Quad {
                    bounds,
                    border: Border::default().rounded(RADIUS),
                    ..Quad::default()
                },
                fill,
            );
        }
        let words = if chosen >= 0.5 {
            &state.bold
        } else {
            &state.medium
        };
        let ink = mix(palette.secondary, palette.text, chosen);
        let place = bounds.anchor(
            words.min_bounds(),
            Alignment::Center,
            alignment::Vertical::Center,
        );
        renderer.fill_paragraph(
            words.raw(),
            place,
            ink,
            bounds.intersection(viewport).unwrap_or(bounds),
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let _ = tree;
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

/// The words an operation reads from an option.
pub(super) struct Words<'a>(pub &'a str);

impl iced::advanced::widget::operation::Text for Words<'_> {
    fn text(&self) -> iced::advanced::text::Fragment<'_> {
        self.0.into()
    }

    fn select(
        &mut self,
        _start: iced::Point,
        _end: iced::Point,
        _target: iced::advanced::text::Target,
    ) {
    }

    fn select_all(&mut self) {}

    fn deselect(&mut self) {}

    fn copy(&mut self) -> std::option::Option<String> {
        None
    }
}
