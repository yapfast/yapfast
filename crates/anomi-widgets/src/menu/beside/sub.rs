//! The submenu beside a row, as an overlay: its card drawn as
//! `ContextMenu`'s are, the pointer lighting its rows, a click sending a
//! row's intent, the wheel scrolling it.

use iced::advanced::overlay::{self, Overlay};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::Operation;
use iced::advanced::{Shell, mouse};
use iced::{Event, Point, Rectangle, touch, window};

use anomi_theme::{Renderer, Theme};
use anomi_vm::menu::Row;

use super::super::row::{Look, Paint};
use super::State;

struct Sub<'a, 'b, M> {
    rows: &'a [Row<M>],
    state: &'b mut State,
    lit: Option<usize>,
}

/// The submenu of `rows`, placed in `state`; the keys light `lit`.
pub(super) fn overlay<'b, M: Clone + 'b>(
    rows: &'b [Row<M>],
    state: &'b mut State,
    lit: Option<usize>,
) -> overlay::Element<'b, M, Theme, Renderer> {
    overlay::Element::new(Box::new(Sub { rows, state, lit }))
}

impl<M> Sub<'_, '_, M> {
    fn hit(&self, point: Option<Point>) -> Option<usize> {
        point.and_then(|point| self.state.panel.hit(point))
    }

    fn over(&self, point: Option<Point>) -> bool {
        point.is_some_and(|point| self.state.panel.bounds.contains(point))
    }
}

impl<M: Clone> Overlay<M, Theme, Renderer> for Sub<'_, '_, M> {
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        _cursor: mouse::Cursor,
    ) {
        let state = &*self.state;
        let alpha = state.fade.interpolate(0.0, 1.0, state.now);
        let lit = state.pointer.or(self.lit);
        let rows = self.rows;
        renderer.with_layer(Rectangle::with_size(state.window), |renderer| {
            state
                .panel
                .draw(renderer, theme, rows, false, alpha, |index| Paint {
                    lit: lit == Some(index),
                    arrow: false,
                    turned: 0.0,
                    ticked: if rows[index].checked { 1.0 } else { 0.0 },
                    focused: false,
                    alpha,
                    look: Look::Menu,
                });
        });
    }

    fn update(
        &mut self,
        event: &Event,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, M>,
    ) {
        let point = cursor.position();
        let over = self.over(point);
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
            | Event::Touch(touch::Event::FingerMoved { .. }) => {
                let hit = self.hit(point);
                if hit != self.state.pointer {
                    self.state.pointer = hit;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) if over => {
                if *button == mouse::Button::Left {
                    self.state.pressed = self.hit(point);
                }
                shell.capture_event();
            }
            Event::Touch(touch::Event::FingerPressed { .. }) if over => {
                self.state.pressed = self.hit(point);
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. }) => {
                let pressed = self.state.pressed.take();
                let hit = self.hit(point);
                let chosen = hit
                    .filter(|_| pressed == hit)
                    .and_then(|at| self.rows.get(at))
                    .filter(|row| !row.disabled)
                    .and_then(|row| row.intent.clone());
                if let Some(intent) = chosen {
                    shell.publish(intent);
                }
                if over {
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if over => {
                if self.state.panel.wheel(*delta) {
                    self.state.pointer = self.hit(point);
                    shell.request_redraw();
                }
                shell.capture_event();
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                self.state.now = *now;
                if self.state.fade.is_animating(*now) {
                    shell.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(&self, cursor: mouse::Cursor, _renderer: &Renderer) -> mouse::Interaction {
        let point = cursor.position();
        match self.hit(point).and_then(|at| self.rows.get(at)) {
            Some(row) if !row.disabled => mouse::Interaction::Pointer,
            _ if self.over(point) => mouse::Interaction::Idle,
            _ => mouse::Interaction::None,
        }
    }

    fn operate(&mut self, _renderer: &Renderer, operation: &mut dyn Operation) {
        // Each shown row's text, so a test clicks a row by what it says.
        let panel = &self.state.panel;
        for (index, row) in self.rows.iter().enumerate().take(panel.len()) {
            if let Some(shown) = panel.row(index).intersection(&panel.bounds) {
                operation.text(None, shown, &mut row.text.clone());
            }
        }
    }
}
