//! The track and handle the three sliders share: `VolumeSlider`'s,
//! `SensitivitySlider`'s and `UprightVolumeSlider`'s (`ui/controls.slint:1526`,
//! `:1818`, `:1737`), drawn and driven alike.
//!
//! A press picks the value under the pointer, takes the keys and starts a
//! drag; the drag reports each step through `on_change` and the release the
//! last through `on_commit`; the arrow keys move a step and commit at once.
//! The widget is controlled: it shows the `value` it is given and asks for
//! the next, so what the app keeps is always what is drawn.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::operation::{Focusable, Operation};
use iced::advanced::widget::{self, Id, Tree, tree};
use iced::advanced::{Shell, mouse};
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key};
use iced::{Border, Color, Event, Length, Point, Rectangle, Size};

use anomi_theme::{Renderer, Theme, metrics};

use super::level;
use super::track::{Axis, Span, Track, pick};

/// What lies under the rail.
#[derive(Clone, Debug, PartialEq)]
pub enum Backdrop {
    /// The plain rail: a control's face with the accent filled to the value.
    Rail,
    /// A microphone's level in dBFS, an 8 px meter the handle runs over.
    Level(level::Reading),
}

/// The mark `VolumeSlider` draws at 100%, 2 by 10 px.
const MARK_WIDTH: f32 = 2.0;
const MARK_HEIGHT: f32 = 10.0;

/// The width of the upright slider's hit area (`UprightVolumeSlider`).
pub const UPRIGHT_WIDTH: f32 = 24.0;

pub struct Dial<'a, Message> {
    axis: Axis,
    span: Span,
    value: i32,
    step: i32,
    page: Option<i32>,
    mark: Option<i32>,
    backdrop: Backdrop,
    id: Option<Id>,
    on_change: Box<dyn Fn(i32) -> Message + 'a>,
    on_commit: Option<Box<dyn Fn(i32) -> Message + 'a>>,
    on_drag: Option<Box<dyn Fn(bool) -> Message + 'a>>,
}

impl<'a, Message> Dial<'a, Message> {
    /// A dial over `span` at `value`, asking for each new value with
    /// `on_change`.
    pub fn new(
        axis: Axis,
        span: Span,
        value: i32,
        on_change: impl Fn(i32) -> Message + 'a,
    ) -> Self {
        Self {
            axis,
            span,
            value: span.clamp(value),
            step: 1,
            page: None,
            mark: None,
            backdrop: Backdrop::Rail,
            id: None,
            on_change: Box::new(on_change),
            on_commit: None,
            on_drag: None,
        }
    }

    /// How far an arrow key moves it.
    pub fn step(mut self, step: i32) -> Self {
        self.step = step.max(1);
        self
    }

    /// How far Page Up and Page Down move it; with none they do nothing.
    pub fn page(mut self, page: i32) -> Self {
        self.page = Some(page);
        self
    }

    /// A value the track marks and a drag snaps to when near it.
    pub fn mark(mut self, mark: Option<i32>) -> Self {
        self.mark = mark;
        self
    }

    pub fn backdrop(mut self, backdrop: Backdrop) -> Self {
        self.backdrop = backdrop;
        self
    }

    /// Asked for when a drag lets go and after a key moves it: the value to
    /// keep, where `on_change` only follows along.
    pub fn on_commit(mut self, on_commit: impl Fn(i32) -> Message + 'a) -> Self {
        self.on_commit = Some(Box::new(on_commit));
        self
    }

    /// Told `true` when a press starts a drag and `false` when its release
    /// ends it (`pressed`): what a popup that stays open while the slider is
    /// held follows. [`upright`](super::upright) returns a `Dial`, so it
    /// chains this too.
    pub fn on_drag(mut self, on_drag: impl Fn(bool) -> Message + 'a) -> Self {
        self.on_drag = Some(Box::new(on_drag));
        self
    }

    /// The id a focus operation finds it by.
    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    fn track(&self, bounds: Rectangle) -> Track {
        Track {
            axis: self.axis,
            bounds,
        }
    }

    fn set(&self, state: &mut State, shell: &mut Shell<'_, Message>, value: i32) {
        let value = self.span.clamp(value);
        state.last = Some(value);
        let base = self.base(state);
        state.asked = Some(value);
        if value != base {
            shell.publish((self.on_change)(value));
        }
    }

    /// The value to compare and step from: the last one asked for in this
    /// batch of events (a rebuild of the view ends it), else the shown one.
    fn base(&self, state: &State) -> i32 {
        state.asked.unwrap_or(self.value)
    }

    fn commit(&self, shell: &mut Shell<'_, Message>, value: i32) {
        if let Some(on_commit) = &self.on_commit {
            shell.publish(on_commit(value));
        }
    }

    /// What a key asks of the value, if it is one of the slider's.
    fn key_step(&self, key: &Key) -> Option<(i32, bool)> {
        let step = self.step;
        let by = match key.as_ref() {
            Key::Named(Named::ArrowDown) => -step,
            Key::Named(Named::ArrowUp) => step,
            Key::Named(Named::ArrowLeft) if self.axis == Axis::Across => -step,
            Key::Named(Named::ArrowRight) if self.axis == Axis::Across => step,
            Key::Named(Named::PageUp) => self.page?,
            Key::Named(Named::PageDown) => -self.page?,
            _ => return None,
        };
        Some((by, self.on_commit.is_some()))
    }
}

#[derive(Debug, Default)]
struct State {
    focused: bool,
    dragging: bool,
    /// The value the last pick asked for, which a release commits though the
    /// app may not have shown it yet.
    last: Option<i32>,
    /// The value last asked for since the view was last rebuilt.
    asked: Option<i32>,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl<Message> widget::Meta for Dial<'_, Message> {}

impl<Message: Clone> widget::Widget<Message, Theme, Renderer> for Dial<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.state.downcast_mut::<State>().asked = None;
    }

    fn size(&self) -> Size<Length> {
        match self.axis {
            Axis::Across => Size::new(Length::Fill, Length::Fixed(metrics::SLIDER_HEIGHT)),
            Axis::Up => Size::new(Length::Fixed(UPRIGHT_WIDTH), Length::Fill),
        }
    }

    fn layout(&mut self, tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) {
        let size = self.size();
        // An upright track with nothing to stretch to is as tall as two
        // insets and a handle.
        let min = Size::new(
            0.0,
            2.0 * super::track::UPRIGHT_INSET + metrics::SLIDER_HANDLE,
        );
        tree.size = limits.resolve(size.width, size.height, min);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout,
        _viewport: &Rectangle,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(self.id.as_ref(), layout.bounds(), state);
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
        let bounds = layout.bounds();
        let track = self.track(bounds);
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(at) = cursor.position_over(bounds) {
                    state.focused = true;
                    state.dragging = true;
                    if let Some(on_drag) = &self.on_drag {
                        shell.publish(on_drag(true));
                    }
                    let value = pick(track, self.span, self.mark, at);
                    self.set(state, shell, value);
                    shell.request_redraw();
                    shell.capture_event();
                } else if state.focused {
                    state.focused = false;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => {
                let value = pick(track, self.span, self.mark, *position);
                self.set(state, shell, value);
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
                if let Some(on_drag) = &self.on_drag {
                    shell.publish(on_drag(false));
                }
                let value = state.last.take().unwrap_or(self.value);
                self.commit(shell, value);
                shell.request_redraw();
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) if state.focused => {
                if let Some((by, commits)) = self.key_step(key) {
                    let value = self.span.clamp(self.base(state) + by);
                    self.set(state, shell, value);
                    if commits {
                        self.commit(shell, value);
                    }
                    state.last = None;
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
        // A held slider keeps its cursor off the track, as Slint's TouchArea.
        if tree.state.downcast_ref::<State>().dragging || cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
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
        let palette = theme.palette();
        let bounds = layout.bounds();
        let track = self.track(bounds);
        let fraction = self.span.fraction(self.value);
        let rail = Border::default().rounded(metrics::SLIDER_RAIL / 2.0);

        match &self.backdrop {
            Backdrop::Rail => {
                let quad = |bounds| Quad {
                    bounds,
                    border: rail,
                    ..Quad::default()
                };
                renderer.fill_quad(quad(track.rail()), palette.button_default);
                renderer.fill_quad(quad(track.filled(fraction)), palette.accent);
            }
            Backdrop::Level(level) => {
                let y = bounds.center_y() - metrics::METER_HEIGHT / 2.0;
                let meter = Rectangle::new(
                    Point::new(bounds.x, y),
                    Size::new(bounds.width, metrics::METER_HEIGHT),
                );
                level::draw(renderer, theme, meter, level.decibels());
            }
        }

        if let Some(mark) = self.mark.filter(|_| self.axis == Axis::Across) {
            let x = track.point(self.span.fraction(mark)).x - MARK_WIDTH / 2.0;
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle::new(
                        Point::new(x, bounds.center_y() - MARK_HEIGHT / 2.0),
                        Size::new(MARK_WIDTH, MARK_HEIGHT),
                    ),
                    ..Quad::default()
                },
                palette.secondary,
            );
        }

        let side = metrics::SLIDER_HANDLE;
        let centre = track.point(fraction);
        let (ring, ring_colour) = if state.focused {
            (metrics::SLIDER_RING, palette.accent)
        } else {
            (0.0, Color::TRANSPARENT)
        };
        renderer.fill_quad(
            Quad {
                bounds: Rectangle::new(
                    Point::new(centre.x - side / 2.0, centre.y - side / 2.0),
                    Size::new(side, side),
                ),
                border: Border {
                    color: ring_colour,
                    width: ring,
                    radius: (side / 2.0).into(),
                },
                ..Quad::default()
            },
            palette.text,
        );
    }
}

/// Gives the keys to the slider with this id, as [`field::focus`] does for a
/// field: a task the app runs.
///
/// [`field::focus`]: crate::field::focus
pub fn focus<T: Send + 'static>(id: impl Into<Id>) -> iced::Task<T> {
    iced::widget::operation::focus(id.into())
}
