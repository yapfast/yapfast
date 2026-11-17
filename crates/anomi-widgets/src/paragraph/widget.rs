//! The paragraph as a widget of its own: its state in the tree, the fold's
//! toggle as its one child, the link under the pointer found on each move,
//! and its server's emoji asking for frames only while they play.

use std::time::Instant;

use iced::advanced::Renderer as _;
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Event, Length, Point, Rectangle, Size, Vector, window};

use anomi_theme::{Renderer, Theme};
use anomi_vm::text::Look as SpanLook;

use super::draw::{Colors, Painter};
use super::select::Operand;
use super::shape::Inks;
use super::state::{Look, State, natural_width};
use super::{LINE, Paragraph, TOGGLE_GAP};

impl<Message> widget::Meta for Paragraph<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Paragraph<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(self.toggle.as_mut_slice());
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) {
        let limits = limits.width(self.width);
        let max = limits.bounds().width;
        let look = Look {
            size: self.text_size(),
            framed: self.framed,
            max_lines: self.max_lines,
            hint_factor: renderer.hint_factor(),
        };
        let state = tree.state.downcast_mut::<State>();
        state.lay_out(self.text, look, max);
        let natural = natural_width(&state.text, &state.shapes.get_mut().blocks);
        let width = match self.width {
            Length::Shrink => natural.min(max),
            _ if max.is_finite() => max,
            _ => natural,
        };
        state.arrange(width);
        let line = self.size * LINE;
        state.long = self
            .fold
            .is_some_and(|fold| state.full > line * f32::from(fold.beyond));
        state.shown = match self.fold {
            Some(fold) if state.long && !fold.open => {
                (line * f32::from(fold.lines)).min(state.full)
            }
            _ => state.full,
        };
        let (long, mut height) = (state.long, state.shown);
        if let (true, Some(toggle), Some(child)) =
            (long, self.toggle.as_mut(), tree.children.first_mut())
        {
            toggle.layout(
                child,
                renderer,
                &Limits::new(Size::ZERO, Size::new(width, f32::INFINITY)),
            );
            child.translation = Vector::new(0.0, height + TOGGLE_GAP);
            height += TOGGLE_GAP + child.size.height;
        }
        tree.size = Size::new(width, height);
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
        let long = tree.state.downcast_ref::<State>().long;
        if let (true, Some(toggle)) = (long, self.toggle.as_mut())
            && let Some((child_layout, child)) = layout.iter_mut(&mut tree.children).next()
        {
            toggle.update(
                child,
                event,
                child_layout,
                cursor,
                renderer,
                shell,
                viewport,
            );
            if shell.is_event_captured() {
                return;
            }
        }
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
            | Event::Window(window::Event::RedrawRequested(_)) => {
                let at = cursor.position_in(bounds);
                let hovered = at
                    .and_then(|at| state.link_at(at))
                    .filter(|_| self.on_link.is_some() || self.on_right_press.is_some());
                state.over_text = self.selectable
                    && at.is_some_and(|at| at.y <= state.shown && state.glyph_at(at));
                if hovered != state.hovered {
                    state.hovered = hovered;
                    if !matches!(event, Event::Window(_)) {
                        shell.request_redraw();
                    }
                }
                if let Event::Window(window::Event::RedrawRequested(now)) = event
                    && let Some(due) = self.play(state, bounds, viewport, *now)
                {
                    shell.request_redraw_at(due);
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(hovered) = state.hovered.filter(|_| self.on_link.is_some()) {
                    state.pressed = Some(hovered);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let pressed = state.pressed.take();
                if let (Some(pressed), Some(on_link)) = (pressed, &self.on_link)
                    && state.hovered == Some(pressed)
                    && let Some(link) = state.text.link(pressed.link)
                {
                    shell.publish(on_link(link.clone()));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let (Some(hovered), Some(on_right)) = (state.hovered, &self.on_right_press)
                    && let Some(link) = state.text.link(hovered.link)
                {
                    shell.publish(on_right(link.clone()));
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
        let inks = Inks::of(palette);
        state.reink(inks);
        inks.drawn();
        let text = (self.color)(palette);
        Painter {
            state,
            text: &state.text,
            theme,
            colors: Colors {
                text,
                heading: self
                    .heading
                    .as_ref()
                    .map_or(text, |heading| heading(palette)),
            },
            origin: layout.position() - Point::ORIGIN,
            viewport,
            scale: crate::picture::scale(),
        }
        .draw(renderer);
        if let (true, Some(toggle)) = (state.long, self.toggle.as_ref())
            && let Some((child_layout, child)) = layout.iter(&tree.children).next()
        {
            toggle.draw(
                child,
                renderer,
                theme,
                style,
                child_layout,
                cursor,
                viewport,
            );
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
        let state = tree.state.downcast_ref::<State>();
        if let (true, Some(toggle)) = (state.long, self.toggle.as_ref())
            && let Some((child_layout, child)) = layout.iter(&tree.children).next()
            && cursor.is_over(child_layout.bounds())
        {
            return toggle.mouse_interaction(child, child_layout, cursor, viewport, renderer);
        }
        if state.hovered.is_some() && self.on_link.is_some() {
            mouse::Interaction::Pointer
        } else if state.over_text && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        viewport: &Rectangle,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let (width, long) = (state.width, state.long);
        let State { text, shapes, .. } = state;
        let position = layout.position() - Point::ORIGIN;
        for ((_, spans), shaped) in text.blocks().zip(shapes.get_mut().blocks.iter_mut()) {
            let origin = position + (shaped.origin - Point::ORIGIN);
            let bounds = Rectangle::new(Point::ORIGIN + origin, Size::new(width, shaped.shown));
            operation.text(
                None,
                bounds,
                &mut Operand {
                    shaped,
                    text,
                    spans,
                    origin,
                    selectable: self.selectable,
                },
            );
        }
        if let (true, Some(toggle)) = (long, self.toggle.as_mut())
            && let Some((child_layout, child)) = layout.iter_mut(&mut tree.children).next()
        {
            toggle.operate(child, child_layout, viewport, renderer, operation);
        }
    }
}

impl<Message> Paragraph<'_, Message> {
    /// Moves each server's emoji that shows to its frame due, and says when
    /// the next is, while `motion` plays: none behind another window.
    pub(super) fn play(
        &self,
        state: &mut State,
        bounds: Rectangle,
        viewport: &Rectangle,
        now: Instant,
    ) -> Option<Instant> {
        if state.text.pictures().is_empty() {
            return None;
        }
        let shown = Rectangle::new(bounds.position(), Size::new(bounds.width, state.shown));
        let viewport = viewport.intersection(&shown)?;
        let origin = bounds.position() - Point::ORIGIN;
        let State {
            text,
            shapes,
            players,
            ..
        } = state;
        let shapes = shapes.get_mut();
        let mut due: Option<Instant> = None;
        for ((_, spans), shaped) in text.blocks().zip(&shapes.blocks) {
            let at = origin + (shaped.origin - Point::ORIGIN);
            for run in &shaped.runs {
                let Some(SpanLook::Picture(index)) =
                    spans.get(run.span as usize).map(|span| span.look)
                else {
                    continue;
                };
                if let (Some(player), Some(picture)) =
                    (players.get_mut(index as usize), text.picture(index))
                {
                    let next = player.update(
                        &picture.still,
                        run.picture() + at,
                        &viewport,
                        self.motion,
                        now,
                    );
                    due = due.into_iter().chain(next).min();
                }
            }
        }
        due
    }
}
