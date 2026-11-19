//! An edge that resizes a panel: the Slint window's `TouchArea`s over the
//! sidebar's right edge, the thread beside's and the member list's left
//! edge (`ui/yapfast.slint:4497`, `:4517`, `:4538`) and a DM call's lower
//! edge (`:3489`). A strip 8 px wide across the boundary (6 px inside a
//! call's foot) takes the pointer with a resize cursor; under the pointer
//! a 2 px line shows in it, `Theme.line`, `Theme.faint` while held, fading
//! in and out over `QUICK`. Dragging asks [`Intent::Resize`] with the
//! panel's size kept within its bounds, letting go after a drag (or the
//! window going behind another during it) [`Intent::Resized`] (a click
//! that drags nothing asks nothing, so a size left to the style stays so),
//! a double click [`Intent::ResetSize`] (and its release nothing more).
//!
//! The widget fills what it's stacked over and is told where its strip is
//! in it ([`Strip`]), and the panel's size now, or that the panel is what
//! it's stacked over.

use std::time::{Duration, Instant};

use iced::advanced::Shell;
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse::{self, Click, click};
use iced::advanced::renderer::{self, Quad, Renderer as _};
use iced::advanced::widget::{self, Tree, tree};
use iced::{Event, Length, Point, Rectangle, Size, Vector, window};

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, motion};
use anomi_vm::shell::{Intent, Panel};

/// The strip across a boundary, and the line in it (`x: 3px; width: 2px`).
pub const STRIP: f32 = 8.0;
const LINE_INSET: f32 = 3.0;
const LINE: f32 = 2.0;

/// Which way the pointer resizes the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grows {
    /// Dragging right widens it: the sidebar.
    Right,
    /// Dragging left widens it: a panel docked at the right.
    Left,
    /// Dragging down makes it taller: a call above the messages.
    Down,
}

/// Where the strip is in what the edge is stacked over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Strip {
    /// Across a boundary, at this rectangle; the panel is `size` now.
    At(Rectangle, f32),
    /// The last pixels of what it's stacked over, which is the panel: a
    /// call's foot (`y: parent.height - 6px`).
    Foot(f32),
}

pub struct Edge {
    panel: Panel,
    grows: Grows,
    strip: Strip,
    bounds: (f32, f32),
    motion: Motion,
}

/// An edge of `panel` with its strip where `strip` says, kept within
/// `bounds` as it drags, asking with the window's intent.
pub fn edge(
    panel: Panel,
    grows: Grows,
    strip: Strip,
    bounds: (f32, f32),
    motion: Motion,
) -> impl View<anomi_vm::Intent> {
    Edge {
        panel,
        grows,
        strip,
        bounds,
        motion,
    }
    .map(anomi_vm::Intent::from)
}

#[derive(Default)]
struct State {
    hovered: bool,
    /// Where the pointer went down along the edge's axis, and the size then.
    held: Option<(f32, f32)>,
    /// The last size asked for while held, if any was.
    asked: Option<f32>,
    /// A double click reset the size: its release asks nothing.
    reset: bool,
    last_click: Option<Click>,
    line: Fade,
    now: Option<Instant>,
}

impl Edge {
    /// The strip in the window, and the panel's size now.
    fn strip(&self, layout: Layout) -> (Rectangle, f32) {
        let bounds = layout.bounds();
        match self.strip {
            Strip::At(strip, size) => (strip + offset(layout), size),
            Strip::Foot(height) => {
                let strip = Rectangle {
                    y: bounds.y + bounds.height - height,
                    height,
                    ..bounds
                };
                let size = match self.grows {
                    Grows::Right | Grows::Left => bounds.width,
                    Grows::Down => bounds.height,
                };
                (strip, size)
            }
        }
    }

    fn along(&self, point: Point) -> f32 {
        match self.grows {
            Grows::Right | Grows::Left => point.x,
            Grows::Down => point.y,
        }
    }

    /// The size the pointer at `at` asks for, held from `from` at `size`.
    fn dragged(&self, from: f32, size: f32, at: f32) -> f32 {
        let moved = at - from;
        let wanted = match self.grows {
            Grows::Right | Grows::Down => size + moved,
            Grows::Left => size - moved,
        };
        wanted.min(self.bounds.1).max(self.bounds.0)
    }

    fn interaction(&self) -> mouse::Interaction {
        match self.grows {
            Grows::Right | Grows::Left => mouse::Interaction::ResizingHorizontally,
            Grows::Down => mouse::Interaction::ResizingVertically,
        }
    }
}

impl widget::Meta for Edge {}

impl widget::Widget<Intent, Theme, Renderer> for Edge {
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
        tree.size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
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
        let (strip, size) = self.strip(layout);
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = Some(*now);
                if state.line.moving(*now) {
                    shell.request_redraw();
                }
                return;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some((from, size)), Some(at)) = (state.held, cursor.position()) {
                    let wanted = self.dragged(from, size, self.along(at));
                    if wanted != state.asked.unwrap_or(size) {
                        state.asked = Some(wanted);
                        shell.publish(Intent::Resize(self.panel, wanted));
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(at) = cursor.position_over(strip) {
                    let click = Click::new(at, mouse::Button::Left, state.last_click);
                    state.last_click = Some(click);
                    if click.kind() == click::Kind::Double {
                        state.reset = true;
                        state.held = None;
                        shell.publish(Intent::ResetSize(self.panel));
                    } else {
                        state.reset = false;
                        state.held = Some((self.along(at), size));
                        state.asked = None;
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.held.take().is_some() {
                    // Only a drag chooses a size (`members-resized(members-width
                    // == 0px ? 0px : ...)`, `call-resized`, `:4550`, `:3499`).
                    if let Some(asked) = state.asked.take() {
                        shell.publish(Intent::Resized(self.panel, asked));
                    }
                    shell.capture_event();
                } else if std::mem::take(&mut state.reset) {
                    shell.capture_event();
                }
            }
            // Behind another window no release may come: a drag lets go
            // where it is, as its release would.
            Event::Window(window::Event::Unfocused) => {
                if state.held.take().is_some()
                    && let Some(asked) = state.asked.take()
                {
                    shell.publish(Intent::Resized(self.panel, asked));
                }
                state.reset = false;
            }
            _ => {}
        }
        let hovered = cursor.is_over(strip);
        let lit = hovered || state.held.is_some();
        if hovered != state.hovered || lit != state.line.lit() {
            state.hovered = hovered;
            let now = Instant::now();
            state.line.go(lit, self.motion.duration(motion::QUICK), now);
            state.now = Some(now);
            shell.request_redraw();
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
        let (strip, _) = self.strip(layout);
        if state.held.is_some() || cursor.is_over(strip) {
            self.interaction()
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
        let shown = state.line.amount(state.now.unwrap_or_else(Instant::now));
        if shown <= 0.0 {
            return;
        }
        let (strip, _) = self.strip(layout);
        let bounds = match self.grows {
            Grows::Right | Grows::Left => Rectangle {
                x: strip.x + LINE_INSET,
                width: LINE,
                ..strip
            },
            Grows::Down => Rectangle {
                y: strip.y + LINE_INSET,
                height: LINE,
                ..strip
            },
        };
        let palette = theme.palette();
        let colour = if state.held.is_some() {
            palette.faint
        } else {
            palette.line
        };
        renderer.fill_quad(
            Quad {
                bounds,
                ..Quad::default()
            },
            colour.scale_alpha(shown),
        );
    }
}

/// Where the widget is in the window.
fn offset(layout: Layout) -> Vector {
    let position = layout.position();
    Vector::new(position.x, position.y)
}

/// The line's opacity, from 0 to 1, moving linearly over its duration as
/// Slint's `animate opacity { duration: Theme.quick; }` does.
#[derive(Clone, Copy, Debug, Default)]
struct Fade {
    from: f32,
    to: f32,
    start: Option<Instant>,
    duration: Duration,
}

impl Fade {
    fn lit(&self) -> bool {
        self.to > 0.0
    }

    fn go(&mut self, lit: bool, duration: Duration, now: Instant) {
        self.from = self.amount(now);
        self.to = if lit { 1.0 } else { 0.0 };
        self.start = Some(now);
        self.duration = duration;
    }

    fn amount(&self, now: Instant) -> f32 {
        let Some(start) = self.start else {
            return self.to;
        };
        if self.duration.is_zero() {
            return self.to;
        }
        let done = (now.saturating_duration_since(start).as_secs_f32()
            / self.duration.as_secs_f32())
        .clamp(0.0, 1.0);
        self.from + (self.to - self.from) * done
    }

    fn moving(&self, now: Instant) -> bool {
        self.start
            .is_some_and(|start| !self.duration.is_zero() && now < start + self.duration)
    }
}
