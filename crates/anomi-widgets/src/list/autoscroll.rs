//! Autoscroll: a middle click in a list with more than shows scrolls it
//! towards the pointer, faster the farther the pointer is from where it
//! clicked, as browsers do. Clicking starts a scroll that lasts until the
//! next press, a key, a wheel or the window losing the focus; holding the
//! button and moving away scrolls until it's let go. While it lasts the
//! rows don't see the pointer, and the press that ends it goes no further.
//! A circle at the click shows it, with arrows the ways the list can go.
//! The Slint side: `vendor/i-slint-core/yapfast_autoscroll.rs`;
//! `AutoscrollMarker`, `AutoscrollArrow` in `ui/scrollview.slint:184`, `:195`.

use std::time::Instant;

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::mouse;
use iced::advanced::renderer::{Quad, Renderer as _};
use iced::widget::canvas::{Frame, Path};
use iced::{Border, Event, Point, Rectangle, Shadow, Size, Vector, keyboard, window};

use anomi_theme::{Renderer, Theme, rgba};

/// How far the pointer can be from where it clicked without scrolling.
const STILL: f32 = 10.0;
/// How fast it scrolls just past `STILL`, in pixels a second for each pixel.
const SPEED: f32 = 8.0;
/// How much faster the farther it is: pixels a second for each pixel squared.
const ACCELERATION: f32 = 0.05;
/// The fastest it scrolls, in pixels a second.
const FASTEST: f32 = 12_000.0;
/// The longest step a frame takes, so a late frame doesn't leap.
const LONGEST_FRAME: f32 = 0.05;
/// The marker: a 30 px circle with a 4 px dot and 8 by 5 px arrows 5 px in.
const MARKER: f32 = 30.0;
const DOT: f32 = 4.0;
const ARROW: Size = Size::new(8.0, 5.0);
const ARROW_INSET: f32 = 5.0;
/// The marker's 1 px border, and its shadow: 2 px down, blurred 8 px
/// (`ui/scrollview.slint:202`-`:206`).
const MARKER_BORDER: f32 = 1.0;
const MARKER_SHADOW: Shadow = Shadow {
    color: rgba(0x0000_0060),
    offset: Vector::new(0.0, 2.0),
    blur_radius: 8.0,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Autoscroll {
    origin: Point,
    pointer: Point,
    /// The middle button that started it is still down; moving away with it
    /// down makes letting go end the scroll.
    held: bool,
    dragged: bool,
    /// The button of the press that ended it, until it's let go.
    ending: Option<mouse::Button>,
    last: Option<Instant>,
}

/// What an event did to the autoscroll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// It took the event.
    Took,
    /// It's over; the event goes on as usual.
    Ended,
    /// It's over, and took the event that ended it.
    EndedTaking,
    /// Scroll by this much, towards the end when positive.
    Scroll(f32),
    /// It goes on, and the event goes on as usual.
    Pass,
}

impl Autoscroll {
    pub fn new(origin: Point) -> Self {
        Self {
            origin,
            pointer: origin,
            held: true,
            dragged: false,
            ending: None,
            last: None,
        }
    }

    /// The marker shows until a press ends the scroll.
    pub fn shown(&self) -> bool {
        self.ending.is_none()
    }

    /// It scrolls, so frames must come.
    pub fn moving(&self) -> bool {
        self.shown() && speed(self.pointer.y - self.origin.y) != 0.0
    }

    pub fn update(&mut self, event: &Event) -> Outcome {
        if let Some(button) = self.ending {
            return match event {
                Event::Mouse(mouse::Event::ButtonReleased(released)) if *released == button => {
                    Outcome::EndedTaking
                }
                Event::Mouse(_) => Outcome::Took,
                Event::Window(window::Event::Unfocused)
                | Event::Keyboard(keyboard::Event::KeyPressed { .. }) => Outcome::Ended,
                _ => Outcome::Pass,
            };
        }
        match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                self.pointer = *position;
                if self.held && self.pointer.distance(self.origin) > STILL {
                    self.dragged = true;
                }
                Outcome::Took
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                self.ending = Some(*button);
                Outcome::Took
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => {
                self.held = false;
                if self.dragged {
                    Outcome::EndedTaking
                } else {
                    Outcome::Took
                }
            }
            // Leaving the window and coming back, as past its bottom edge to
            // scroll fast, goes on scrolling.
            Event::Mouse(
                mouse::Event::ButtonReleased(_)
                | mouse::Event::CursorLeft
                | mouse::Event::CursorEntered,
            ) => Outcome::Took,
            Event::Window(window::Event::RedrawRequested(now)) => {
                let elapsed = self.last.map_or(0.0, |last| {
                    now.saturating_duration_since(last)
                        .as_secs_f32()
                        .min(LONGEST_FRAME)
                });
                self.last = self.moving().then_some(*now);
                Outcome::Scroll(speed(self.pointer.y - self.origin.y) * elapsed)
            }
            Event::Window(window::Event::Unfocused)
            | Event::Keyboard(keyboard::Event::KeyPressed { .. })
            | Event::Mouse(_) => Outcome::Ended,
            _ => Outcome::Pass,
        }
    }

    /// The pointer: arrows up and down while it rests, which a vertical
    /// list's way is either way.
    pub fn interaction(&self) -> mouse::Interaction {
        mouse::Interaction::ResizingVertically
    }

    /// The circle at the click, with arrows up and down.
    pub fn draw(&self, renderer: &mut Renderer, theme: &Theme) {
        if !self.shown() {
            return;
        }
        let palette = theme.palette();
        let half = MARKER / 2.0;
        let circle = Rectangle::new(
            self.origin - Vector::new(half, half),
            Size::new(MARKER, MARKER),
        );
        renderer.fill_quad(
            Quad {
                bounds: circle,
                border: Border::default()
                    .rounded(half)
                    .width(MARKER_BORDER)
                    .color(palette.line),
                shadow: MARKER_SHADOW,
                ..Quad::default()
            },
            palette.raised,
        );
        renderer.fill_quad(
            Quad {
                bounds: Rectangle::new(
                    self.origin - Vector::new(DOT / 2.0, DOT / 2.0),
                    Size::new(DOT, DOT),
                ),
                border: Border::default().rounded(DOT / 2.0),
                ..Quad::default()
            },
            palette.secondary,
        );
        let mut frame = Frame::new(renderer, Size::new(MARKER, MARKER));
        let left = (MARKER - ARROW.width) / 2.0;
        let up = Path::new(|path| {
            path.move_to(Point::new(left, ARROW_INSET + ARROW.height));
            path.line_to(Point::new(left + ARROW.width / 2.0, ARROW_INSET));
            path.line_to(Point::new(left + ARROW.width, ARROW_INSET + ARROW.height));
            path.close();
        });
        let bottom = MARKER - ARROW_INSET;
        let down = Path::new(|path| {
            path.move_to(Point::new(left, bottom - ARROW.height));
            path.line_to(Point::new(left + ARROW.width / 2.0, bottom));
            path.line_to(Point::new(left + ARROW.width, bottom - ARROW.height));
            path.close();
        });
        frame.fill(&up, palette.secondary);
        frame.fill(&down, palette.secondary);
        renderer.with_translation(Vector::new(circle.x, circle.y), |renderer| {
            renderer.draw_geometry(frame.into_geometry());
        });
    }
}

/// How fast the pointer `distance` from where it clicked scrolls, in pixels
/// a second.
pub fn speed(distance: f32) -> f32 {
    let past = distance.abs() - STILL;
    if past <= 0.0 {
        return 0.0;
    }
    (past * SPEED + past * past * ACCELERATION)
        .min(FASTEST)
        .copysign(distance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn scrolls_faster_the_farther_the_pointer_is() {
        assert_eq!(speed(5.0), 0.0);
        assert_eq!(speed(-10.0), 0.0);
        assert!(speed(30.0) > 0.0 && speed(-30.0) < 0.0);
        assert!(speed(200.0) > 2.0 * speed(100.0));
        assert_eq!(speed(10_000.0), FASTEST);
    }

    #[test]
    fn a_click_scrolls_until_the_next_press_and_a_drag_until_let_go() {
        let start = Instant::now();
        let moved = |y| {
            Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(0.0, y),
            })
        };
        let frame = |ms| {
            Event::Window(window::Event::RedrawRequested(
                start + Duration::from_millis(ms),
            ))
        };
        let mut scroll = Autoscroll::new(Point::ORIGIN);
        let up = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle));
        assert_eq!(scroll.update(&up), Outcome::Took, "a click: it lasts");
        assert_eq!(scroll.update(&moved(110.0)), Outcome::Took);
        assert_eq!(scroll.update(&frame(0)), Outcome::Scroll(0.0));
        let Outcome::Scroll(step) = scroll.update(&frame(16)) else {
            panic!("scrolls")
        };
        assert!(step > 0.0, "down, towards the pointer");
        let left = Event::Mouse(mouse::Event::CursorLeft);
        let entered = Event::Mouse(mouse::Event::CursorEntered);
        assert_eq!(scroll.update(&left), Outcome::Took, "out of the window");
        assert_eq!(scroll.update(&entered), Outcome::Took, "and back");
        let Outcome::Scroll(step) = scroll.update(&frame(32)) else {
            panic!("scrolls on")
        };
        assert!(step > 0.0, "the scroll goes on");
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        assert_eq!(scroll.update(&press), Outcome::Took);
        assert!(!scroll.shown());
        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        assert_eq!(scroll.update(&release), Outcome::EndedTaking);

        let mut held = Autoscroll::new(Point::ORIGIN);
        held.update(&moved(50.0));
        assert_eq!(
            held.update(&up),
            Outcome::EndedTaking,
            "dragged, then let go"
        );
    }
}
