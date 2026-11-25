//! Every list that scrolls: iced's scrollable with Yapfast's own wheel and
//! touchpad motion ([`scroll_view`], [`flick`]).
//!
//! iced's scrollable moves by each wheel event's whole distance at once.
//! Here a `Smooth` around it takes the wheel's events that nothing inside
//! it took and applies touchpad movement directly or animates wheel
//! notches on the window's frames, by [`flick::Flick`]. A `Claim` round what
//! it holds marks those events after what's inside had them, in place of
//! the scrollable's own handling.
//!
//! iced's scroll bar is as wide all the time and has a rail. The `Smooth`
//! draws and drags the list's own instead ([`bar`]), over what the list
//! holds, and iced's is left without width.

use std::{
    cell::Cell,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

use iced::{
    Border, Element, Length, Point, Rectangle, Vector,
    advanced::{
        Shell, mouse, renderer,
        widget::{Id, Operation, operation},
    },
    widget::scrollable::{self, AbsoluteOffset, Scrollable, Viewport},
    window,
};

use crate::frames;

use super::{At, Hooks, Inner, Wrap};

mod bar;
pub mod flick;

pub use bar::Scroller;
use flick::{Flick, Room, Rooms, ScrollKind};

thread_local! {
    /// Set by a `Claim` as a wheel event goes through what a scrollable
    /// holds untaken, for the `Smooth` round the scrollable to take.
    static CLAIMED: Cell<bool> = const { Cell::new(false) };
}

/// The window's pixels to each of the view's, for a touchpad's moves:
/// iced hands them on in the window's pixels.
static PIXEL_SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// Says how many of the window's pixels make one of the view's (the
/// display's scale times the zoom).
pub fn set_pixel_scale(scale: f32) {
    if scale.is_finite() && scale > 0.0 {
        PIXEL_SCALE.store(scale.to_bits(), Ordering::Relaxed);
    }
}

/// How many of the window's pixels make one of the view's, as last set.
pub fn pixel_scale() -> f32 {
    f32::from_bits(PIXEL_SCALE.load(Ordering::Relaxed))
}

/// A list that scrolls: iced's scrollable, whose wheel moves it over a
/// moment, faster spun fast, and whose touchpad follows the fingers
/// directly, with a bar over its edge that takes no room from
/// what it holds. It scrolls down unless it's said to scroll
/// [`across`](ScrollView::across).
pub fn scroll_view<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> ScrollView<'a, M> {
    ScrollView {
        inner: iced::widget::scrollable(Wrap::new(content, Claim))
            .direction(scrollable::Direction::Vertical(no_bar())),
        smooth: Smooth {
            across: false,
            down: true,
            anchored: false,
            bar: Some(Scroller::default()),
        },
    }
}

/// iced's own bar, without width: nothing of it shows or takes the
/// pointer.
fn no_bar() -> scrollable::Scrollbar {
    scrollable::Scrollbar::new().width(0).scroller_width(0)
}

/// A [`scroll_view`] being built.
pub struct ScrollView<'a, M> {
    inner: Scrollable<'a, M>,
    smooth: Smooth,
}

impl<'a, M> ScrollView<'a, M> {
    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.inner = self.inner.id(id);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    /// Its bar's inks: the theme's (`style::scrollbar`).
    pub fn style(mut self, scroller: Scroller) -> Self {
        if self.smooth.bar.is_some() {
            self.smooth.bar = Some(scroller);
        }
        self
    }

    /// Without a bar: it scrolls all the same.
    pub fn barless(mut self) -> Self {
        self.smooth.bar = None;
        self
    }

    pub fn on_scroll(mut self, on: impl Fn(Viewport) -> M + 'a) -> Self {
        self.inner = self.inner.on_scroll(on);
        self
    }

    /// Scrolls across, not down.
    pub fn across(mut self) -> Self {
        self.smooth.across = true;
        self.smooth.down = false;
        self.inner = self
            .inner
            .direction(scrollable::Direction::Horizontal(no_bar()));
        self
    }

    /// Held to its foot: what it holds grows upwards, and its offset counts
    /// from the end.
    pub fn anchor_bottom(mut self) -> Self {
        self.smooth.anchored = true;
        self.inner = self.inner.anchor_bottom();
        self
    }
}

impl<'a, M: 'a> From<ScrollView<'a, M>> for Element<'a, M> {
    fn from(view: ScrollView<'a, M>) -> Self {
        Wrap::new(view.inner, view.smooth).into()
    }
}

/// Round a scrollable: its motion.
struct Smooth {
    across: bool,
    down: bool,
    /// Held to its foot, so iced counts its offset from there.
    anchored: bool,
    /// Its bar's inks; `None` for a list without one.
    bar: Option<Scroller>,
}

#[derive(Default)]
pub struct SmoothState {
    flick: Flick,
    shift: bool,
    bar: bar::BarState,
}

/// How far a wheel event moves the view, down and right positive, in the
/// view's pixels; with Shift, down becomes across.
fn view_delta(delta: mouse::ScrollDelta, shift: bool) -> Vector {
    let delta = match delta {
        mouse::ScrollDelta::Lines { x, y } => Vector::new(x, y) * -flick::WHEEL_NOTCH,
        mouse::ScrollDelta::Pixels { x, y } => Vector::new(x, y) * (-1.0 / pixel_scale()),
    };
    if shift {
        Vector::new(delta.y, delta.x)
    } else {
        delta
    }
}

impl Smooth {
    /// Runs `step` with the scrollable's room and moves it as far as
    /// `step` says.
    fn drive<M>(
        &self,
        child: &mut Inner<'_, '_, M>,
        step: &mut (dyn FnMut(Rooms) -> Vector + Send),
    ) {
        child.operate(&mut Drive {
            smooth: self,
            step,
            done: false,
        });
    }

    fn rooms<M>(&self, child: &mut Inner<'_, '_, M>) -> Rooms {
        let mut rooms = Rooms::default();
        self.drive(child, &mut |found| {
            rooms = found;
            Vector::ZERO
        });
        rooms
    }

    fn move_by<M>(&self, child: &mut Inner<'_, '_, M>, by: Vector) {
        if by != Vector::ZERO {
            self.drive(child, &mut |_| by);
        }
    }

    /// A wheel event goes to the list first while the pointer stays where
    /// it scrolled recently. Otherwise, the list's contents get it first.
    fn wheel<M>(
        &self,
        state: &mut SmoothState,
        delta: mouse::ScrollDelta,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        let kind = match delta {
            mouse::ScrollDelta::Pixels { x: 0.0, y: 0.0 } => {
                state.flick.end_touchpad();
                child.update(shell);
                return;
            }
            mouse::ScrollDelta::Lines { .. } => ScrollKind::Wheel,
            mouse::ScrollDelta::Pixels { .. } => ScrollKind::Touchpad,
        };
        let Some(pointer) = at.cursor.position_over(at.bounds) else {
            child.update(shell);
            return;
        };
        if shell.is_event_captured() {
            child.update(shell);
            return;
        }
        let by = view_delta(delta, state.shift);
        if by == Vector::ZERO {
            child.update(shell);
            return;
        }
        let now = Instant::now();
        let rooms = self.rooms(child);
        let intercepts = state.flick.intercepts(now, by, pointer, rooms, kind);
        let field = by.y != 0.0 && (intercepts || state.flick.moving()) && {
            let mut find = WheelFieldAt {
                point: Some(pointer),
                scroll: None,
                found: false,
            };
            child.operate(&mut find);
            find.found
        };
        if field {
            state.flick.stop();
        }
        let take = if intercepts && !field {
            true
        } else {
            CLAIMED.set(false);
            child.update(shell);
            CLAIMED.replace(false)
        };
        if take {
            let outcome = match kind {
                ScrollKind::Wheel => state.flick.wheel(now, by, pointer, rooms),
                ScrollKind::Touchpad => state.flick.touchpad(now, by, pointer, rooms),
            };
            if frames::on() {
                tracing::debug!(
                    target: frames::TARGET,
                    t = frames::micros(now),
                    input = ?delta,
                    dy = by.y,
                    moved = outcome.moved.y,
                    offset = rooms.y.offset,
                    "wheel"
                );
            }
            if outcome.moved != Vector::ZERO {
                // Drawn, and said, on the next frame.
                self.move_by(child, outcome.moved);
                shell.request_redraw();
            }
            if outcome.taken {
                shell.capture_event();
            }
        }
        if state.flick.moving() {
            shell.request_redraw();
        }
    }
}

/// Finds a text field under the wheel, following the scroll offsets and clips
/// that iced applies before giving it the event.
struct WheelFieldAt {
    point: Option<Point>,
    scroll: Option<(Rectangle, Vector)>,
    found: bool,
}

impl Operation for WheelFieldAt {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let point = self.point;
        if let Some((bounds, translation)) = self.scroll.take() {
            self.point = point
                .filter(|at| bounds.contains(*at))
                .map(|at| at + translation);
        }
        if self.point.is_some() && !self.found {
            operate(self);
        }
        self.point = point;
    }

    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn operation::Scrollable,
    ) {
        self.scroll = Some((bounds, translation));
    }

    fn custom(&mut self, _: Option<&Id>, bounds: Rectangle, state: &mut dyn std::any::Any) {
        self.found |= state.is::<crate::field::WheelField>()
            && self.point.is_some_and(|at| bounds.contains(at));
    }
}

impl Smooth {
    /// The bar's part in an event: the pointer over the list's edge
    /// thickens it, a press there takes its thumb, and the thumb follows
    /// the pointer until the button lifts. True when the event was the
    /// bar's alone.
    fn bar_event<M>(
        &self,
        state: &mut SmoothState,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) -> bool {
        if self.bar.is_none() {
            return false;
        }
        let iced::Event::Mouse(event) = event else {
            return false;
        };
        let tracks = |rooms| bar::tracks(at.bounds, rooms, self.down, self.across);
        let pointer = at.cursor.position_over(at.bounds);
        let mut taken = false;
        match event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => {
                let over = pointer.and_then(|pointer| {
                    let rooms = self.rooms(child);
                    state.bar.rooms = rooms;
                    Some((state.bar.under(pointer, &tracks(rooms))?, pointer))
                });
                if let Some((track, pointer)) = over
                    && !shell.is_event_captured()
                {
                    state.flick.stop();
                    let by = state.bar.press(track, pointer);
                    self.move_by(child, by);
                    shell.capture_event();
                    shell.request_redraw();
                    taken = true;
                }
            }
            mouse::Event::CursorMoved { position } if state.bar.held.is_some() => {
                // What the list holds may have grown since the press.
                let rooms = self.rooms(child);
                state.bar.rooms = rooms;
                let by = state.bar.drag(*position, &tracks(rooms));
                self.move_by(child, by);
                shell.capture_event();
                shell.request_redraw();
                taken = true;
            }
            mouse::Event::ButtonReleased(mouse::Button::Left) => state.bar.held = None,
            mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft => {}
            _ => return false,
        }
        let over = pointer
            .and_then(|pointer| state.bar.under(pointer, &tracks(state.bar.rooms)))
            .is_some();
        if state
            .bar
            .point(over || state.bar.held.is_some(), Instant::now())
        {
            shell.request_redraw();
        }
        if over && !taken {
            // What's under the bar's edge doesn't light for a pointer that
            // would take the bar.
            child.cursor = child.cursor.levitate();
        }
        taken
    }
}

impl<M> Hooks<M> for Smooth {
    type State = SmoothState;

    fn update(
        &mut self,
        state: &mut SmoothState,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        if self.bar_event(state, event, at, shell, child) {
            return;
        }
        match event {
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                self.wheel(state, *delta, at, shell, child);
                return;
            }
            iced::Event::Window(window::Event::RedrawRequested(now)) => {
                if state.flick.moving() {
                    let flick = &mut state.flick;
                    let trace = frames::on();
                    self.drive(child, &mut |rooms| {
                        let by = flick.frame(*now, rooms);
                        if trace {
                            trace_step(*now, rooms, by);
                        }
                        by
                    });
                    if state.flick.moving() {
                        shell.request_redraw();
                    }
                }
                if self.bar.is_some() {
                    // Where the frame about to be drawn has the list.
                    state.bar.rooms = self.rooms(child);
                    if state.bar.frame(*now) {
                        shell.request_redraw();
                    }
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if at.cursor.is_over(at.bounds) =>
            {
                state.flick.stop();
            }
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                state.shift = modifiers.shift();
            }
            _ => {}
        }
        child.update(shell);
    }

    /// The pointer over the bar's edge is the bar's, whatever is under it.
    fn interaction(
        &self,
        state: &SmoothState,
        _at: At,
        child: mouse::Interaction,
    ) -> mouse::Interaction {
        if state.bar.hot {
            mouse::Interaction::default()
        } else {
            child
        }
    }

    /// The list, then its bar over it.
    fn draw(
        &self,
        state: &SmoothState,
        renderer: &mut super::Renderer,
        at: At,
        child: &mut dyn FnMut(&mut super::Renderer),
    ) {
        use iced::advanced::Renderer as _;
        child(renderer);
        let Some(scroller) = self.bar else {
            return;
        };
        let tracks = bar::tracks(at.bounds, state.bar.rooms, self.down, self.across);
        let Some(shown) = at.bounds.intersection(&at.viewport) else {
            return;
        };
        if tracks.iter().all(Option::is_none) {
            return;
        }
        let grown = state.bar.grown;
        // A layer of its own: over the list's text and pictures.
        renderer.with_layer(shown, |renderer| {
            for track in tracks.iter().flatten() {
                let thumb = track.thumb(grown);
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: thumb,
                        border: Border::default().rounded(thumb.width.min(thumb.height) / 2.0),
                        ..renderer::Quad::default()
                    },
                    scroller.ink(grown),
                );
            }
        });
    }

    /// Put somewhere (`scroll_to`, `snap_to`), the list stops its motion;
    /// moved by (`scroll_by`, rows measured above what's read), it keeps on.
    fn operate(
        &mut self,
        state: &mut SmoothState,
        operation: &mut dyn Operation,
        child: &mut dyn FnMut(&mut dyn Operation),
    ) {
        let mut put = false;
        child(&mut Watched {
            inner: operation,
            put: &mut put,
            first: true,
        });
        if put {
            state.flick.stop();
        }
    }
}

/// A frame's step of a list's motion, for the frame trace: when the frame
/// was asked for and how late its step is taken, where the list was, and
/// how far it moves.
fn trace_step(frame: Instant, rooms: Rooms, by: Vector) {
    tracing::debug!(
        target: frames::TARGET,
        t = frames::micros(frame),
        late_us = frame.elapsed().as_micros() as u64,
        offset = rooms.y.offset,
        step = by.y,
        across = by.x,
        "scrolled"
    );
}

/// Moves the scrollable a [`Smooth`] is round by what `step` makes of
/// its room: the first scrollable its operation meets.
struct Drive<'a> {
    smooth: &'a Smooth,
    step: &'a mut (dyn FnMut(Rooms) -> Vector + Send),
    done: bool,
}

impl Operation for Drive<'_> {
    fn traverse(&mut self, _: &mut dyn FnMut(&mut dyn Operation)) {}

    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        state: &mut dyn operation::Scrollable,
    ) {
        if std::mem::replace(&mut self.done, true) {
            return;
        }
        let room = |on: bool, offset: f32, content: f32, shown: f32| Room {
            offset,
            max: if on { content - shown } else { 0.0 },
        };
        let rooms = Rooms {
            x: room(
                self.smooth.across,
                translation.x,
                content.width,
                bounds.width,
            ),
            y: room(
                self.smooth.down,
                translation.y,
                content.height,
                bounds.height,
            ),
        };
        let by = (self.step)(rooms);
        if by != Vector::ZERO {
            // A list held to its foot counts its offset up from there.
            let y = if self.smooth.anchored { -by.y } else { by.y };
            state.scroll_by(AbsoluteOffset { x: by.x, y }, bounds, content);
        }
    }
}

/// An operation passed on, noting whether it put the first scrollable it
/// meets somewhere.
struct Watched<'a> {
    inner: &'a mut dyn Operation,
    put: &'a mut bool,
    first: bool,
}

impl Operation for Watched<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        self.inner.traverse(operate);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        self.inner.container(id, bounds);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        state: &mut dyn operation::Scrollable,
    ) {
        if std::mem::replace(&mut self.first, false) {
            let mut state = Puts {
                inner: state,
                put: self.put,
            };
            self.inner
                .scrollable(id, bounds, content, translation, &mut state);
        } else {
            self.inner
                .scrollable(id, bounds, content, translation, state);
        }
    }

    fn focusable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        state: &mut dyn operation::Focusable,
    ) {
        self.inner.focusable(id, bounds, state);
    }

    fn text_input(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        state: &mut dyn operation::TextInput,
    ) {
        self.inner.text_input(id, bounds, state);
    }

    fn text(&mut self, id: Option<&Id>, bounds: Rectangle, text: &str) {
        self.inner.text(id, bounds, text);
    }

    fn custom(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn std::any::Any) {
        self.inner.custom(id, bounds, state);
    }

    fn finish(&self) -> operation::Outcome<()> {
        self.inner.finish()
    }
}

/// A scrollable's state, noting a move to a place.
struct Puts<'a> {
    inner: &'a mut dyn operation::Scrollable,
    put: &'a mut bool,
}

impl operation::Scrollable for Puts<'_> {
    fn snap_to(&mut self, offset: operation::scrollable::RelativeOffset<Option<f32>>) {
        *self.put = true;
        self.inner.snap_to(offset);
    }

    fn scroll_to(&mut self, offset: AbsoluteOffset<Option<f32>>) {
        *self.put = true;
        self.inner.scroll_to(offset);
    }

    fn scroll_by(&mut self, offset: AbsoluteOffset, bounds: Rectangle, content: Rectangle) {
        self.inner.scroll_by(offset, bounds, content);
    }
}

/// Round what a scrollable holds: a wheel event that nothing in it took,
/// over the part shown and along a way it scrolls, is taken here, before
/// the scrollable would move by it at once, for the [`Smooth`] round it.
struct Claim;

#[derive(Default)]
pub struct ClaimState {
    shift: bool,
}

impl<M> Hooks<M> for Claim {
    type State = ClaimState;

    fn update(
        &mut self,
        state: &mut ClaimState,
        event: &iced::Event,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        if let iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.shift = modifiers.shift();
        }
        child.update(shell);
        let iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) = event else {
            return;
        };
        if shell.is_event_captured() {
            return;
        }
        // Over its scrollbar the pointer floats above what it holds.
        let pointer = match at.cursor {
            mouse::Cursor::Available(at) | mouse::Cursor::Levitating(at) => at,
            mouse::Cursor::Unavailable => return,
        };
        let delta = view_delta(*delta, state.shift);
        let (content, shown) = (at.bounds, at.viewport);
        let along = (delta.y != 0.0 && content.height > shown.height + 0.5)
            || (delta.x != 0.0 && content.width > shown.width + 0.5);
        if shown.contains(pointer) && along {
            CLAIMED.set(true);
            shell.capture_event();
        }
    }
}

/// A touchpad move and the frame that shows it, for tests.
pub fn swipe(pixels: f32) -> [iced::Event; 2] {
    [
        iced::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: pixels },
        }),
        iced::Event::Window(window::Event::RedrawRequested(Instant::now())),
    ]
}

#[cfg(test)]
mod tests {
    use crate::testing::Message;
    use std::time::Duration;

    use iced::{
        Event, Point, Settings, Size,
        widget::{Space, column},
    };
    use iced_test::Simulator;

    use super::*;

    fn list<'a>() -> Element<'a, Message> {
        scroll_view(column![Space::new().height(4000.0)].width(Length::Fill))
            .on_scroll(Message::Scrolled)
            .width(300)
            .height(400)
            .into()
    }

    fn offsets(ui: Simulator<'_, Message>) -> Vec<f32> {
        ui.into_messages()
            .filter_map(|message| match message {
                Message::Scrolled(viewport) => Some(viewport.absolute_offset().y),
                _ => None,
            })
            .collect()
    }

    fn frame(at: Instant) -> Event {
        Event::Window(window::Event::RedrawRequested(at))
    }

    #[test]
    fn a_wheel_notch_arrives_over_several_frames() {
        let mut ui = Simulator::with_size(Settings::default(), Size::new(300.0, 400.0), list());
        ui.point_at(Point::new(150.0, 200.0));
        let start = Instant::now();
        ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
        })]);
        for millis in [16, 32, 48, 64, 96, 128, 200] {
            ui.simulate([frame(start + Duration::from_millis(millis))]);
        }
        let seen = offsets(ui);
        assert!(seen.len() >= 5, "a report each frame it moved: {seen:?}");
        assert!(seen[0] > 0.0 && seen[0] < flick::WHEEL_NOTCH, "{seen:?}");
        assert!(seen.windows(2).all(|pair| pair[1] > pair[0]), "{seen:?}");
        let last = *seen.last().expect("moved");
        assert!((last - flick::WHEEL_NOTCH).abs() < 0.01, "{seen:?}");
    }

    #[test]
    fn a_touchpad_follows_small_moves_from_the_first_event() {
        let mut ui = Simulator::with_size(Settings::default(), Size::new(300.0, 400.0), list());
        ui.point_at(Point::new(150.0, 200.0));
        ui.simulate(swipe(-1.5));
        ui.simulate(swipe(-2.0));
        ui.simulate(swipe(0.0));
        ui.simulate(swipe(-1.0));
        assert_eq!(offsets(ui), [1.5, 3.5, 4.5]);
    }

    #[test]
    fn a_touchpad_flick_stops_on_the_last_pixel_delta() {
        let mut ui = Simulator::with_size(Settings::default(), Size::new(300.0, 400.0), list());
        ui.point_at(Point::new(150.0, 200.0));
        ui.simulate(swipe(-20.0));
        ui.simulate(swipe(-40.0));
        let lifted = Instant::now();
        ui.simulate(swipe(0.0));
        for millis in [16, 32, 100, 300] {
            ui.simulate([frame(lifted + Duration::from_millis(millis))]);
        }
        assert_eq!(offsets(ui), [20.0, 60.0]);
    }

    #[test]
    fn the_bars_thumb_drags_the_list() {
        let mut ui = Simulator::with_size(Settings::default(), Size::new(300.0, 400.0), list());
        let start = Instant::now();
        let left = mouse::Button::Left;
        // The thumb is the top 40 px of the list's right edge: held 10 px
        // down it and taken 36 px further, a tenth of its way.
        ui.point_at(Point::new(295.0, 10.0));
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(left))]);
        let position = Point::new(295.0, 46.0);
        ui.point_at(position);
        ui.simulate([
            Event::Mouse(mouse::Event::CursorMoved { position }),
            frame(start + Duration::from_millis(16)),
        ]);
        // Let go, it stays where the pointer left it.
        let position = Point::new(150.0, 300.0);
        ui.point_at(position);
        ui.simulate([
            Event::Mouse(mouse::Event::ButtonReleased(left)),
            Event::Mouse(mouse::Event::CursorMoved { position }),
            frame(start + Duration::from_millis(32)),
        ]);
        let seen = offsets(ui);
        assert_eq!(seen, [360.0]);
    }

    #[test]
    fn a_list_put_somewhere_drops_its_motion() {
        let mut ui = Simulator::with_size(Settings::default(), Size::new(300.0, 400.0), list());
        ui.point_at(Point::new(150.0, 200.0));
        let start = Instant::now();
        ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
        })]);
        ui.simulate([frame(start + Duration::from_millis(16))]);
        // A press on it stops it where it is.
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        ))]);
        ui.simulate([frame(start + Duration::from_millis(300))]);
        let seen = offsets(ui);
        assert_eq!(seen.len(), 1, "no move after the press: {seen:?}");
        assert!(seen[0] < flick::WHEEL_NOTCH, "{seen:?}");
    }
}
