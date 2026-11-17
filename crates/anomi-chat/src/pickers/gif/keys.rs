//! The GIF picker's keys and Shift (the `FocusScope` of `GifPicker`,
//! `ui/gifs.slint:111`-`:128`): Escape goes back to the home while another
//! page shows, Ctrl+G closes the picker, and Shift's coming and going is
//! told, so a click can put a GIF in the message. A leaf with no size,
//! placed over the picker, which hears events whoever has the focus.
//! Escape on the home is the layer's to close.

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::keyboard::{self, Key, key};
use iced::{Event, Length, Rectangle, Size};

use anomi_theme::{Renderer, Theme};

#[derive(Default)]
struct State {
    shift: bool,
}

pub struct Keys<Message> {
    /// Escape means Back here, not close.
    back: Option<Message>,
    close: Message,
    shift: fn(bool) -> Message,
}

/// Listens for the picker's keys. `back` is what Escape sends when it's
/// away from the home, `close` what Ctrl+G sends, `shift` the change of
/// Shift.
pub fn keys<Message>(
    back: Option<Message>,
    close: Message,
    shift: fn(bool) -> Message,
) -> Keys<Message> {
    Keys { back, close, shift }
}

impl<Message> widget::Meta for Keys<Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for Keys<Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(0.0), Length::Fixed(0.0))
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fixed(0.0), Length::Fixed(0.0), Size::ZERO);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let Event::Keyboard(event) = event else {
            return;
        };
        match event {
            keyboard::Event::ModifiersChanged(modifiers) => {
                if modifiers.shift() != state.shift {
                    state.shift = modifiers.shift();
                    shell.publish((self.shift)(state.shift));
                }
            }
            keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                ..
            } => {
                if let (Key::Named(key::Named::Escape), Some(back)) = (key, &self.back) {
                    shell.publish(back.clone());
                    shell.capture_event();
                } else if modifiers.control()
                    // By the key's Latin place, so Ctrl+G closes on any
                    // layout, as the composer opens the picker.
                    && key
                        .to_latin(*physical_key)
                        .map(|letter| letter.to_ascii_lowercase())
                        == Some('g')
                {
                    shell.publish(self.close.clone());
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        _layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
    }
}
