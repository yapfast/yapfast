//! Exclusive choices in a well, the current one raised. On a settings page
//! it is as wide as its options' text and at least `widest`; in a dialog it
//! fills the row, its options sharing it evenly; a set too long for its row
//! always goes on to more lines, as Slint's flexbox does (`wrap` only keeps
//! the least width that lets it). The Slint side: `Choices`
//! (`ui/settings.slint:132`): 4 px of padding and gap, 28 px options with
//! 10 px each side of their text, 4 px corners, a well with a rim; a click
//! only reports the choice (`controlled`: the page keeps the value).
//!
//! It is one widget that measures its options' words itself, in bold as the
//! Slint measure does, so the well is as wide as they need and no more.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Style;
use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{
    Alignment, Ellipsis, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::{Border, Event, Font, Length, Pixels, Rectangle, Size, alignment, window};

use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;
use anomi_theme::{Renderer, typography};

use super::arrange::{GAP, OPTION, PAD, arrange};
use super::press::{Words, fill, mix};

const SIDE: f32 = 10.0;
const RADIUS: f32 = 4.0;
const WELL_RADIUS: f32 = 8.0;
/// The widest a set stays on one line where the row has no room for it.
const LINE_MAX: f32 = 480.0;
/// How wide a page's set grows at least, while the row has room.
const WIDEST: f32 = 360.0;

type Paragraph = Plain<<Renderer as iced::advanced::text::Renderer>::Paragraph>;

pub struct Choices<'a, M> {
    options: Vec<&'a str>,
    current: usize,
    widest: f32,
    fill: bool,
    wrap: bool,
    on_pick: Option<Box<dyn Fn(usize) -> M + 'a>>,
    motion: Motion,
}

/// Choices `options`, `current` raised; chain [`Choices::on_pick`] (what a
/// click on option `i` sends), [`Choices::widest`], [`Choices::fill`],
/// [`Choices::wrap`].
pub fn choices<'a, M, S: AsRef<str>>(
    options: &'a [S],
    current: usize,
    motion: Motion,
) -> Choices<'a, M> {
    Choices {
        options: options.iter().map(AsRef::as_ref).collect(),
        current,
        widest: WIDEST,
        fill: false,
        wrap: false,
        on_pick: None,
        motion,
    }
}

impl<'a, M> Choices<'a, M> {
    pub fn on_pick(mut self, on_pick: impl Fn(usize) -> M + 'a) -> Self {
        self.on_pick = Some(Box::new(on_pick));
        self
    }

    /// How wide it grows at least while the row has room.
    pub fn widest(mut self, widest: f32) -> Self {
        self.widest = widest;
        self
    }

    /// It fills the row, its options sharing it evenly.
    pub fn fill(mut self, fill: bool) -> Self {
        self.fill = fill;
        self
    }

    /// Its options go on to more lines wherever the row is narrower than
    /// them.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn view(self) -> Element<'a, M>
    where
        M: Clone + 'a,
    {
        self.boxed()
    }
}

struct State {
    /// The options' hash, and each one's words shaped medium and bold.
    key: u64,
    words: Vec<(Paragraph, Paragraph)>,
    /// Where each option sits in the well, from its top left.
    placed: Vec<Rectangle>,
    hovered: Option<usize>,
    /// The option a press began on; it is chosen on the release over it.
    pressed: Option<usize>,
    lit: Vec<Animation<bool>>,
    chosen: Vec<Animation<bool>>,
    now: Instant,
}

fn shape(content: &str, font: Font) -> Text {
    Text {
        content: content.to_owned(),
        bounds: Size::new(f32::INFINITY, OPTION),
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

impl<M> Choices<'_, M> {
    fn key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.options.hash(&mut hasher);
        hasher.finish()
    }

    /// Each option's least width, as wide as its bold words and 10 px each side.
    fn least(state: &State) -> Vec<f32> {
        state
            .words
            .iter()
            .map(|(_, bold)| bold.min_width().ceil() + 2.0 * SIDE)
            .collect()
    }
}

impl<M> widget::Meta for Choices<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Choices<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            key: 0,
            words: Vec::new(),
            placed: Vec::new(),
            hovered: None,
            pressed: None,
            lit: Vec::new(),
            chosen: Vec::new(),
            now: Instant::now(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(
            if self.fill {
                Length::Fill
            } else {
                Length::Shrink
            },
            Length::Shrink,
        )
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let key = self.key();
        let state = tree.state.downcast_mut::<State>();
        if state.key != key || state.words.len() != self.options.len() {
            state.key = key;
            state.words = self
                .options
                .iter()
                .map(|option| {
                    (
                        Paragraph::new(shape(option, typography::MEDIUM)),
                        Paragraph::new(shape(option, typography::BOLD)),
                    )
                })
                .collect();
            let current = self.current;
            state.lit = vec![Animation::new(false); self.options.len()];
            state.chosen = (0..self.options.len())
                .map(|i| Animation::new(i == current))
                .collect();
        }
        let least = Self::least(state);
        let natural =
            least.iter().sum::<f32>() + GAP * least.len().saturating_sub(1) as f32 + 2.0 * PAD;
        let available = limits.max.width;
        let wide = if self.fill && available.is_finite() {
            available
        } else if available.is_finite() {
            available.min(self.widest.max(natural))
        } else {
            self.widest.max(natural)
        };
        let wide = if self.wrap {
            wide
        } else {
            wide.max(natural.min(LINE_MAX).min(available))
        };
        let (placed, height) = arrange(&least, wide - 2.0 * PAD, true);
        state.placed = placed;
        tree.size = Size::new(wide, height);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let origin = layout.position();
        let state = tree.state.downcast_ref::<State>();
        for (option, at) in self.options.iter().zip(&state.placed) {
            let at = *at + iced::Vector::new(origin.x, origin.y);
            operation.text(None, at, &mut Words(option));
        }
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
        let origin = layout.position();
        let state = tree.state.downcast_mut::<State>();
        let over = |cursor: mouse::Cursor| {
            state
                .placed
                .iter()
                .position(|at| cursor.is_over(*at + iced::Vector::new(origin.x, origin.y)))
        };
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hovered = over(cursor);
                if hovered != state.hovered {
                    state.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let (Some(index), Some(_)) = (over(cursor), &self.on_pick) {
                    state.pressed = Some(index);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(pressed) = state.pressed.take() {
                    if let (true, Some(on_pick)) = (over(cursor) == Some(pressed), &self.on_pick) {
                        shell.publish(on_pick(pressed));
                    }
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                let motion = self.motion;
                let mut moving = false;
                let step = |animation: &mut Animation<bool>, target: bool| {
                    if motion.still() {
                        *animation = Animation::new(target);
                    } else if animation.value() != target {
                        *animation = animation
                            .clone()
                            .duration(QUICK)
                            .easing(Easing::Linear)
                            .go(target, *now);
                    }
                    animation.is_animating(*now)
                };
                for index in 0..state.lit.len() {
                    moving |= step(&mut state.lit[index], state.hovered == Some(index));
                    moving |= step(&mut state.chosen[index], index == self.current);
                }
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
        iced::advanced::Renderer::fill_quad(
            renderer,
            iced::advanced::renderer::Quad {
                bounds,
                border: Border::default()
                    .rounded(WELL_RADIUS)
                    .width(1)
                    .color(palette.line),
                ..Default::default()
            },
            palette.sunken,
        );
        let amount = |animation: &Animation<bool>| animation.interpolate(0.0f32, 1.0, state.now);
        for (index, at) in state.placed.iter().enumerate() {
            let at = *at + iced::Vector::new(bounds.x, bounds.y);
            let (Some(lit), Some(chosen)) = (state.lit.get(index), state.chosen.get(index)) else {
                break;
            };
            let (lit, chosen) = (amount(lit), amount(chosen));
            let rest = mix(iced::Color::TRANSPARENT, palette.hover, lit);
            fill(renderer, at, RADIUS, mix(rest, palette.active, chosen));
            let (medium, bold) = &state.words[index];
            let words = if chosen >= 0.5 { bold } else { medium };
            let ink = mix(palette.secondary, palette.text, chosen);
            let place = at.anchor(
                words.min_bounds(),
                Alignment::Center,
                alignment::Vertical::Center,
            );
            renderer.fill_paragraph(
                words.raw(),
                place,
                ink,
                at.intersection(viewport).unwrap_or(at),
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        _layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if tree.state.downcast_ref::<State>().hovered.is_some() && self.on_pick.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}
