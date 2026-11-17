//! The picker's left edge, which drags it wider (`edge`,
//! `ui/yapfast.slint:2617`-`:2643`): an 8 px strip down the card with a 2 px
//! line in its middle that shows while the pointer is on it or presses it.
//! A press records the width the picker has; each move says the width that
//! leaves the edge under the pointer (the width then, plus how far the
//! pointer went left), unheld, as the pointer leaves the strip with the edge
//! following it; letting go says the width it ended at if it changed.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::{Border, Event, Length, Rectangle, Size};

use anomi_theme::{Renderer, Theme};

/// The strip's width and its line's (`width: 8px`; `x: 3px; width: 2px`).
const STRIP: f32 = 8.0;
const LINE: f32 = 2.0;
const LINE_AT: f32 = 3.0;

#[derive(Default)]
struct State {
    hovered: bool,
    /// The pointer's x and the picker's width when it pressed.
    drag: Option<(f32, f32)>,
    /// The last width published, so a move that leaves it is not.
    sent: f32,
}

pub struct Edge<'a, Message> {
    shown: f32,
    on_drag: Box<dyn Fn(f32) -> Message + 'a>,
    on_end: Box<dyn Fn(f32) -> Message + 'a>,
}

/// The edge of a picker `shown` wide: `on_drag` hears the width the pointer
/// asks for, `on_end` the width a drag that changed it ended at.
pub fn edge<'a, Message>(
    shown: f32,
    on_drag: impl Fn(f32) -> Message + 'a,
    on_end: impl Fn(f32) -> Message + 'a,
) -> Edge<'a, Message> {
    Edge {
        shown,
        on_drag: Box::new(on_drag),
        on_end: Box::new(on_end),
    }
}

impl<Message> widget::Meta for Edge<'_, Message> {}

impl<Message> widget::Widget<Message, Theme, Renderer> for Edge<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(STRIP), Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        tree.size = limits.resolve(Length::Fixed(STRIP), Length::Fill, Size::ZERO);
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
        let state = tree.state.downcast_mut::<State>();
        let over = cursor.is_over(layout.bounds());
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
                if let Some(at) = cursor.position() {
                    state.drag = Some((at.x, self.shown));
                    state.sent = self.shown;
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some((from_x, from)) = state.drag {
                    let wanted = from + (from_x - position.x);
                    if wanted != state.sent {
                        state.sent = wanted;
                        shell.publish((self.on_drag)(wanted));
                    }
                    shell.capture_event();
                } else if state.hovered != over {
                    state.hovered = over;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some((_, from)) = state.drag.take() {
                    if self.shown != from {
                        shell.publish((self.on_end)(self.shown));
                    }
                    state.hovered = over;
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.drag.is_some() || cursor.is_over(layout.bounds()) {
            mouse::Interaction::ResizingHorizontally
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        if !state.hovered && state.drag.is_none() {
            return;
        }
        let palette = theme.palette();
        let colour = if state.drag.is_some() {
            palette.faint
        } else {
            palette.line
        };
        let bounds = layout.bounds();
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: bounds.x + LINE_AT,
                    width: LINE,
                    ..bounds
                },
                border: Border::default(),
                ..renderer::Quad::default()
            },
            colour,
        );
    }
}
