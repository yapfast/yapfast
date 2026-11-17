//! A line of an icon and a word that the pointer and the keys act on: the
//! body of a section's row (`ChannelRow`, `ui/rail.slint:397`) and of a
//! [`link`](super::link) (`LinkButton`, `ui/settings.slint:259`), and of the
//! small buttons of the frame. It draws its own fill, ring, icon and text,
//! so the colours that follow the pointer fade in one place over `QUICK`.
//!
//! Tab reaches it, Space or Enter presses it, and the ring shows while it
//! has the focus; a click doesn't focus it, as in the Slint app. With a
//! still [`Motion`] it lands at once and asks for no frame.

use std::borrow::Cow;

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Style;
use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{
    Alignment, Ellipsis, LineHeight, Renderer as _, Shaping, Text, Wrapping,
};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::keyboard::{self, key::Named};
use iced::{Color, Event, Font, Length, Pixels, Point, Rectangle, Size, alignment, window};

use anomi_theme::{Motion, Renderer, Theme, typography};
use anomi_vm::icon::Icon;

use super::press::{Hot, RING, fill, glyph, ring};

/// A leaf's defaults: the gap to its glyph, its line and its corners
/// (`ui/settings.slint:263-281`).
const GAP: f32 = 8.0;
const LINE: f32 = 36.0;
const RADIUS: f32 = 6.0;

/// The colours of one moment.
#[derive(Clone, Copy, Debug)]
pub struct Ink {
    pub fill: Color,
    pub text: Color,
    pub glyph: Color,
}

/// A look: its colours for how lit the leaf is (0 at rest, 1 under the pointer).
pub type Look<'a> = Box<dyn Fn(&Theme, f32) -> Ink + 'a>;

type Line = Plain<<Renderer as iced::advanced::text::Renderer>::Paragraph>;

pub struct Leaf<'a, Message> {
    pub icon: Option<Icon>,
    pub glyph: f32,
    /// The icon follows the text instead of leading it.
    pub trailing: bool,
    pub text: Cow<'a, str>,
    pub size: f32,
    pub font: Font,
    pub gap: f32,
    /// Between the edge and the content.
    pub pad: f32,
    /// Room kept beyond `pad` at the right end, for something laid over it.
    pub tail: f32,
    pub height: f32,
    pub stretch: bool,
    pub radius: f32,
    /// The ring's reach beyond the leaf's own edge, each side.
    pub outset: f32,
    pub look: Look<'a>,
    pub on_press: Option<Message>,
    pub enabled: bool,
    pub motion: Motion,
}

struct State {
    hot: Hot,
    line: Line,
}

impl<'a, Message> Leaf<'a, Message> {
    pub fn new(
        text: impl Into<Cow<'a, str>>,
        motion: Motion,
        look: impl Fn(&Theme, f32) -> Ink + 'a,
    ) -> Self {
        Self {
            icon: None,
            glyph: 0.0,
            trailing: false,
            text: text.into(),
            size: typography::size::BODY,
            font: typography::MEDIUM,
            gap: GAP,
            pad: 0.0,
            tail: 0.0,
            height: LINE,
            stretch: false,
            radius: RADIUS,
            outset: 0.0,
            look: Box::new(look),
            on_press: None,
            enabled: true,
            motion,
        }
    }

    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn live(&self) -> bool {
        self.enabled && self.on_press.is_some()
    }

    fn icons(&self) -> f32 {
        self.icon.map_or(0.0, |_| self.glyph)
    }

    /// What the icon and the gap after it take of the width.
    fn chrome(&self) -> f32 {
        2.0 * self.pad
            + self.tail
            + self.icons()
            + if self.icon.is_some() && !self.text.is_empty() {
                self.gap
            } else {
                0.0
            }
    }
}

fn shape(content: &str, size: f32, font: Font, bounds: Size) -> Text<&str> {
    Text {
        content,
        bounds,
        size: Pixels(size),
        line_height: LineHeight::default(),
        font,
        align_x: Alignment::Left,
        align_y: alignment::Vertical::Center,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
        ellipsis: Ellipsis::End,
        hint_factor: None,
    }
}

impl<M> widget::Meta for Leaf<'_, M> {}

impl<M: Clone> widget::Widget<M, Theme, Renderer> for Leaf<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            hot: Hot::new(),
            line: Line::default(),
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(
            if self.stretch {
                Length::Fill
            } else {
                Length::Shrink
            },
            Length::Fixed(self.height),
        )
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let state = tree.state.downcast_mut::<State>();
        let room = (limits.max.width - self.chrome()).max(0.0);
        state.line.update(shape(
            &self.text,
            self.size,
            self.font,
            Size::new(room, self.height),
        ));
        let wide = self.chrome() + state.line.min_width().ceil();
        let width = if self.stretch {
            Length::Fill
        } else {
            Length::Shrink
        };
        tree.size = limits.resolve(
            width,
            Length::Fixed(self.height),
            Size::new(wide, self.height),
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        // The words are named, so a test (or a reader) finds the part by them.
        operation.text(None, layout.bounds(), &mut super::press::Words(&self.text));
        if self.live() {
            let state = tree.state.downcast_mut::<State>();
            operation.focusable(None, layout.bounds(), &mut state.hot);
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let live = self.live();
        let hot = &mut tree.state.downcast_mut::<State>().hot;
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let hovered = live && cursor.is_over(bounds);
                if hovered != hot.hovered {
                    hot.hovered = hovered;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                if !cursor.is_over(bounds) {
                    if hot.focused {
                        hot.focused = false;
                        shell.request_redraw();
                    }
                } else if live && *button == mouse::Button::Left {
                    hot.pressed = true;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if hot.pressed => {
                hot.pressed = false;
                if live
                    && cursor.is_over(bounds)
                    && let Some(message) = &self.on_press
                {
                    shell.publish(message.clone());
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                if live && hot.focused && !shell.is_event_captured() =>
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
                if !live && hot.hovered {
                    hot.hovered = false;
                }
                if hot.advance(*now, hot.hovered, self.motion) {
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
        _style: &Style,
        layout: Layout,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let ink = (self.look)(theme, state.hot.amount());
        fill(renderer, bounds, self.radius, ink.fill);
        if state.hot.focused {
            let outer = Rectangle {
                x: bounds.x - self.outset,
                width: bounds.width + 2.0 * self.outset,
                ..bounds
            };
            ring(renderer, outer, self.radius, RING, theme.palette().accent);
        }
        let middle = bounds.y + (bounds.height - self.glyph) / 2.0;
        let mut x = bounds.x + self.pad;
        let place = |renderer: &mut Renderer, icon: Icon, x: f32| {
            glyph(
                renderer,
                icon,
                Point::new(x, middle),
                self.glyph,
                ink.glyph,
                viewport,
            );
        };
        if let (Some(icon), false) = (self.icon, self.trailing) {
            place(renderer, icon, x);
            x += self.glyph + self.gap;
        }
        let room = (bounds.width - self.chrome()).max(0.0);
        let wide = state.line.min_width().min(room);
        let area = Rectangle::new(Point::new(x, bounds.y), Size::new(wide, bounds.height));
        let at = area.anchor(
            state.line.min_bounds(),
            Alignment::Left,
            alignment::Vertical::Center,
        );
        renderer.fill_paragraph(state.line.raw(), at, ink.text, area);
        if let (Some(icon), true) = (self.icon, self.trailing) {
            place(renderer, icon, x + wide + self.gap);
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
        if self.live() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}
