//! Every list that scrolls: iced's scrollable with Yapfast's own wheel and
//! touchpad motion ([`scroll_view`], [`flick`]).
//!
//! iced's scrollable moves by each wheel event's whole distance at once.
//! Here a `Smooth` around it takes the wheel's events that nothing inside
//! it took and moves it on the window's frames instead, by
//! [`flick::Flick`]; a `Claim` round what it holds marks those events,
//! after what's inside had them, in place of the scrollable's own handling.

use std::{
    cell::Cell,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};

use iced::{
    Element, Length, Pixels, Rectangle, Theme, Vector,
    advanced::{
        Shell, mouse,
        widget::{Id, Operation, operation},
    },
    widget::scrollable::{self, AbsoluteOffset, Scrollable, Viewport},
    window,
};

use crate::frames;

use super::{At, Hooks, Inner, Wrap};

pub mod flick;

use flick::{Flick, Phase, Room, Rooms};

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

fn pixel_scale() -> f32 {
    f32::from_bits(PIXEL_SCALE.load(Ordering::Relaxed))
}

/// A list that scrolls: iced's scrollable, whose wheel moves it over a
/// moment, faster spun fast, and whose touchpad follows the fingers and
/// flings as they lift. Built as iced's is.
pub fn scroll_view<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> ScrollView<'a, M> {
    ScrollView {
        inner: iced::widget::scrollable(Wrap::new(content, Claim)),
        smooth: Smooth {
            across: false,
            down: true,
            anchored: false,
        },
    }
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

    pub fn spacing(mut self, spacing: impl Into<Pixels>) -> Self {
        self.inner = self.inner.spacing(spacing);
        self
    }

    pub fn style(
        mut self,
        style: impl Fn(&Theme, scrollable::Status) -> scrollable::Style + 'a,
    ) -> Self {
        self.inner = self.inner.style(style);
        self
    }

    pub fn on_scroll(mut self, on: impl Fn(Viewport) -> M + 'a) -> Self {
        self.inner = self.inner.on_scroll(on);
        self
    }

    pub fn direction(mut self, direction: impl Into<scrollable::Direction>) -> Self {
        let direction = direction.into();
        self.smooth.across = direction.horizontal().is_some();
        self.smooth.down = direction.vertical().is_some();
        self.inner = self.inner.direction(direction);
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
}

#[derive(Default)]
pub struct SmoothState {
    flick: Flick,
    phases: Phases,
    shift: bool,
}

/// What the touchpad does, told from its events: iced hands on a wheel's
/// notches as lines and a touchpad's moves as pixels, without winit's
/// phases. On Wayland the fingers' lifting comes as a move of nothing, and
/// the next move after it is their going down again.
#[derive(Default)]
struct Phases {
    touching: bool,
}

impl Phases {
    fn next(&mut self, delta: mouse::ScrollDelta) -> Option<Phase> {
        match delta {
            mouse::ScrollDelta::Lines { .. } => {
                self.touching = false;
                Some(Phase::Moved)
            }
            mouse::ScrollDelta::Pixels { x, y } if x == 0.0 && y == 0.0 => {
                std::mem::take(&mut self.touching).then_some(Phase::Ended)
            }
            mouse::ScrollDelta::Pixels { .. } if self.touching => Some(Phase::Moved),
            mouse::ScrollDelta::Pixels { .. } => {
                self.touching = true;
                Some(Phase::Started)
            }
        }
    }
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

    /// A wheel event: to the list first while it moves under the
    /// fingers or the pointer stays where it scrolled, else to what it
    /// holds first, and to the list if nothing there took it.
    fn wheel<M>(
        &self,
        state: &mut SmoothState,
        phase: Phase,
        delta: mouse::ScrollDelta,
        at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        let Some(pointer) = at.cursor.position_over(at.bounds) else {
            child.update(shell);
            return;
        };
        if shell.is_event_captured() {
            child.update(shell);
            return;
        }
        let now = Instant::now();
        let delta = view_delta(delta, state.shift);
        let rooms = self.rooms(child);
        let take = if state.flick.intercepts(now, phase, delta, pointer, rooms) {
            true
        } else {
            CLAIMED.set(false);
            child.update(shell);
            // A touchpad's fingers going down or lifting tell every list
            // under them, whatever took the event.
            CLAIMED.replace(false) || phase != Phase::Moved
        };
        if take {
            let outcome = state.flick.wheel(now, delta, phase, pointer, rooms);
            if frames::on() {
                tracing::debug!(
                    target: frames::TARGET,
                    t = frames::micros(now),
                    phase = ?phase,
                    dy = delta.y,
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
        match event {
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                // Every list hears of every event, to tell the phases.
                if let Some(phase) = state.phases.next(*delta) {
                    self.wheel(state, phase, *delta, at, shell, child);
                    return;
                }
            }
            iced::Event::Window(window::Event::RedrawRequested(now)) if state.flick.moving() => {
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

/// A touchpad's two moves that scroll a list by `pixels` at once (the
/// first only begins the gesture, and the list holds back the next
/// 8 px), and the frame that shows it, for tests.
pub fn swipe(pixels: f32) -> [iced::Event; 3] {
    let sign = pixels.signum();
    let moved = |y| {
        iced::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y },
        })
    };
    [
        moved(sign),
        moved(pixels + 8.0 * sign),
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
    fn a_touchpad_moves_the_list_at_once_and_flings_once_lifted() {
        let mut ui = Simulator::with_size(Settings::default(), Size::new(300.0, 400.0), list());
        ui.point_at(Point::new(150.0, 200.0));
        ui.simulate(swipe(-100.0));
        let lifted = Instant::now();
        // The fingers stopped long ago, by the time they lift: no fling.
        ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: 0.0 },
        })]);
        ui.simulate([frame(lifted + Duration::from_millis(100))]);
        let seen = offsets(ui);
        assert_eq!(seen.last(), Some(&100.0), "{seen:?}");
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
