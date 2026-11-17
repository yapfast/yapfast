//! A face as an iced widget: its size, layout, events and drawing.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::keyboard::{self, key::Named};
use iced::{Event, Length, Rectangle, Size, window};

use anomi_theme::{Renderer, Theme};

use super::face::{CAPTION_HEIGHT, CAPTION_SIZE, Face};
use super::paint;
use super::state::State;
use anomi_theme::typography;

impl<Message> widget::Meta for Face<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for Face<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::new(self.open, self.enabled))
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width_length(), Length::Fixed(self.total_height()))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let state = tree.state.downcast_mut::<State>();
        let length = self.width_length();
        let height = self.total_height();
        let max = limits.max.width;

        // The text gets what the shape has left of the room it is offered, so
        // it elides at its end when the shape is narrower than its content.
        let offered = match length {
            Length::Fixed(width) => width.min(max),
            _ => max,
        };
        let room = (offered - self.chrome()).max(0.0);
        if !self.text.is_empty() {
            let bounds = Size::new(room, self.min.height);
            state
                .text
                .update(paint::text(self.text, self.text_size, self.font, bounds));
        }
        let mut wide = if self.fixed {
            self.min.width
        } else {
            self.min
                .width
                .max(self.content_width(self.text_width(state)) + 2.0 * self.padding)
        };
        if let Some(caption) = self.caption {
            let bounds = Size::new(max, CAPTION_HEIGHT);
            state
                .caption
                .update(paint::text(caption, CAPTION_SIZE, typography::UI, bounds));
            wide = wide.max(state.caption.min_width().ceil());
        }
        tree.size = limits.resolve(length, Length::Fixed(height), Size::new(wide, height));
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        if self.live() && self.on_press.is_some() {
            let state = tree.state.downcast_mut::<State>();
            operation.focusable(self.id.as_ref(), layout.bounds(), state);
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let shape = self.shape(layout.bounds());
        let state = tree.state.downcast_mut::<State>();
        let live = self.live();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hovered = live && cursor.is_over(shape);
                if hovered != state.hovered {
                    state.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                if !cursor.is_over(shape) {
                    // A click elsewhere takes the focus away, as a text
                    // field's does.
                    if state.focused {
                        state.focused = false;
                        shell.request_redraw();
                    }
                } else if live && *button == mouse::Button::Left {
                    state.pressed = true;
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.pressed => {
                state.pressed = false;
                shell.request_redraw();
                if live
                    && cursor.is_over(shape)
                    && let Some(message) = &self.on_press
                {
                    shell.publish(message.clone());
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                if live && state.focused && !shell.is_event_captured() =>
            {
                if matches!(
                    key.as_ref(),
                    keyboard::Key::Named(Named::Space | Named::Enter)
                ) {
                    if let Some(message) = &self.on_press {
                        shell.publish(message.clone());
                    }
                    shell.capture_event();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if !live && (state.hovered || state.pressed) {
                    state.hovered = false;
                    state.pressed = false;
                }
                if state.advance(*now, self.open, self.enabled, self.motion) {
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
        _style: &iced::advanced::renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        self.paint(state, renderer, theme, layout.bounds(), viewport);
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.live() && cursor.is_over(self.shape(layout.bounds())) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}
