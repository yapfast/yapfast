//! What a press and the drag after it do to the rows: the press starts a
//! selection where it's in a part's text (`press`, `items/text.rs:480`);
//! the pointer, the wheel or a frame carries it on (`drag_to`,
//! `items/flickable.rs:249`), only where the pointer or the rows moved; a
//! drag held at the list's edges scrolls it (`scroll_at_edge`).

use std::cmp::Ordering;

use iced::Point;
use iced::advanced::widget::{Tree, Widget as _};
use iced::advanced::{Layout, Shell};

use anomi_theme::Renderer;

use super::Selection;
use super::edge::{self, Nudge};
use super::model::{At, Drag, Held, Position, State, next_version};
use super::pass::Showing;

impl<Message> Selection<'_, Message> {
    /// A left press at `point`: starts a selection there if it's in a part
    /// a press selects, a point or the word or line there. Returns whether
    /// it started.
    pub(super) fn press(
        &mut self,
        state: &mut State,
        content: &mut Tree,
        layout: Layout,
        renderer: &Renderer,
        point: Point,
    ) -> bool {
        let unit = state.unit(point);
        let walked = self.walk(
            content,
            layout,
            renderer,
            Some(point),
            Showing::all(state.version),
        );
        let Some(anchor) = walked.pressed(point) else {
            return false;
        };
        // The rows show nothing else: those the press acts on show this.
        let showing = Showing::all(state.changed());
        let row = anchor.row;
        let copy = walked.copying(&[row]);
        let mut shown = |selection: &mut Self, from: Position, to: Position| {
            let acts = walked.acts(from, to, unit, Some(row));
            let orders = walked.orders(&acts, &copy, unit);
            let pieces = selection.apply(content, layout, renderer, orders, showing);
            walked.copied(row, &acts, &pieces)
        };
        // The press's row as copied before the press and after it, for when
        // it scrolls away.
        let start = Position {
            part: 0,
            at: At::Start,
            ..anchor
        };
        let end = Position {
            part: usize::MAX,
            at: At::End,
            ..anchor
        };
        let before = shown(self, start, anchor);
        let after = shown(self, anchor, end);
        let mut held = Held::at(anchor, unit);
        held.first = shown(self, anchor, anchor);
        held.selected = !held.first.is_empty();
        let scroller = walked.scroller(anchor).cloned();
        state.held = Some(held);
        state.shown = state.version;
        state.drag = Some(Drag {
            pointer: point,
            before,
            after,
            scroller,
            frame: None,
            last: Some((point, walked.span())),
            side: Ordering::Equal,
        });
        true
    }

    /// Selects from where the press began to the pointer at `pointer`,
    /// unless neither it nor the rows moved since it last did.
    pub(super) fn drag_to(
        &mut self,
        state: &mut State,
        content: &mut Tree,
        layout: Layout,
        renderer: &Renderer,
        pointer: Point,
    ) {
        let State {
            drag: Some(drag),
            held: Some(held),
            version,
            shown,
            ..
        } = state
        else {
            return;
        };
        drag.pointer = pointer;
        if let Some(rows) = &self.rows {
            held.anchor = rows.found(held.anchor);
        }
        let walked = self.walk(content, layout, renderer, None, Showing::all(*version));
        // Where the list is now, should the window have changed.
        if let Some(scroller) = &mut drag.scroller
            && scroller.id.is_some()
            && let Some(now) = walked
                .scrollers
                .iter()
                .find(|found| found.id == scroller.id)
        {
            scroller.bounds = now.bounds;
        }
        let place = (pointer, walked.span());
        if drag.last == Some(place) {
            return;
        }
        let Some(focus) = walked.focus(pointer) else {
            return;
        };
        let (anchor, unit) = (held.anchor, held.unit);
        held.focus = focus;
        *version = next_version();
        let showing = Showing::all(*version);
        let copy = walked.copying(&[focus.row]);
        let acts = walked.acts(anchor, focus, unit, None);
        let orders = walked.orders(&acts, &copy, unit);
        let pieces = self.apply(content, layout, renderer, orders, showing);
        *shown = *version;
        drag.last = Some(place);
        let copied = walked.copied(focus.row, &acts, &pieces);
        // The press's row may have scrolled away: it's copied as it was,
        // taken anew only when the pointer goes to its other side.
        let side = focus.row.cmp(&anchor.row);
        match side {
            Ordering::Equal => (held.first, held.last) = (copied, String::new()),
            Ordering::Greater => {
                if drag.side != side {
                    held.first.clone_from(&drag.after);
                }
                held.last = copied;
            }
            Ordering::Less => {
                held.first = copied;
                if drag.side != side {
                    held.last.clone_from(&drag.before);
                }
            }
        }
        drag.side = side;
        held.selected = focus.row != anchor.row || !held.first.is_empty();
    }

    /// While the pointer is near the list's top or bottom edge or past it,
    /// and the window in front, scrolls the list a frame's worth and asks
    /// for the next frame.
    pub(super) fn scroll_at_edge(
        &mut self,
        state: &mut State,
        content: &mut Tree,
        layout: Layout,
        renderer: &Renderer,
        now: std::time::Instant,
        shell: &mut Shell<'_, Message>,
    ) {
        let Some(drag) = &mut state.drag else {
            return;
        };
        let speed = drag
            .scroller
            .as_ref()
            .filter(|_| self.motion.front)
            .map_or(0.0, |scroller| edge::speed(scroller.bounds, drag.pointer.y));
        if speed == 0.0 {
            drag.frame = None;
            return;
        }
        let elapsed = drag.frame.map_or(0.0, |last| {
            now.saturating_duration_since(last).as_secs_f32()
        });
        drag.frame = Some(now);
        let delta = edge::step(speed, elapsed);
        if delta != 0.0
            && let Some(scroller) = drag.scroller.clone()
        {
            let bounds = layout.bounds();
            let mut nudge = Nudge::new(scroller, delta);
            self.content
                .operate(content, layout, &bounds, renderer, &mut nudge);
            shell.invalidate_layout();
        }
        shell.request_redraw();
    }
}
