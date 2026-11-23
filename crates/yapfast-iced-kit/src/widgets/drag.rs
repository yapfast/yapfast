//! What's pressed and dragged: a bar to move through something playing
//! ([`Scrub`]), an edge that resizes ([`Splitter`]), and a row dragging
//! moves ([`Drags`]).

use std::time::Duration;

use iced::{
    Color, Element, Length, Point, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Tree, tree},
    },
};

use super::{At, Hooks, Inner, Renderer, Wrap, moved_to};

/// Its child, across which a press says where it was pressed (from 0 at
/// its left to 1 at its right) with `seek`, and so does the pointer while
/// it's held; letting go says `released`: a voice message's waveform.
pub struct Scrub<'a, M> {
    seek: Box<dyn Fn(f32) -> M + 'a>,
    released: Option<M>,
}

impl<'a, M: Clone> Scrub<'a, M> {
    pub fn new(
        child: impl Into<Element<'a, M>>,
        seek: impl Fn(f32) -> M + 'a,
    ) -> Wrap<'a, Self, M> {
        Wrap::new(
            child,
            Self {
                seek: Box::new(seek),
                released: None,
            },
        )
    }
}

impl<M: Clone> Wrap<'_, Scrub<'_, M>, M> {
    pub fn released(mut self, released: M) -> Self {
        self.hooks.released = Some(released);
        self
    }
}

/// Whether the button went down on it and is still down, and where.
#[derive(Default)]
pub struct ScrubState {
    held: bool,
    /// Where along it the last seek said, so a move that stays there says
    /// nothing.
    at: f32,
}

impl<M: Clone> Hooks<M> for Scrub<'_, M> {
    type State = ScrubState;

    fn update(
        &mut self,
        state: &mut ScrubState,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        child.update(shell);
        let bounds = at.bounds;
        let along = |x: f32| ((x - bounds.x) / bounds.width.max(1.0)).clamp(0.0, 1.0);
        if state.held
            && let Some(position) = moved_to(event, at.cursor)
            && along(position.x) != state.at
        {
            state.at = along(position.x);
            shell.publish((self.seek)(state.at));
        }
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = at.cursor.position_over(bounds) {
                    state.held = true;
                    state.at = along(position.x);
                    shell.publish((self.seek)(state.at));
                    shell.capture_event();
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.held => {
                state.held = false;
                if let Some(released) = &self.released {
                    shell.publish(released.clone());
                }
            }
            _ => {}
        }
    }

    fn interaction(&self, _: &ScrubState, at: At, _: mouse::Interaction) -> mouse::Interaction {
        if at.cursor.is_over(at.bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

// ---------------------------------------------------------------------------
// An edge dragged to resize

/// The edge between two panels, which a drag moves: 2 px wide, drawn as a
/// line of `edge` down its first, it takes the pointer `REACH` px either
/// side, and fills both under the pointer (`lit`, or `held` while dragged), says how far the
/// pointer has moved since the press with `dragged`, `released` as it's
/// let go, and `reset` on a double click: the thread beside's.
pub struct Splitter<'a, M> {
    edge: Color,
    lit: Color,
    held: Color,
    dragged: Box<dyn Fn(f32) -> M + 'a>,
    released: M,
    reset: M,
}

/// How far either side of the edge it takes the pointer.
const REACH: f32 = 3.0;
/// Its width: the edge's line and the lit line's.
const EDGE: f32 = 2.0;
/// The most time between a double click's clicks.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

#[derive(Default)]
struct SplitterState {
    /// Where the button went down, while it's held.
    from: Option<f32>,
    pointed: bool,
    last_click: Option<std::time::Instant>,
}

impl<'a, M: Clone> Splitter<'a, M> {
    pub fn new(
        edge: Color,
        lit: Color,
        held: Color,
        dragged: impl Fn(f32) -> M + 'a,
        released: M,
        reset: M,
    ) -> Self {
        Self {
            edge,
            lit,
            held,
            dragged: Box::new(dragged),
            released,
            reset,
        }
    }

    fn reach(bounds: Rectangle) -> Rectangle {
        Rectangle {
            x: bounds.x - REACH,
            width: bounds.width + 2.0 * REACH,
            ..bounds
        }
    }
}

impl<M: Clone> Widget<M, Theme, Renderer> for Splitter<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(EDGE), Length::Fill)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SplitterState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SplitterState::default())
    }

    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fixed(EDGE), Length::Fill, Size::ZERO))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<SplitterState>();
        let reach = Self::reach(layout.bounds());
        match event {
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(position) = cursor.position() else {
                    return;
                };
                let pointed = reach.contains(position);
                if pointed != state.pointed {
                    state.pointed = pointed;
                    shell.request_redraw();
                }
                if let Some(from) = state.from {
                    shell.publish((self.dragged)(position.x - from));
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor.position_over(reach) else {
                    return;
                };
                let now = std::time::Instant::now();
                if state
                    .last_click
                    .is_some_and(|last| now.duration_since(last) < DOUBLE_CLICK)
                {
                    state.last_click = None;
                    shell.publish(self.reset.clone());
                } else {
                    state.last_click = Some(now);
                    state.from = Some(at.x);
                }
                shell.capture_event();
                shell.request_redraw();
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.from.take().is_some() =>
            {
                shell.publish(self.released.clone());
                shell.request_redraw();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<SplitterState>();
        if state.from.is_some() || cursor.is_over(Self::reach(layout.bounds())) {
            mouse::Interaction::ResizingHorizontally
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let state = tree.state.downcast_ref::<SplitterState>();
        let bounds = layout.bounds();
        let held = state.from.is_some();
        let (ink, width) = if held {
            (self.held, EDGE)
        } else if state.pointed {
            (self.lit, EDGE)
        } else {
            (self.edge, 1.0)
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle { width, ..bounds },
                ..Default::default()
            },
            ink,
        );
    }
}

impl<'a, M: Clone + 'a> From<Splitter<'a, M>> for Element<'a, M> {
    fn from(splitter: Splitter<'a, M>) -> Self {
        Element::new(splitter)
    }
}

// ---------------------------------------------------------------------------
// A row that dragging moves

/// How far the pointer goes with the button down before a press becomes a
/// drag.
const DRAG_START: f32 = 4.0;

/// Its child, which a press and a move of [`DRAG_START`] px up or down
/// starts dragging (`drag`, with how far down the row it was pressed),
/// and a press let go on it without a drag clicks (`click`): a channel's
/// or a category's row in the sidebar. While a drag is on (`tracking`), it
/// says when the pointer comes onto it and leaves (`over`), and a release
/// on it clicks nothing; the drag itself
/// follows the pointer through the window's events, not through it.
pub struct Drags<'a, M> {
    click: Option<M>,
    drag: Option<Box<dyn Fn(Point, Point) -> M + 'a>>,
    over: Option<Box<dyn Fn(bool) -> M + 'a>>,
    tracking: bool,
    head: Option<f32>,
}

impl<'a, M: Clone> Drags<'a, M> {
    pub fn new(child: impl Into<Element<'a, M>>) -> Wrap<'a, Self, M> {
        Wrap::new(
            child,
            Self {
                click: None,
                drag: None,
                over: None,
                tracking: false,
                head: None,
            },
        )
    }
}

impl<'a, M: Clone> Wrap<'a, Drags<'a, M>, M> {
    /// Only a press on its top `height` px clicks or drags: a voice
    /// channel's own row, not who is listed under it.
    pub fn head(mut self, height: f32) -> Self {
        self.hooks.head = Some(height);
        self
    }

    pub fn click(mut self, click: M) -> Self {
        self.hooks.click = Some(click);
        self
    }

    pub fn drag(self, drag: impl Fn(f32) -> M + 'a) -> Self {
        self.drag_at(move |_, grab| drag(grab.y))
    }

    /// Starts a drag with the window pointer and the press offset in the tile.
    pub fn drag_at(mut self, drag: impl Fn(Point, Point) -> M + 'a) -> Self {
        self.hooks.drag = Some(Box::new(drag));
        self
    }

    pub fn tracking(mut self, tracking: bool, over: impl Fn(bool) -> M + 'a) -> Self {
        self.hooks.tracking = tracking;
        self.hooks.over = Some(Box::new(over));
        self
    }
}

#[derive(Default)]
pub struct DragsState {
    /// Where the button went down on it, while it's down.
    pressed: Option<Point>,
    /// The press moved far enough to be a drag.
    moved: bool,
    /// The pointer is over it while a drag is on.
    over: bool,
}

impl<M: Clone> Hooks<M> for Drags<'_, M> {
    type State = DragsState;

    fn update(
        &mut self,
        state: &mut DragsState,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        let bounds = at.bounds;
        let released = matches!(
            event,
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        );
        // A drag's end isn't the child's: a button under it would click.
        if !(released && self.tracking) {
            child.update(shell);
        }
        if let iced::Event::Mouse(mouse::Event::CursorMoved { position }) = event
            && let (Some(from), Some(cursor), Some(drag)) =
                (state.pressed, at.cursor.position(), &self.drag)
            && !state.moved
            && (cursor.y - from.y).abs() > DRAG_START
        {
            state.moved = true;
            shell.publish(drag(
                *position,
                Point::new(from.x - bounds.x, from.y - bounds.y),
            ));
        }
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                // Not a press a button inside took.
                let head = Rectangle {
                    height: self.head.unwrap_or(bounds.height),
                    ..bounds
                };
                state.pressed = at
                    .cursor
                    .position_over(head)
                    .filter(|_| !shell.is_event_captured());
                state.moved = false;
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let pressed = state.pressed.take();
                if pressed.is_some()
                    && !state.moved
                    && !self.tracking
                    && !shell.is_event_captured()
                    && at.cursor.is_over(bounds)
                    && let Some(click) = &self.click
                {
                    shell.publish(click.clone());
                    shell.capture_event();
                }
                state.moved = false;
            }
            _ => {}
        }
        // Over it while a drag is on; checked on each frame too, as the
        // list scrolling under a still pointer moves it.
        let over = self.tracking && at.cursor.is_over(bounds);
        if over != state.over {
            state.over = over;
            if let Some(on) = &self.over {
                shell.publish(on(over));
            }
        }
    }

    fn interaction(&self, _: &DragsState, at: At, child: mouse::Interaction) -> mouse::Interaction {
        if self.tracking {
            mouse::Interaction::Grabbing
        } else if child == mouse::Interaction::default() && at.cursor.is_over(at.bounds) {
            mouse::Interaction::Pointer
        } else {
            child
        }
    }

    /// No card beside a row while one is dragged.
    fn overlays(&self) -> bool {
        !self.tracking
    }
}
