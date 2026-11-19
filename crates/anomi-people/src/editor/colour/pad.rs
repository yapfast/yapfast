//! What the popover's pad and hue bar take from the pointer: a press places
//! the colour, a drag keeps placing it (as Slint's `TouchArea` `place()`
//! does, clamped to the area), and a release lets go. The area is drawn
//! beneath by the popover; this draws the knob and sends the change.

use iced::mouse::{self, Button, Cursor};
use iced::widget::canvas::{self, Action, Frame, Geometry, Path, Program, Stroke};
use iced::{Color, Event, Point, Rectangle, Size, window};

use anomi_theme::{Renderer, Theme};
use anomi_vm::people::editor_preview::Change;

/// Which of the two areas this is.
#[derive(Clone, Copy)]
pub enum Kind {
    /// Saturation along it, brightness up it.
    Pad { sat: f32, val: f32 },
    /// Hue along it.
    Bar { hue: f32 },
}

pub struct Pointer<M> {
    pub kind: Kind,
    pub send: fn(Change) -> M,
}

#[derive(Default)]
pub struct Dragging(bool);

impl<M> Pointer<M> {
    fn at(&self, point: Point, size: Size) -> Change {
        let across = (point.x / size.width).clamp(0.0, 1.0);
        match self.kind {
            Kind::Pad { .. } => Change::Pad {
                sat: across,
                val: 1.0 - (point.y / size.height).clamp(0.0, 1.0),
            },
            Kind::Bar { .. } => Change::Hue(across * 360.0),
        }
    }
}

impl<M> Program<M, Theme, Renderer> for Pointer<M> {
    type State = Dragging;

    fn update(
        &self,
        state: &mut Dragging,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<M>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => {
                let point = cursor.position_in(bounds)?;
                state.0 = true;
                Some(Action::publish((self.send)(self.at(point, bounds.size()))).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.0 => {
                let point = Point::new(position.x - bounds.x, position.y - bounds.y);
                Some(Action::publish((self.send)(self.at(point, bounds.size()))).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) if state.0 => {
                state.0 = false;
                Some(Action::capture())
            }
            // Behind another window no release may come: it lets go there.
            Event::Window(window::Event::Unfocused) => {
                state.0 = false;
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Dragging,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let white = Color::WHITE;
        match self.kind {
            // A 12 px ring, 2 px of white, on the colour.
            Kind::Pad { sat, val } => frame.stroke(
                &Path::circle(
                    Point::new(sat * bounds.width, (1.0 - val) * bounds.height),
                    5.0,
                ),
                Stroke::default().with_width(2.0).with_color(white),
            ),
            // An 8 by 16 px knob, 3 px over the bar's top and bottom.
            Kind::Bar { hue } => frame.fill(
                &Path::rounded_rectangle(
                    Point::new(hue / 360.0 * bounds.width - 4.0, bounds.height / 2.0 - 8.0),
                    Size::new(8.0, 16.0),
                    4.0.into(),
                ),
                canvas::Fill::from(white),
            ),
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _: &Dragging,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}
