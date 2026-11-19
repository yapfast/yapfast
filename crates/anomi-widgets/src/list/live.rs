//! Dragging in a list that makes room ([`List::live`]): the rows are the
//! preview of the move. The owner shows the dragged row as the gap where it
//! would land and moves it there as the pointer aims, the rows slide to
//! their new places ([`super::slide`]) and the copy follows the pointer
//! where it was grabbed.
//!
//! The pointer aims at the row under it: its upper quarter above it, its
//! lower quarter below it and its middle half onto it, where the owner
//! takes rows onto that one (a server dropped on a server makes a folder);
//! else its upper and lower halves. Over the gap it aims where it did, but
//! never onto a row: from there it aims [here](Zone::Here). A change onto a
//! row is told only once it held for [`DWELL`], so the preview doesn't
//! flicker on the line; every other change is told at once, leaving a row's
//! middle too, so what formed there goes back the moment the pointer is out
//! of it (out of one row's middle into another's, it aims here until the
//! dwell). Under the last row, in the list's room below it, it aims past
//! that row: after everything. Letting go over the list drops it where the
//! owner last showed it; off the list, or with Escape, it goes back.

use std::time::{Duration, Instant};

use iced::Point;
use iced::advanced::Shell;

use super::List;
use super::drag::{Drag, Gesture, target};
use super::state::State;

/// How long the pointer holds in a row's middle before the preview goes
/// onto that row.
pub const DWELL: Duration = Duration::from_millis(150);

/// Where a dragged row meets the row the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Above,
    Onto,
    Below,
    /// Under the list's last row, in the room below it: after everything,
    /// out of whatever holds that row.
    Past,
    /// At the dragged row itself: where it lies now, onto no row. Told
    /// once it was onto a row and the pointer is over its own place, or on
    /// its way into another row's middle.
    Here,
}

/// An aim, by the key of the row aimed at, which holds while rows move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aim {
    pub key: u64,
    pub zone: Zone,
}

impl<'a, Message> List<'a, Message> {
    /// Makes the rows make room for a dragged row, after [`Self::reorder`]:
    /// the owner hears [`Drag::Aimed`] as the pointer aims (see the
    /// module), moves the row there, and builds it as its gap; `onto` says
    /// whether row `to` takes the row from `from` onto itself. Without a
    /// line, and dropped only over the list across as well as down. The
    /// copy is held where it was grabbed, on the [ground](Self::drag_ground)
    /// the owner gives it.
    pub fn live(mut self, onto: impl Fn(usize, usize) -> bool + 'a) -> Self {
        if let Some(reorder) = &mut self.reorder {
            reorder.onto = Some(Box::new(onto));
            reorder.line = false;
        }
        self
    }
}

/// The zone of a row `height` tall that a pointer `y` below its top is in.
pub fn zone(y: f32, height: f32, onto: bool) -> Zone {
    let share = if height > 0.0 { y / height } else { 0.5 };
    if onto && (0.25..0.75).contains(&share) {
        Zone::Onto
    } else if share < 0.5 {
        Zone::Above
    } else {
        Zone::Below
    }
}

/// Whether `aim` is onto a row.
fn is_onto(aim: Option<Aim>) -> bool {
    aim.is_some_and(|aim| aim.zone == Zone::Onto)
}

/// Whether going to `aim` waits for the dwell: only onto a row. Leaving
/// one is told at once.
pub fn waits(aim: Option<Aim>) -> bool {
    is_onto(aim)
}

/// Where the pointer at `pointer` aims, on the rows as they lie now.
fn aim<'a, Message: 'a>(
    list: &List<'a, Message>,
    state: &State,
    bounds: iced::Rectangle,
    pointer: Point,
) -> Option<Option<Aim>> {
    let Gesture::Dragging {
        key,
        index: from,
        aim,
        ..
    } = state.gesture
    else {
        return None;
    };
    let onto = list.reorder.as_ref()?.onto.as_ref()?;
    let Some(to) = target(list, state, bounds, pointer) else {
        return Some(past(list, state, bounds, pointer));
    };
    if to == from {
        // Over its own place it's onto no row.
        return Some(if is_onto(aim) {
            Some(Aim {
                key,
                zone: Zone::Here,
            })
        } else {
            aim
        });
    }
    let at = to - state.built.first;
    let (top, height) = state.row(at);
    Some(Some(Aim {
        key: state.built.keys[at],
        zone: zone(pointer.y - (bounds.y + top), height, onto(from, to)),
    }))
}

/// The last row aimed past, with the pointer over the list under it.
fn past<'a, Message: 'a>(
    list: &List<'a, Message>,
    state: &State,
    bounds: iced::Rectangle,
    pointer: Point,
) -> Option<Aim> {
    let reorder = list.reorder.as_ref()?;
    let last = state.built.keys.len().checked_sub(1)?;
    let (top, height) = state.row(last);
    let under = reorder.over_list(bounds, pointer) && pointer.y >= bounds.y + top + height;
    (under && state.built.first + last + 1 == state.built.count).then(|| Aim {
        key: state.built.keys[last],
        zone: Zone::Past,
    })
}

/// A drag in a list that makes room follows the pointer at `pointer`, or
/// the rows moved under it: the owner hears where it aims once that holds.
pub fn follow<'a, Message: 'a>(
    list: &List<'a, Message>,
    state: &mut State,
    bounds: iced::Rectangle,
    pointer: Point,
    now: Instant,
    shell: &mut Shell<'_, Message>,
) {
    let Some(raw) = aim(list, state, bounds, pointer) else {
        return;
    };
    let built = &state.built;
    let Gesture::Dragging {
        key,
        index,
        aim: told,
        waiting,
        ..
    } = &mut state.gesture
    else {
        return;
    };
    if raw == *told {
        *waiting = None;
        return;
    }
    let mut tell = raw;
    if waits(raw) {
        let since = match *waiting {
            Some((pending, since)) if pending == raw => since,
            _ => {
                *waiting = Some((raw, now));
                now
            }
        };
        if now.saturating_duration_since(since) < DWELL {
            shell.request_redraw_at(since + DWELL);
            if !is_onto(*told) {
                return;
            }
            // Out of one row's middle into another's: the first goes back
            // now, the second waits.
            tell = Some(Aim {
                key: *key,
                zone: Zone::Here,
            });
        } else {
            *waiting = None;
        }
    } else {
        *waiting = None;
    }
    *told = tell;
    // The row aimed at, by its key; the dragged one where it lies.
    let (dragged, from) = (*key, *index);
    let to = tell.and_then(|aim| {
        let at = if aim.key == dragged {
            from
        } else {
            built.first + built.keys.iter().position(|row| *row == aim.key)?
        };
        Some((at, aim.zone))
    });
    if let Some(reorder) = &list.reorder {
        shell.publish((reorder.on_drag)(Drag::Aimed { from, to }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_middle_half_is_onto_where_the_row_takes_it() {
        assert_eq!(zone(5.0, 48.0, true), Zone::Above);
        assert_eq!(zone(12.0, 48.0, true), Zone::Onto);
        assert_eq!(zone(35.0, 48.0, true), Zone::Onto);
        assert_eq!(zone(36.0, 48.0, true), Zone::Below);
        assert_eq!(zone(20.0, 48.0, false), Zone::Above, "the upper half");
        assert_eq!(zone(24.0, 48.0, false), Zone::Below, "the lower half");
    }

    #[test]
    fn only_going_onto_a_row_waits_and_leaving_it_does_not() {
        let at = |zone| Some(Aim { key: 1, zone });
        assert!(waits(at(Zone::Onto)));
        assert!(!waits(at(Zone::Below)), "out of its middle");
        assert!(!waits(at(Zone::Here)), "over its own place");
        assert!(!waits(at(Zone::Past)));
        assert!(!waits(None), "off the list");
    }
}
