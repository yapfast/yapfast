//! The switch's widget: its label row and what it does as children, the
//! track drawn at the row's end (see the module above).

use std::time::Instant;

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::Style;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::animation::Animation;
use iced::keyboard::{self, key::Named};
use iced::{Event, Length, Point, Rectangle, Size, Vector, window};

use anomi_theme::motion::QUICK;
use anomi_theme::prelude::*;
use anomi_theme::{Renderer, metrics};

use crate::kit::press::{GLIDE, Hot, RING, fill, mix, ring};

pub(super) const TRACK_WIDTH: f32 = metrics::TOGGLE_WIDTH;
const TRACK_HEIGHT: f32 = metrics::TOGGLE_HEIGHT;
const KNOB: f32 = metrics::TOGGLE_KNOB;
const INSET: f32 = metrics::TOGGLE_KNOB_INSET;
/// Between the label and the track (`spacing: 12px`), the row and what it
/// does (`spacing: 4px`).
pub(super) const GAP: f32 = 12.0;
const BELOW: f32 = 4.0;

pub(super) struct Body<'a, M> {
    /// The label's row, what it does if it says, and, if `track_tip`, last,
    /// the part over the track that holds its tooltip.
    pub children: Vec<Element<'a, M>>,
    pub track_tip: bool,
    pub on: bool,
    pub enabled: bool,
    pub on_toggle: Option<Box<dyn Fn(bool) -> M + 'a>>,
    pub motion: Motion,
}

struct State {
    hot: Hot,
    on: Animation<bool>,
    enabled: Animation<bool>,
    now: Instant,
}

impl State {
    fn advance(&mut self, now: Instant, on: bool, enabled: bool, motion: Motion) -> bool {
        self.now = now;
        let step = |animation: &Animation<bool>, target: bool| {
            if motion.still() {
                Animation::new(target)
            } else if animation.value() != target {
                animation
                    .clone()
                    .duration(QUICK)
                    .easing(GLIDE)
                    .go(target, now)
            } else {
                animation.clone()
            }
        };
        self.on = step(&self.on, on);
        self.enabled = step(&self.enabled, enabled);
        let lit = self.hot.hovered && enabled;
        let moving = self.hot.advance(now, lit, motion);
        moving || self.on.is_animating(now) || self.enabled.is_animating(now)
    }
}

impl<M> widget::Meta for Body<'_, M> {}

impl<M> Body<'_, M> {
    /// The row of label and track, without what it does below.
    fn row_of(&self, tree: &Tree, layout: Layout) -> Rectangle {
        let head = tree.children[0].size;
        let origin = layout.position();
        Rectangle::new(
            origin,
            Size::new(layout.bounds().width, head.height.max(TRACK_HEIGHT)),
        )
    }
}

impl<M> widget::Widget<M, Theme, Renderer> for Body<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hot: Hot::new(),
            on: Animation::new(self.on),
            enabled: Animation::new(self.enabled),
            now: Instant::now(),
        })
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) {
        let width = limits.max.width;
        let room = layout::Limits::new(
            Size::ZERO,
            Size::new((width - TRACK_WIDTH - GAP).max(0.0), f32::INFINITY),
        );
        self.children[0].layout(&mut tree.children[0], renderer, &room);
        let head = tree.children[0].size;
        let line = head.height.max(TRACK_HEIGHT);
        tree.children[0].translation = Vector::new(0.0, (line - head.height) / 2.0);
        let mut height = line;
        let note = self.children.len() - usize::from(self.track_tip) > 1;
        if note {
            let note = &mut self.children[1];
            let below = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
            note.layout(&mut tree.children[1], renderer, &below);
            tree.children[1].translation = Vector::new(0.0, line + BELOW);
            height = line + BELOW + tree.children[1].size.height;
        }
        if self.track_tip {
            let last = self.children.len() - 1;
            let track = layout::Limits::new(Size::ZERO, Size::new(TRACK_WIDTH, TRACK_HEIGHT));
            self.children[last].layout(&mut tree.children[last], renderer, &track);
            tree.children[last].translation =
                Vector::new(width - TRACK_WIDTH, (line - TRACK_HEIGHT) / 2.0);
        }
        tree.size = Size::new(width, height);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if self.enabled && self.on_toggle.is_some() {
            let row = self.row_of(tree, layout);
            let state = tree.state.downcast_mut::<State>();
            operation.focusable(None, row, &mut state.hot);
        }
        operation.container(None, layout.bounds(), viewport);
        operation.traverse(&mut |operation| {
            self.children
                .iter_mut()
                .zip(layout.iter_mut(&mut tree.children))
                .for_each(|(child, (layout, state))| {
                    child.operate(state, layout, viewport, renderer, operation);
                });
        });
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
        for (child, (layout, tree)) in self
            .children
            .iter_mut()
            .zip(layout.iter_mut(&mut tree.children))
        {
            child.update(tree, event, layout, cursor, renderer, shell, viewport);
        }
        let row = self.row_of(tree, layout);
        let live = self.enabled && self.on_toggle.is_some();
        let (on, enabled, motion) = (self.on, self.enabled, self.motion);
        let state = tree.state.downcast_mut::<State>();
        let toggle = |shell: &mut Shell<'_, M>| {
            if let Some(on_toggle) = &self.on_toggle {
                shell.publish(on_toggle(!on));
            }
        };
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hovered = cursor.is_over(row);
                if hovered != state.hot.hovered {
                    state.hot.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if live && cursor.is_over(row) && !shell.is_event_captured() =>
            {
                state.hot.pressed = true;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(_)) if !cursor.is_over(row) => {
                if state.hot.focused {
                    state.hot.focused = false;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.hot.pressed =>
            {
                state.hot.pressed = false;
                if live && cursor.is_over(row) {
                    toggle(shell);
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                if live && state.hot.focused && !shell.is_event_captured() =>
            {
                if matches!(
                    key.as_ref(),
                    keyboard::Key::Named(Named::Space | Named::Enter)
                ) {
                    toggle(shell);
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now))
                if state.advance(*now, on, enabled, motion) =>
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
        style: &Style,
        layout: Layout,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        for (child, (layout, tree)) in self.children.iter().zip(layout.iter(&tree.children)) {
            child.draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
        let state = tree.state.downcast_ref::<State>();
        let palette = theme.palette();
        let bounds = layout.bounds();
        let line = self.row_of(tree, layout).height;
        let track = Rectangle::new(
            Point::new(
                bounds.x + bounds.width - TRACK_WIDTH,
                bounds.y + (line - TRACK_HEIGHT) / 2.0,
            ),
            Size::new(TRACK_WIDTH, TRACK_HEIGHT),
        );
        let amount = |animation: &Animation<bool>| animation.interpolate(0.0f32, 1.0, state.now);
        let (on, able) = (amount(&state.on), amount(&state.enabled));
        let lit = state.hot.amount();
        let off = mix(palette.button_default, palette.button_default_hover, lit);
        let lit_on = mix(palette.accent, palette.accent_hover, lit);
        let face = mix(off, lit_on, on);
        let fade = 0.5 + 0.5 * able;
        fill(renderer, track, TRACK_HEIGHT / 2.0, face.scale_alpha(fade));
        let knob_at = INSET + (TRACK_WIDTH - KNOB - 2.0 * INSET) * on;
        let knob = Rectangle::new(
            Point::new(track.x + knob_at, track.y + (TRACK_HEIGHT - KNOB) / 2.0),
            Size::new(KNOB, KNOB),
        );
        let ink = mix(palette.secondary, palette.on_accent, on);
        fill(renderer, knob, KNOB / 2.0, ink.scale_alpha(fade));
        if state.hot.focused {
            ring(renderer, track, TRACK_HEIGHT / 2.0, RING, palette.text);
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
        let own = if self.enabled
            && self.on_toggle.is_some()
            && cursor.is_over(self.row_of(tree, layout))
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        };
        self.children
            .iter()
            .zip(layout.iter(&tree.children))
            .map(|(child, (layout, tree))| {
                child.mouse_interaction(tree, layout, cursor, viewport, renderer)
            })
            .fold(own, Ord::max)
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
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
            window,
        )
    }
}
