//! The waveform (`ui/messages.slint`, `FileCard`): a round bar of 3 px every
//! 6 for each point, a quiet one a dot, filled up to where it has played.
//! A press moves there and follows the pointer while held; playing, it
//! pauses while held and plays on once let go.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Border, Event, Length, Rectangle, Size, window};

use anomi_theme::{Renderer, Theme};

pub const BAR: f32 = 3.0;
pub const STEP: f32 = 6.0;
pub const HEIGHT: f32 = 24.0;
/// The pill's height, which a press anywhere across it reaches, as Slint's
/// `TouchArea` over the bars does.
const REACH: f32 = 52.0;

/// The bars' width for `count` points.
pub fn width(count: usize) -> f32 {
    (count as f32 * STEP - BAR).max(0.0)
}

pub struct Wave<'a, Message> {
    levels: &'a [f32],
    played: f32,
    playing: bool,
    enabled: bool,
    on_seek: Box<dyn Fn(f32) -> Message + 'a>,
    on_toggle: Message,
}

pub fn wave<'a, Message>(
    levels: &'a [f32],
    played: f32,
    playing: bool,
    on_seek: impl Fn(f32) -> Message + 'a,
    on_toggle: Message,
) -> Wave<'a, Message> {
    Wave {
        levels,
        played,
        playing,
        enabled: true,
        on_seek: Box::new(on_seek),
        on_toggle,
    }
}

impl<Message> Wave<'_, Message> {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[derive(Debug, Default)]
struct State {
    held: bool,
    resume: bool,
}

fn fraction(bounds: Rectangle, x: f32) -> f32 {
    if bounds.width <= 0.0 {
        0.0
    } else {
        ((x - bounds.x) / bounds.width).clamp(0.0, 1.0)
    }
}

impl<Message> widget::Meta for Wave<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for Wave<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(
            Length::Fixed(width(self.levels.len())),
            Length::Fixed(REACH),
        )
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let size = self.size();
        tree.size = limits.resolve(size.width, size.height, Size::ZERO);
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
        if !self.enabled {
            return;
        }
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(at) = cursor.position_over(bounds) {
                    state.held = true;
                    state.resume = self.playing;
                    if state.resume {
                        shell.publish(self.on_toggle.clone());
                    }
                    shell.publish((self.on_seek)(fraction(bounds, at.x)));
                    shell.capture_event();
                }
            }
            // The cursor, not the event's position: a list it scrolls in
            // moves the one and not the other.
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.held => {
                if let Some(at) = cursor.position() {
                    shell.publish((self.on_seek)(fraction(bounds, at.x)));
                }
                shell.capture_event();
            }
            // Behind another window no release may come: it lets go there,
            // and the window's own event goes on to the rest.
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Window(window::Event::Unfocused)
                if state.held =>
            {
                state.held = false;
                if std::mem::take(&mut state.resume) {
                    shell.publish(self.on_toggle.clone());
                }
                if let Event::Mouse(_) = event {
                    shell.capture_event();
                }
            }
            _ => {}
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
        if self.enabled && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let palette = theme.palette();
        let bounds = layout.bounds();
        let count = self.levels.len() as f32;
        for (index, level) in self.levels.iter().enumerate() {
            let height = (level.clamp(0.0, 1.0) * HEIGHT).max(BAR);
            let done = (index as f32 + 0.5) / count <= self.played;
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: bounds.x + index as f32 * STEP,
                        y: bounds.y + (bounds.height - height) / 2.0,
                        width: BAR,
                        height,
                    },
                    border: Border::default().rounded(BAR / 2.0),
                    ..Quad::default()
                },
                if done { palette.text } else { palette.muted },
            );
        }
    }
}
