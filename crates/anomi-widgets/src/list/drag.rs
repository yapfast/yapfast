//! Dragging rows to reorder them: a press on a row that may move, then the
//! pointer moving 4 px with the button down, picks it up; it follows the
//! pointer faintly, a line marks where it would drop (under the row it's
//! over when dragged down, above it when dragged up, as Discord draws it),
//! and the list scrolls while the pointer is near its top or bottom. Letting
//! go drops it there if it may go there and the pointer is still over the
//! list; Escape puts it back. The owner hears of each row the pointer goes
//! over, and of none while it's off the list; a list without the line
//! leaves marking it to the owner, as someone dragged into a voice channel
//! outlines it, draws the copy just below the pointer, and drops only while
//! the pointer is over the list across as well as down. The Slint side:
//! `DropLine`, the rows' `start-drag`, `dragged` and `dropped`, `over-list`,
//! `move-aimed` and the drag's `Timer` in `ui/rail.slint:787`,
//! `:1201`-`:1316`, `:1474`, and `RoleListRow`'s handle in
//! `ui/roles.slint:143`.

use std::time::Instant;

use iced::advanced::renderer::{Quad, Renderer as _};
use iced::advanced::{Shell, mouse};
use iced::{Border, Point, Rectangle, Shadow, Size, Vector};

use anomi_theme::{Element, Renderer, Theme, metrics, rgba};

use super::List;
use super::ghost::Ground;
use super::live::{self, Aim, Zone};
use super::state::State;

/// How far the pointer moves with the button down before a row is picked
/// up (`ui/rail.slint:568`).
const PICK_UP: f32 = 4.0;
/// How near the list's top or bottom the pointer scrolls it, and how fast:
/// 8 px every 16 ms (`ui/rail.slint:1303`).
const EDGE: f32 = 40.0;
const EDGE_SPEED: f32 = 500.0;
/// The drop line: 4 px of the live colour with a soft shadow, blurred 3 px
/// (`ui/rail.slint:787`, `:789`).
const LINE: f32 = 4.0;
const LINE_SHADOW: Shadow = Shadow {
    color: rgba(0x0000_0066),
    offset: Vector::ZERO,
    blur_radius: 3.0,
};

/// What happened to a drag, for the list's owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drag {
    /// The row at `from` was picked up: rows it can't drop among may dim.
    Started { from: usize },
    /// The pointer went over the row at `to`'s place, or off the rows or
    /// the list (`None`): where it would drop, for the owner to mark until
    /// it's dropped or cancelled.
    Over { from: usize, to: Option<usize> },
    /// In a list that makes room ([`List::live`]): the pointer aims at the
    /// row at `to` and meets it there, or is off the list (`None`); the
    /// owner shows the row where that puts it.
    Aimed {
        from: usize,
        to: Option<(usize, Zone)>,
    },
    /// It was dropped on the row at `to`'s place; in a list that makes
    /// room, where the owner last showed it (`to` is `from`).
    Dropped { from: usize, to: usize },
    /// It went back where it was, or its row left the list.
    Cancelled { from: usize },
}

impl Drag {
    /// The row dragged.
    pub fn from(&self) -> usize {
        match *self {
            Drag::Started { from }
            | Drag::Over { from, .. }
            | Drag::Aimed { from, .. }
            | Drag::Dropped { from, .. }
            | Drag::Cancelled { from } => from,
        }
    }
}

/// What the owner says about dragging its rows.
pub struct Reorder<'a, Message> {
    pub(super) may_drag: Box<dyn Fn(usize) -> bool + 'a>,
    pub(super) may_drop: Box<dyn Fn(usize, usize) -> bool + 'a>,
    pub(super) on_drag: Box<dyn Fn(Drag) -> Message + 'a>,
    /// Only a press this near a row's left edge picks it up, as a role is
    /// dragged by its handle; anywhere on the row when `None`.
    pub(super) handle: Option<f32>,
    /// A line shows where it would drop. Without it the rows are targets
    /// the owner marks, and it drops only over the list across too.
    pub(super) line: bool,
    /// The ground under the copy of the row dragged from each index.
    pub(super) ground: Box<dyn Fn(usize) -> Ground + 'a>,
    /// Builds the copy of the row dragged from each index, where it isn't
    /// the row itself.
    pub(super) copy: Option<Box<dyn Fn(usize) -> Element<'a, Message> + 'a>>,
    /// The rows make room ([`List::live`]): whether row `to` takes the row
    /// from `from` onto itself.
    pub(super) onto: Option<Box<dyn Fn(usize, usize) -> bool + 'a>>,
}

impl<Message> Reorder<'_, Message> {
    /// The pointer is over the list: down it, and across too for rows that
    /// are targets (`over-list`, `ui/rail.slint:1246`).
    pub(super) fn over_list(&self, bounds: Rectangle, pointer: Point) -> bool {
        let down = pointer.y >= bounds.y && pointer.y < bounds.y + bounds.height;
        let across = pointer.x >= bounds.x && pointer.x < bounds.x + bounds.width;
        down && (self.line || across)
    }
}

/// The pointer's dealings with the rows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Gesture {
    #[default]
    Idle,
    /// The button went down on a row that may be dragged.
    Pressed {
        key: u64,
        index: usize,
        at: Point,
        grab: f32,
    },
    /// The row is being dragged: where the pointer holds it, and the row
    /// the pointer is over; in a list that makes room, the aim told and
    /// one waiting for the dwell, since when.
    Dragging {
        key: u64,
        index: usize,
        grab: f32,
        pointer: Point,
        over: Option<usize>,
        aim: Option<Aim>,
        waiting: Option<(Option<Aim>, Instant)>,
    },
}

impl Gesture {
    pub fn dragging(&self) -> bool {
        matches!(self, Gesture::Dragging { .. })
    }

    /// Whether `pointer` moved far enough from the press to pick it up.
    pub fn picks_up(at: Point, pointer: Point) -> bool {
        (pointer.y - at.y).abs() > PICK_UP
    }
}

/// How far to scroll for the frame `elapsed` after the last while the
/// pointer is at `y` in `bounds`: negative near the top.
pub fn edge_scroll(bounds: Rectangle, y: f32, elapsed: f32) -> f32 {
    let step = EDGE_SPEED * elapsed.min(0.05);
    if y < bounds.y + EDGE {
        -step
    } else if y > bounds.y + bounds.height - EDGE {
        step
    } else {
        0.0
    }
}

/// Whether the edge scroll needs the next frame.
pub fn near_edge(bounds: Rectangle, y: f32) -> bool {
    edge_scroll(bounds, y, 1.0) != 0.0
}

/// The row whose place the pointer at `y` is over: the row, or the 2 px
/// above it (`ui/rail.slint:1385`). `rows` are each row's top and height.
pub fn over(rows: impl Iterator<Item = (usize, f32, f32)>, y: f32) -> Option<usize> {
    let mut found = None;
    for (index, top, height) in rows {
        if y >= top - 2.0 && y < top + height {
            found = Some(index);
        }
    }
    found
}

/// The line where the row dragged from `from` drops on the row at `to`,
/// whose bounds are `row`: under it when dragged down, above it when up,
/// and inside its top when nothing is above it to draw in.
pub fn draw_line(
    renderer: &mut Renderer,
    theme: &Theme,
    row: Rectangle,
    from: usize,
    to: usize,
    first_at_top: bool,
) {
    let y = if to > from {
        row.y + row.height - 1.0
    } else if first_at_top {
        row.y
    } else {
        row.y - 3.0
    };
    let inset = metrics::ROW_INSET;
    renderer.fill_quad(
        Quad {
            bounds: Rectangle::new(
                Point::new(row.x + inset, y),
                Size::new((row.width - inset).max(0.0), LINE),
            ),
            border: Border::default().rounded(LINE / 2.0),
            shadow: LINE_SHADOW,
            ..Quad::default()
        },
        theme.palette().live,
    );
}

/// The time since the last frame, for the edge scroll.
pub fn elapsed(last: Option<Instant>, now: Instant) -> f32 {
    last.map_or(0.0, |last| {
        now.saturating_duration_since(last).as_secs_f32()
    })
}

/// A press on a row that may be dragged readies it.
pub fn press<'a, Message: 'a>(
    list: &List<'a, Message>,
    state: &mut State,
    bounds: Rectangle,
    cursor: mouse::Cursor,
) {
    let (Some(reorder), Some(position)) = (&list.reorder, cursor.position()) else {
        return;
    };
    if list.on_bar(state, bounds, cursor) {
        return;
    }
    let Some(index) = row_over(state, bounds, position.y) else {
        return;
    };
    let at = index - state.built.first;
    let (top, _) = state.row(at);
    let in_handle = reorder
        .handle
        .is_none_or(|width| position.x < bounds.x + list.padding.left + width);
    if in_handle && (reorder.may_drag)(index) {
        state.gesture = Gesture::Pressed {
            key: state.built.keys[at],
            index,
            at: position,
            grab: position.y - (bounds.y + top),
        };
    }
}

/// The row under `y`, in the window's coordinates.
pub fn row_over(state: &State, bounds: Rectangle, y: f32) -> Option<usize> {
    let rows = (0..state.built.keys.len()).map(|at| {
        let (top, height) = state.row(at);
        (state.built.first + at, bounds.y + top, height)
    });
    over(rows, y)
}

/// The row a drag with the pointer at `pointer` would drop on: the one
/// under it, while over the list (across too for rows that are targets).
pub(super) fn target<'a, Message: 'a>(
    list: &List<'a, Message>,
    state: &State,
    bounds: Rectangle,
    pointer: Point,
) -> Option<usize> {
    let reorder = list.reorder.as_ref()?;
    // Past the list's edges the rows there are built out of view.
    if !reorder.over_list(bounds, pointer) {
        return None;
    }
    row_over(state, bounds, pointer.y)
}

/// A drag follows the pointer to `pointer`, or the rows scrolled under it:
/// the owner hears of a new row under it, or where it aims in a list that
/// makes room.
pub fn follow<'a, Message: 'a>(
    list: &List<'a, Message>,
    state: &mut State,
    bounds: Rectangle,
    pointer: Point,
    now: Instant,
    shell: &mut Shell<'_, Message>,
) {
    if let Gesture::Dragging { pointer: held, .. } = &mut state.gesture {
        *held = pointer;
    }
    if list
        .reorder
        .as_ref()
        .is_some_and(|reorder| reorder.onto.is_some())
    {
        live::follow(list, state, bounds, pointer, now, shell);
        return;
    }
    let to = target(list, state, bounds, pointer);
    let Gesture::Dragging {
        index,
        pointer: held,
        over,
        ..
    } = &mut state.gesture
    else {
        return;
    };
    *held = pointer;
    if *over != to {
        *over = to;
        if let Some(reorder) = &list.reorder {
            shell.publish((reorder.on_drag)(Drag::Over { from: *index, to }));
        }
    }
}

/// Letting go: a dragged row drops where it may go while over the list.
pub fn release<'a, Message: 'a>(
    list: &List<'a, Message>,
    state: &mut State,
    bounds: Rectangle,
    shell: &mut Shell<'_, Message>,
) {
    let gesture = std::mem::take(&mut state.gesture);
    let Gesture::Dragging {
        key,
        index: from,
        grab,
        pointer,
        over,
        ..
    } = gesture
    else {
        return;
    };
    let Some(reorder) = &list.reorder else {
        return;
    };
    let inside = reorder.over_list(bounds, pointer);
    let live = reorder.onto.is_some();
    if live && inside {
        // It lands from where its copy is.
        if let Some(at) = state.built.keys.iter().position(|built| *built == key) {
            let (top, _) = state.row(at);
            let from_copy = pointer.y - grab - (bounds.y + top);
            let duration = list.motion.duration(anomi_theme::motion::SLIDE);
            state.slides.start(key, from_copy, duration, Instant::now());
        }
    }
    let drag = match over {
        _ if live && inside => Drag::Dropped { from, to: from },
        Some(to) if inside && to != from && (reorder.may_drop)(from, to) => {
            Drag::Dropped { from, to }
        }
        _ => Drag::Cancelled { from },
    };
    shell.publish((reorder.on_drag)(drag));
    shell.capture_event();
    shell.request_redraw();
}

/// A drag under way goes back.
pub fn cancel<'a, Message: 'a>(
    list: &List<'a, Message>,
    state: &mut State,
    shell: &mut Shell<'_, Message>,
) {
    if let Gesture::Dragging { index, .. } = std::mem::take(&mut state.gesture)
        && let Some(reorder) = &list.reorder
    {
        shell.publish((reorder.on_drag)(Drag::Cancelled { from: index }));
        shell.request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_row_over_counts_the_gap_above_it() {
        let rows = [(0, 0.0, 32.0), (1, 34.0, 32.0), (2, 68.0, 32.0)];
        assert_eq!(over(rows.into_iter(), 10.0), Some(0));
        assert_eq!(over(rows.into_iter(), 32.5), Some(1), "the gap above row 1");
        assert_eq!(over(rows.into_iter(), 120.0), None);
    }

    #[test]
    fn scrolls_near_the_edges_only() {
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(200.0, 400.0));
        assert!(edge_scroll(bounds, 10.0, 0.016) < 0.0);
        assert!(edge_scroll(bounds, 390.0, 0.016) > 0.0);
        assert_eq!(edge_scroll(bounds, 200.0, 0.016), 0.0);
        assert!(
            (edge_scroll(bounds, 390.0, 0.016) - 8.0).abs() < 0.1,
            "8 px a 16 ms frame"
        );
    }
}
