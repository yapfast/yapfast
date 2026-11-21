//! A list's scroll reports, passed on only when they matter ([`Settle`]).

use std::time::{Duration, Instant};

use iced::{Element, advanced::Shell, widget::scrollable::Viewport, window};

use super::{At, Hooks, Inner, Wrap};

/// How far a list may scroll before its report goes on: well inside the
/// rows built beyond its edges (`sync::MARGIN`), so the rows it shows are
/// always built.
pub const SETTLE_STEP: f32 = 120.0;
/// How long a report may wait while the list scrolls by less; the last
/// one goes on this long after the list stops.
pub const SETTLE_WAIT: Duration = Duration::from_millis(100);

/// Its child, a scrollable, whose scroll reports (those `pick` finds among
/// what it says) go on only when they matter: a step of [`SETTLE_STEP`],
/// its end reached or left, its size or its content's changed, or
/// [`SETTLE_WAIT`] since the last, and the last once it stops. Each report
/// that goes on builds the whole view again; a scrolling list says where it is
/// on every frame it moves, and its rows built beyond its
/// edges already cover a step.
pub struct Settle<M> {
    pick: fn(&M) -> Option<&Viewport>,
}

impl<M: Clone + 'static> Settle<M> {
    pub fn new<'a>(
        child: impl Into<Element<'a, M>>,
        pick: fn(&M) -> Option<&Viewport>,
    ) -> Wrap<'a, Self, M> {
        Wrap::new(child, Self { pick })
    }
}

/// What a report says that matters.
#[derive(Clone, Copy, PartialEq)]
struct Seen {
    offset: f32,
    at_end: bool,
    size: (f32, f32),
    content: (f32, f32),
}

impl Seen {
    fn of(viewport: &Viewport) -> Self {
        let offset = viewport.absolute_offset();
        let bounds = viewport.bounds();
        let content = viewport.content_bounds();
        Self {
            offset: offset.y,
            // Either end: an anchored list's offset counts from its end.
            at_end: offset.y <= 2.0 || viewport.absolute_offset_reversed().y <= 2.0,
            size: (bounds.width, bounds.height),
            content: (content.width, content.height),
        }
    }

    fn matters(&self, last: &Self) -> bool {
        (self.offset - last.offset).abs() >= SETTLE_STEP
            || self.at_end != last.at_end
            || self.size != last.size
            || self.content != last.content
    }
}

pub struct SettleState<M> {
    /// The last report passed on, and when.
    last: Option<(Seen, Instant)>,
    /// The newest held back.
    held: Option<M>,
}

impl<M> Default for SettleState<M> {
    fn default() -> Self {
        Self {
            last: None,
            held: None,
        }
    }
}

impl<M: Clone + 'static> Hooks<M> for Settle<M> {
    type State = SettleState<M>;

    fn update(
        &mut self,
        state: &mut SettleState<M>,
        event: &iced::Event,
        _at: At,
        shell: &mut Shell<'_, M>,
        child: &mut Inner<'_, '_, M>,
    ) {
        let mut said = Vec::new();
        let mut inner = Shell::new(&mut said);
        child.update(&mut inner);
        // What the child asked of the window, as its shell would carry it.
        if inner.is_event_captured() {
            shell.capture_event();
        }
        match inner.redraw_request() {
            window::RedrawRequest::NextFrame => shell.request_redraw(),
            window::RedrawRequest::At(at) => shell.request_redraw_at(at),
            window::RedrawRequest::Wait => {}
        }
        if inner.is_layout_invalid() {
            shell.invalidate_layout();
        }
        if inner.are_widgets_invalid() {
            shell.invalidate_widgets();
        }
        shell.input_method_mut().merge(inner.input_method());
        drop(inner);
        let now = Instant::now();
        for message in said {
            let Some(seen) = (self.pick)(&message).map(Seen::of) else {
                shell.publish(message);
                continue;
            };
            let due = state.last.is_none_or(|(last, when)| {
                seen.matters(&last) || now.duration_since(when) >= SETTLE_WAIT
            });
            if due {
                state.last = Some((seen, now));
                state.held = None;
                shell.publish(message);
            } else {
                state.held = Some(message);
                shell.request_redraw_at(now + SETTLE_WAIT);
            }
        }
        // The list stopped: the last report held back goes on.
        if let iced::Event::Window(window::Event::RedrawRequested(_)) = event
            && let Some((_, when)) = state.last
            && now.duration_since(when) >= SETTLE_WAIT
            && let Some(message) = state.held.take()
        {
            if let Some(viewport) = (self.pick)(&message) {
                state.last = Some((Seen::of(viewport), now));
            }
            shell.publish(message);
        } else if state.held.is_some() {
            shell.request_redraw_at(now + SETTLE_WAIT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SETTLE_STEP, Seen};

    #[test]
    fn a_report_matters_after_a_step_an_end_or_a_new_size() {
        let last = Seen {
            offset: 500.0,
            at_end: false,
            size: (800.0, 600.0),
            content: (800.0, 4000.0),
        };
        let moved = |offset| Seen { offset, ..last };
        assert!(!moved(500.0 + SETTLE_STEP - 1.0).matters(&last));
        assert!(!moved(500.0 - SETTLE_STEP + 1.0).matters(&last));
        assert!(moved(500.0 + SETTLE_STEP).matters(&last));
        assert!(moved(500.0 - SETTLE_STEP).matters(&last));
        assert!(
            Seen {
                at_end: true,
                ..moved(501.0)
            }
            .matters(&last)
        );
        assert!(
            Seen {
                size: (800.0, 500.0),
                ..last
            }
            .matters(&last)
        );
        assert!(
            Seen {
                content: (800.0, 4200.0),
                ..last
            }
            .matters(&last)
        );
    }
}
