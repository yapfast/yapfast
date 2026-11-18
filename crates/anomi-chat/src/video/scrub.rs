//! The bar to move through the video (`VideoControls`' position bar,
//! `ui/messages.slint:428`): a 4 px rail, white up to how far it played,
//! across all the room the controls leave it. A click moves there, and so
//! does a drag, at each step. A press on it is its own: it never plays or
//! pauses the video under it.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Background, Border, Event, Length, Point, Rectangle, Size};

use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::video::Intent;

use super::look::{INK, RAIL};

/// The rail's thickness (`:433`).
const THICK: f32 = 4.0;

/// The bar at `progress`, from 0 to 1.
pub fn scrub(progress: f32) -> Scrub {
    Scrub {
        progress: progress.clamp(0.0, 1.0),
    }
}

pub struct Scrub {
    progress: f32,
}

#[derive(Default)]
struct State {
    pressed: bool,
}

/// Where along `bounds` the pointer at `at` is, from 0 to 1.
pub fn fraction(bounds: Rectangle, at: Point) -> f32 {
    if bounds.width <= 0.0 {
        return 0.0;
    }
    ((at.x - bounds.x) / bounds.width).clamp(0.0, 1.0)
}

impl widget::Meta for Scrub {}

impl widget::Widget<Intent, Theme, Renderer> for Scrub {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fill, Length::Fill, Size::new(0.0, THICK));
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Intent>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                state.pressed = true;
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.pressed => {
                shell.publish(Intent::Seek(fraction(bounds, *position)));
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.pressed => {
                state.pressed = false;
                if let Some(at) = cursor.position_over(bounds) {
                    shell.publish(Intent::Seek(fraction(bounds, at)));
                }
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let rail = Rectangle {
            y: bounds.y + (bounds.height - THICK) / 2.0,
            height: THICK,
            ..bounds
        };
        let quad = |bounds| Quad {
            bounds,
            border: Border::default().rounded(THICK / 2.0),
            ..Quad::default()
        };
        renderer.fill_quad(quad(rail), Background::Color(RAIL));
        if self.progress > 0.0 {
            let played = Rectangle {
                width: rail.width * self.progress,
                ..rail
            };
            renderer.fill_quad(quad(played), Background::Color(INK));
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pointer_maps_onto_the_rail_and_stays_on_it() {
        let bounds = Rectangle::new(Point::new(10.0, 0.0), Size::new(100.0, 36.0));
        assert_eq!(fraction(bounds, Point::new(60.0, 5.0)), 0.5);
        assert_eq!(fraction(bounds, Point::new(0.0, 5.0)), 0.0);
        assert_eq!(fraction(bounds, Point::new(200.0, 5.0)), 1.0);
    }
}
