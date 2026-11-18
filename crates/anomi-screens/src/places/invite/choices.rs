//! A trough of options, one chosen: `Choices` filling its row
//! (`ui/settings.slint:132`), as the invite dialog's expiry and uses use it.
//! The settings pages' own is S01's, private to `anomi-settings`; this one
//! is the `fill` case with all of Slint's behaviour: each option at least
//! its bold words and 20 px, the spare shared evenly (see [`lay`]); the
//! hover and chosen colours of the background and the words ease over
//! `Theme.quick`, landing at once while still; Tab reaches each option and
//! Space or Enter picks it.

mod lay;

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{Quad, Renderer as _, Style};
use iced::advanced::text::{Alignment, Fragment, Renderer as _, Target};
use iced::advanced::widget::operation::{Focusable, Text};
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::{Animation, Easing};
use iced::keyboard::{self, key::Named};
use iced::{Border, Color, Event, Length, Rectangle, Size, Vector, alignment, window};

use anomi_theme::fonts::{self, Version};
use anomi_theme::metrics::{RADIUS_CARD, RADIUS_FIELD};
use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;
use anomi_theme::{Renderer, typography};

use lay::{OPTION, PAD, Paragraph, arrange, key, least, mix, shape, width_of};

pub use lay::natural;

/// The ring round an option the keys reached.
const RING: f32 = 2.0;

pub struct Choices<'a, M> {
    labels: Vec<&'a str>,
    current: usize,
    on_pick: Box<dyn Fn(usize) -> M + 'a>,
    motion: Motion,
}

/// A trough of `labels` sharing its row, `current` lit; a click or Space on
/// option `i` sends `pick(i)`.
pub fn choices<'a, M: Clone + 'a>(
    labels: &[&'a str],
    current: usize,
    motion: Motion,
    pick: impl Fn(usize) -> M + 'a,
) -> Element<'a, M> {
    Choices {
        labels: labels.to_vec(),
        current,
        on_pick: Box::new(pick),
        motion,
    }
    .boxed()
}

/// One option's keys, as the focus walk sees it.
#[derive(Default)]
struct Keys(bool);

impl Focusable for Keys {
    fn is_focused(&self) -> bool {
        self.0
    }

    fn focus(&mut self) {
        self.0 = true;
    }

    fn unfocus(&mut self) {
        self.0 = false;
    }
}

/// An option's words as operations read text (a test finds an option by
/// them); they can't be selected.
struct Words<'a>(&'a str);

impl Text for Words<'_> {
    fn text(&self) -> Fragment<'_> {
        self.0.into()
    }

    fn select(&mut self, _start: iced::Point, _end: iced::Point, _target: Target) {}

    fn select_all(&mut self) {}

    fn deselect(&mut self) {}

    fn copy(&mut self) -> Option<String> {
        None
    }
}

struct State {
    key: u64,
    /// Each option's words, medium and bold, and the fonts they're in.
    words: Vec<(Paragraph, Paragraph)>,
    fonts: Version,
    placed: Vec<Rectangle>,
    hovered: Option<usize>,
    keys: Vec<Keys>,
    lit: Vec<Animation<bool>>,
    chosen: Vec<Animation<bool>>,
    now: Instant,
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
            fonts: Version::default(),
            placed: Vec::new(),
            hovered: None,
            keys: Vec::new(),
            lit: Vec::new(),
            chosen: Vec::new(),
            now: Instant::now(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let key = key(&self.labels);
        let state = tree.state.downcast_mut::<State>();
        let fresh = state.key != key || state.words.len() != self.labels.len();
        let fonts = fonts::version();
        if fresh || state.fonts != fonts {
            state.fonts = fonts;
            state.words = self
                .labels
                .iter()
                .map(|label| {
                    (
                        Paragraph::new(shape(label, typography::MEDIUM)),
                        Paragraph::new(shape(label, typography::BOLD)),
                    )
                })
                .collect();
        }
        if fresh {
            state.key = key;
            let (count, current) = (self.labels.len(), self.current);
            state.keys = (0..count).map(|_| Keys::default()).collect();
            state.lit = vec![Animation::new(false); count];
            state.chosen = (0..count).map(|i| Animation::new(i == current)).collect();
        }
        let least = least(state.words.iter().map(|(_, bold)| bold.min_width()));
        let available = limits.max.width;
        let wide = if available.is_finite() {
            available
        } else {
            width_of(&least)
        };
        state.placed = arrange(&least, wide - 2.0 * PAD);
        tree.size = Size::new(wide, 2.0 * PAD + OPTION);
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
        let state = tree.state.downcast_mut::<State>();
        for ((keys, at), label) in state.keys.iter_mut().zip(&state.placed).zip(&self.labels) {
            let at = *at + Vector::new(origin.x, origin.y);
            operation.focusable(None, at, keys);
            operation.text(None, at, &mut Words(label));
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
        let over = |state: &State| {
            state
                .placed
                .iter()
                .position(|at| cursor.is_over(*at + Vector::new(origin.x, origin.y)))
        };
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hovered = over(state);
                if hovered != state.hovered {
                    state.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                let hit = over(state);
                if state.keys.iter().any(|keys| keys.0) {
                    for keys in &mut state.keys {
                        keys.0 = false;
                    }
                    shell.request_redraw();
                }
                if let (mouse::Button::Left, Some(index)) = (button, hit)
                    && !shell.is_event_captured()
                {
                    shell.publish((self.on_pick)(index));
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                if !shell.is_event_captured()
                    && matches!(
                        key.as_ref(),
                        keyboard::Key::Named(Named::Space | Named::Enter)
                    ) =>
            {
                if let Some(index) = state.keys.iter().position(|keys| keys.0) {
                    shell.publish((self.on_pick)(index));
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                let (motion, current) = (self.motion, self.current);
                let mut moving = false;
                let mut step = |animation: &mut Animation<bool>, target: bool| {
                    if motion.still() {
                        *animation = Animation::new(target);
                    } else if animation.value() != target {
                        *animation = animation
                            .clone()
                            .duration(QUICK)
                            .easing(Easing::Linear)
                            .go(target, *now);
                    }
                    moving |= animation.is_animating(*now);
                };
                for index in 0..state.lit.len() {
                    step(&mut state.lit[index], state.hovered == Some(index));
                    step(&mut state.chosen[index], index == current);
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
        renderer.fill_quad(
            Quad {
                bounds,
                border: Border::default()
                    .rounded(RADIUS_CARD)
                    .width(1)
                    .color(palette.line),
                ..Quad::default()
            },
            palette.sunken,
        );
        let amount = |animation: &Animation<bool>| animation.interpolate(0.0f32, 1.0, state.now);
        for (index, at) in state.placed.iter().enumerate() {
            let at = *at + Vector::new(bounds.x, bounds.y);
            let (Some(lit), Some(chosen)) = (state.lit.get(index), state.chosen.get(index)) else {
                break;
            };
            let (lit, chosen) = (amount(lit), amount(chosen));
            let rest = mix(Color::TRANSPARENT, palette.hover, lit);
            let face = mix(rest, palette.active, chosen);
            if face.a > 0.0 {
                renderer.fill_quad(
                    Quad {
                        bounds: at,
                        border: Border::default().rounded(RADIUS_FIELD),
                        ..Quad::default()
                    },
                    face,
                );
            }
            if state.keys[index].0 {
                renderer.fill_quad(
                    Quad {
                        bounds: at,
                        border: Border::default()
                            .rounded(RADIUS_FIELD)
                            .width(RING)
                            .color(palette.text),
                        ..Quad::default()
                    },
                    Color::TRANSPARENT,
                );
            }
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
        if tree.state.downcast_ref::<State>().hovered.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}
