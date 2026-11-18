//! What the list does with an event: an autoscroll under way takes the
//! pointer, then the scrollbar its strip, then the rows see it, and what
//! they leave scrolls the list (a wheel), starts an autoscroll (a middle
//! press) or drags a row. Each frame steps a glide, an autoscroll or the
//! scroll at a drag's edge, carries out what an operation asked, and sends
//! a report that's due.

use std::time::Instant;

use iced::advanced::widget::{Tree, Widget as _};
use iced::advanced::{Layout, Shell, mouse};
use iced::{Event, Rectangle, keyboard, window};

use anomi_theme::Renderer;
use anomi_theme::motion::SLIDE;

use super::List;
use super::autoscroll::{Autoscroll, Outcome};
use super::drag::{self, Drag, Gesture};
use super::glide::{Glide, NOTCH};
use super::rows::moved;
use super::state::{Scrolled, State};
use crate::scrollbar::Response;

#[allow(clippy::too_many_arguments)]
pub fn update<'a, Message: 'a>(
    list: &mut List<'a, Message>,
    tree: &mut Tree,
    event: &Event,
    layout: Layout,
    cursor: mouse::Cursor,
    renderer: &Renderer,
    shell: &mut Shell<'_, Message>,
    viewport: &Rectangle,
) {
    let bounds = layout.bounds();
    let Tree {
        state, children, ..
    } = tree;
    let state = state.downcast_mut::<State>();
    let motion = list.motion;
    // A drag whose row the last layout found gone went back.
    if let Some(from) = state.lost.take()
        && let Some(reorder) = &list.reorder
    {
        shell.publish((reorder.on_drag)(Drag::Cancelled { from }));
        shell.request_redraw();
    }

    // An autoscroll takes the pointer while it lasts, and ends behind
    // another window.
    if !motion.front && state.autoscroll.is_some() {
        state.autoscroll = None;
        shell.request_redraw();
    }
    let autoscrolled = state
        .autoscroll
        .as_mut()
        .map(|autoscroll| (autoscroll.update(event), autoscroll.moving()));
    if let Some((outcome, moving)) = autoscrolled {
        match outcome {
            Outcome::Took => {
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            Outcome::EndedTaking => {
                state.autoscroll = None;
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            Outcome::Ended => {
                state.autoscroll = None;
                shell.request_redraw();
            }
            Outcome::Scroll(delta) => {
                if moving {
                    shell.request_redraw();
                }
                scrolled(list, state, delta, shell);
            }
            Outcome::Pass => {}
        }
    }

    // The bar's strip answers the pointer before the rows under it.
    let extent = state.extent();
    if list.scrollbar && !state.gesture.dragging() {
        match state.bar.update(event, bounds, extent, cursor, motion) {
            Response::Captured => {
                state.glide = None;
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            Response::Seek(share) => {
                state.glide = None;
                let delta = share * extent.maximum - extent.value;
                if delta.abs() < 2.0 * extent.page {
                    scrolled(list, state, delta, shell);
                } else {
                    state.ask(super::state::Request::Offset(share * extent.maximum));
                    shell.invalidate_layout();
                    shell.request_redraw();
                }
                shell.capture_event();
                return;
            }
            Response::Redraw => shell.request_redraw(),
            Response::None => {}
        }
    }

    let row_cursor = list.row_cursor(state, bounds, cursor);
    let clip = bounds.intersection(viewport).unwrap_or(bounds);
    // Rows that moved under a still pointer (a wheel's glide, a jump, rows
    // coming above) hear it where it now is, as if it moved, before the
    // frame: what's lit under it follows, as Slint's hover does.
    let lie = state.lie();
    let pointer = match (event, row_cursor.position()) {
        (Event::Mouse(mouse::Event::CursorMoved { .. }), _) => {
            state.pointed = Some(lie);
            None
        }
        (Event::Window(window::Event::RedrawRequested(_)), Some(position))
            if state.pointed.is_none_or(|pointed| !same_lie(pointed, lie)) =>
        {
            state.pointed = Some(lie);
            Some(Event::Mouse(mouse::Event::CursorMoved { position }))
        }
        _ => None,
    };
    for event in pointer.iter().chain([event]) {
        for (row, (child, child_tree)) in list.built.iter_mut().zip(layout.iter_mut(children)) {
            row.update(
                child_tree,
                event,
                moved(child, state.shift),
                row_cursor,
                renderer,
                shell,
                &clip,
            );
        }
    }
    if let Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) = event {
        let inside = cursor.land().is_over(bounds);
        if state.inside
            && !inside
            && let Some(left) = &list.on_leave
        {
            shell.publish(left());
        }
        state.inside = inside;
    }
    let captured = shell.is_event_captured();
    let over = cursor.is_over(bounds);

    match event {
        Event::Mouse(mouse::Event::WheelScrolled { delta }) if over && !captured => {
            let (pixels, notched) = match delta {
                mouse::ScrollDelta::Lines { y, .. } => (-y * NOTCH, true),
                mouse::ScrollDelta::Pixels { y, .. } => (-y, false),
            };
            if pixels == 0.0 {
                // Sideways: for what scrolls that way.
                return;
            }
            let now = Instant::now();
            let pixels = if notched {
                state.wheel.accelerate(pixels, now)
            } else {
                pixels
            };
            let glides = notched && motion.playing() && !list.on_bar(state, bounds, cursor);
            if glides && can_scroll(state, pixels) {
                match &mut state.glide {
                    Some(glide) => glide.extend(pixels, now),
                    None => state.glide = Some(Glide::new(pixels, motion.duration(SLIDE), now)),
                }
                shell.capture_event();
                shell.request_redraw();
            } else if !glides {
                state.glide = None;
                if scrolled(list, state, pixels, shell) != Scrolled::Blocked {
                    shell.capture_event();
                }
            }
        }
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle))
            if over && !captured && extent.scrolls() && motion.front =>
        {
            if let Some(position) = cursor.position() {
                state.glide = None;
                state.autoscroll = Some(Autoscroll::new(position));
                shell.capture_event();
                shell.request_redraw();
            }
        }
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if over => {
            drag::press(list, state, bounds, cursor);
        }
        Event::Mouse(mouse::Event::CursorMoved { position }) => {
            if let Gesture::Pressed {
                key,
                index,
                at,
                grab,
            } = state.gesture
                && Gesture::picks_up(at, *position)
            {
                state.gesture = Gesture::Dragging {
                    key,
                    index,
                    grab,
                    pointer: *position,
                    over: None,
                };
                if let Some(reorder) = &list.reorder {
                    shell.publish((reorder.on_drag)(Drag::Started { from: index }));
                    // The copy the owner builds is built at a layout.
                    if reorder.copy.is_some() {
                        shell.invalidate_layout();
                    }
                }
            }
            if state.gesture.dragging() {
                drag::follow(list, state, bounds, *position, shell);
                shell.capture_event();
                shell.request_redraw();
            }
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            drag::release(list, state, bounds, shell);
        }
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        }) if state.gesture.dragging() => {
            drag::cancel(list, state, shell);
            shell.capture_event();
        }
        Event::Window(window::Event::Unfocused) => drag::cancel(list, state, shell),
        Event::Window(window::Event::RedrawRequested(now)) => {
            frame(list, state, bounds, *now, shell)
        }
        _ => {}
    }
}

/// The rows lie as they did: the same first row, within half a pixel.
fn same_lie(a: (Option<u64>, f32), b: (Option<u64>, f32)) -> bool {
    a.0 == b.0 && (a.1 - b.1).abs() < 0.5
}

/// Scrolls the list as the person does, and redraws or lays it out again.
/// Returns what it did.
fn scrolled<'a, Message: 'a>(
    list: &List<'a, Message>,
    state: &mut State,
    delta: f32,
    shell: &mut Shell<'_, Message>,
) -> Scrolled {
    let result = state.scroll(delta);
    match result {
        Scrolled::Blocked => return result,
        Scrolled::Gap => shell.invalidate_layout(),
        Scrolled::Moved => {}
    }
    shell.request_redraw();
    list.see(state);
    result
}

/// The list can move `delta`'s way: a wheel at an end it reached goes on to
/// what holds the list.
fn can_scroll(state: &State, delta: f32) -> bool {
    let extent = state.extent();
    let built = &state.built;
    let unbuilt = if delta < 0.0 {
        built.first > 0
    } else {
        built.last() + 1 < built.count
    };
    unbuilt
        || state
            .glide
            .is_some_and(|glide| glide.remaining() * delta > 0.0)
        || (delta < 0.0 && extent.value > 0.5)
        || (delta > 0.0 && extent.maximum - extent.value > 0.5)
}

/// A frame: what an operation asked is laid out, a glide or a drag at the
/// edge steps on, and a report that's due goes.
fn frame<'a, Message: 'a>(
    list: &List<'a, Message>,
    state: &mut State,
    bounds: Rectangle,
    now: Instant,
    shell: &mut Shell<'_, Message>,
) {
    let motion = list.motion;
    // Laid out within this frame: iced updates it again after the layout.
    if state.asked {
        shell.invalidate_layout();
    }
    if let Some(mut glide) = state.glide {
        let (delta, done) = if motion.still() {
            (glide.remaining(), true)
        } else {
            glide.step(now)
        };
        let moved = delta.abs() < 1e-3 || scrolled(list, state, delta, shell) != Scrolled::Blocked;
        state.glide = (!done && moved).then_some(glide);
        if state.glide.is_some() {
            shell.request_redraw();
        }
    }
    if let Gesture::Dragging { pointer, .. } = state.gesture
        && motion.front
        && drag::near_edge(bounds, pointer.y)
    {
        let elapsed = drag::elapsed(state.drag_frame, now);
        let delta = drag::edge_scroll(bounds, pointer.y, elapsed);
        state.drag_frame = Some(now);
        // A step past the rows built leaves none under the pointer until
        // the layout after it builds them: the next frame follows it.
        let gap = delta != 0.0 && scrolled(list, state, delta, shell) == Scrolled::Gap;
        if !gap {
            drag::follow(list, state, bounds, pointer, shell);
        }
        shell.request_redraw();
    } else {
        state.drag_frame = None;
    }
    if let Some(report) = &list.on_viewport {
        match state.reports.take(now, motion.front) {
            Ok(viewport) => shell.publish(report(viewport)),
            Err(Some(at)) => shell.request_redraw_at(at),
            Err(None) => {}
        }
    }
}
