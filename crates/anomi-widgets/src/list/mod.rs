//! The virtual list: rows of any height, built only near the view, so a
//! list of thousands costs what the few dozen on screen cost. The Slint
//! side: `ListView` and `ScrollView` in `ui/scrollview.slint:110`, `:168`,
//! the repeater's virtualization and anchoring in `vendor/i-slint-core`
//! (`model/repeater.rs`, `yapfast_anchor.rs`, `yapfast_autoscroll.rs`), and
//! the timeline's behaviour in `ui/yapfast.slint` (following the newest,
//! jumping to a message at a third of the height, a short history pinned
//! to the composer).
//!
//! - **Rows.** [`list`] takes how many rows there are, each row's key (its
//!   identity: a message's id, not its index) and a function that builds
//!   row `index`. Each layout builds the rows that cover the view and a
//!   quarter of it beyond, measures them, and keeps their state by key as
//!   they come and go; rows never built cost a height in a cache. A row's
//!   height is measured each time it's built, and estimated by the average
//!   of those measured until then. [`scroll_view`] and [`List::with_rows`]
//!   take rows already built, for content that is short or one piece.
//! - **Anchoring.** The list holds its place by a row's key: rows that go
//!   in above the view (older messages loading), or grow there, leave what
//!   shows where it is. Rows that go in right above the first row in view
//!   leave its bottom edge where it was, as its top (a date, a loading
//!   notice) can change with them.
//! - **The end.** A list [laid out from its end](List::from_end) starts at
//!   its last row and follows it: rows that come or grow there stay in view,
//!   until the person scrolls away; reaching the end again follows it again.
//!   Rows shorter than the view sit at its end, against the composer. One
//!   [`follow`](List::follow)ing nothing (a window of older messages) holds
//!   still at its end too.
//! - **Scrolling.** A wheel's notch glides 60 px over [`motion::SLIDE`]
//!   (instantly while [`Motion`] is still), a notch while one glides adds to
//!   what's left, and a wheel spun fast goes farther; a touchpad scrolls as
//!   it moves. The [`scrollbar`](crate::scrollbar) shows over the rows'
//!   right edge. A middle click autoscrolls.
//! - **The pointer.** Rows that move under a still pointer (a glide, a
//!   jump, rows coming above) hear it where it now is at the next frame, as
//!   if it moved, so what's lit under it follows. [`List::on_leave`] tells
//!   as the pointer leaves the list, off it or onto a layer over it; over a
//!   row's overlay (a hover bar) it's still in.
//! - **Asking.** [`operation`] jumps to a row by key (at the top, a third of
//!   the way down, or just into view), scrolls to either end or by some
//!   pixels; iced's own `scrollable` operations work on it too, by its id.
//! - **Telling.** [`List::on_viewport`] reports the rows in view with six of
//!   margin, and where the view is, as they change, at most every 50 ms:
//!   a pixel's scroll that changes no row in view tells nothing, except to
//!   one piece that scrolls or a list that [asks for
//!   pixels](List::report_pixels).
//! - **Dragging.** [`List::reorder`] lets rows be dragged to a new place,
//!   or, [without the line](List::drop_line), onto another row the owner
//!   marks as the pointer goes over it. The dragged row's faint copy
//!   follows the pointer on a [ground](List::drag_ground) its owner shapes:
//!   the row itself, or [one the owner builds](List::drag_copy). A list
//!   that [makes room](List::live) shows the move itself: the rows slide
//!   apart where it would land, and a row may go onto another.
//!
//! Nothing ticks while nothing moves: frames are asked for only while a
//! glide, an autoscroll or a drag near an edge goes on, none while the
//! window is behind another (where a report goes at once), and a report
//! waits for at most one frame.
//!
//! A timeline, for instance:
//!
//! ```ignore
//! list(vm.rows.len(), |at| vm.rows[at].key, |at| message::view(&vm.rows[at]), motion)
//!     .id(Id::from(format!("timeline-{}", vm.room))) // another room starts afresh
//!     .from_end()
//!     .follow(!vm.detached)
//!     .on_viewport(Intent::Viewport)
//! // and in the app: list::operation::jump_to(id, key, Place::Third)
//! ```
//!
//! [`motion::SLIDE`]: anomi_theme::motion::SLIDE

mod anchor;
mod autoscroll;
mod build;
mod drag;
mod events;
mod ghost;
mod glide;
mod heights;
mod live;
pub mod operation;
mod place;
mod report;
mod rows;
mod slide;
mod state;
mod widget;

#[cfg(test)]
mod tests;

use iced::advanced::widget::Id;
use iced::{Length, Padding};

use anomi_theme::prelude::*;

use build::Source;
pub use drag::{Drag, Reorder};
pub use ghost::{Fill, Ground};
pub use glide::NOTCH;
pub use heights::ESTIMATE;
pub use live::{DWELL, Zone};
pub use report::{MARGIN, SPACING, Viewport};

/// Where a jump puts its row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// At the top of the view, as the first unread message.
    Top,
    /// A third of the way down, as a message a reply's click jumps to.
    Third,
    /// Just into view, this far from the edge it was past; where it is when
    /// it shows already. A message the keyboard steps to, or one being
    /// edited as its field grows.
    Reveal(f32),
}

pub struct List<'a, Message> {
    source: Source<'a, Message>,
    /// The rows this widget built at its last layout, from `built_first`,
    /// reused as they are if it lays out again before the next view.
    built: Vec<Element<'a, Message>>,
    built_first: usize,
    /// The dragged row, built apart while it's out of `built`, or the copy
    /// the owner builds for it.
    ghost: Option<Element<'a, Message>>,
    id: Option<Id>,
    width: Length,
    height: Length,
    padding: Padding,
    spacing: f32,
    estimate: f32,
    from_end: bool,
    short_at_start: bool,
    follow: bool,
    reveal: Option<(u64, f32)>,
    on_viewport: Option<Box<dyn Fn(Viewport) -> Message + 'a>>,
    on_leave: Option<Box<dyn Fn() -> Message + 'a>>,
    /// A change of the view's offset or height alone is reported too.
    pixels: bool,
    reorder: Option<Reorder<'a, Message>>,
    scrollbar: bool,
    motion: Motion,
}

/// A list of `count` rows: `key` gives row `index`'s identity, which must
/// differ between rows and stay with a row as others come and go, and `row`
/// builds it. Only the rows near the view are built, at each layout.
pub fn list<'a, Message, Row>(
    count: usize,
    key: impl Fn(usize) -> u64 + 'a,
    row: impl Fn(usize) -> Row + 'a,
    motion: Motion,
) -> List<'a, Message>
where
    Message: 'a,
    Row: iced::Widget<Message, Theme, anomi_theme::Renderer> + 'a,
{
    List::new(
        Source::Lazy {
            count,
            key: Box::new(key),
            row: Box::new(move |index| row(index).boxed()),
        },
        motion,
    )
}

/// One piece of content that scrolls, as Slint's `ScrollView`: the list's
/// scrollbar, wheel and autoscroll over anything taller than its place.
pub fn scroll_view<'a, Message: 'a>(
    content: impl iced::Widget<Message, Theme, anomi_theme::Renderer> + 'a,
    motion: Motion,
) -> List<'a, Message> {
    List::with_rows([(0, content.boxed())], motion).report_pixels(true)
}

impl<'a, Message> List<'a, Message> {
    fn new(source: Source<'a, Message>, motion: Motion) -> Self {
        Self {
            source,
            built: Vec::new(),
            built_first: 0,
            ghost: None,
            id: None,
            width: Length::Fill,
            height: Length::Fill,
            padding: Padding::ZERO,
            spacing: 0.0,
            estimate: ESTIMATE,
            from_end: false,
            short_at_start: false,
            follow: true,
            reveal: None,
            on_viewport: None,
            on_leave: None,
            pixels: false,
            reorder: None,
            scrollbar: true,
            motion,
        }
    }

    /// A list of rows built already, each with its key: all of them are
    /// laid out, for a short list or one piece of content.
    pub fn with_rows(
        rows: impl IntoIterator<Item = (u64, Element<'a, Message>)>,
        motion: Motion,
    ) -> Self {
        let (keys, rows): (Vec<u64>, Vec<Option<Element<'a, Message>>>) =
            rows.into_iter().map(|(key, row)| (key, Some(row))).unzip();
        Self::new(Source::Eager { keys, rows }, motion)
    }

    /// Names the list for [`operation`]s. A list given another id starts
    /// afresh, as for another room's messages.
    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Space around the rows, which scrolls with them.
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Space between two rows.
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// The height a row is taken to be before any was measured.
    pub fn estimate(mut self, height: f32) -> Self {
        self.estimate = height;
        self
    }

    /// Whether the scrollbar shows when the rows are taller than the view;
    /// on by default, off for a picker's rail (`ScrollBarPolicy.always-off`).
    pub fn scrollbar(mut self, shown: bool) -> Self {
        self.scrollbar = shown;
        self
    }

    /// Lays the rows out from the last, which starts in view and is
    /// followed; rows shorter than the view sit at its end.
    pub fn from_end(mut self) -> Self {
        self.from_end = true;
        self
    }

    /// Laid out from its end, rows shorter than the view sit at its start
    /// rather than its end: the short history of a thread under its header
    /// (`content-y = min(0, height - content-height)`, `ui/thread.slint:136`).
    pub fn short_at_start(mut self) -> Self {
        self.short_at_start = true;
        self
    }

    /// Whether the end of a list laid out from its end is followed when
    /// reached: not for a window of older messages, whose end isn't the
    /// newest. On by default.
    pub fn follow(mut self, follow: bool) -> Self {
        self.follow = follow;
        self
    }

    /// Keeps the row keyed `key` in view, `margin` from the edges, when it
    /// first shows here and as its height changes: a message whose editing
    /// field opens and grows.
    pub fn reveal(mut self, key: Option<u64>, margin: f32) -> Self {
        self.reveal = key.map(|key| (key, margin));
        self
    }

    /// Reports the rows in view, and where the view is, as they change, at
    /// most every [`SPACING`].
    pub fn on_viewport(mut self, report: impl Fn(Viewport) -> Message + 'a) -> Self {
        self.on_viewport = Some(Box::new(report));
        self
    }

    /// Sends `left` as the pointer leaves the list: off its bounds, or onto
    /// a layer over it. On an overlay of a row's, a hover bar, it's still in.
    pub fn on_leave(mut self, left: impl Fn() -> Message + 'a) -> Self {
        self.on_leave = Some(Box::new(left));
        self
    }

    /// Whether a scroll that changes no row in view is reported too, with
    /// the view's [offset](Viewport::offset) and height: for a picker's grid
    /// that loads what shows by pixels. Off for a list of rows, on for a
    /// [`scroll_view`].
    pub fn report_pixels(mut self, pixels: bool) -> Self {
        self.pixels = pixels;
        self
    }

    /// Lets rows be dragged to another place: `may_drag` says whether row
    /// `index` may be picked up, `may_drop` whether the row from `from` may
    /// drop on `to`'s place, and `on_drag` tells what happened.
    pub fn reorder(
        mut self,
        may_drag: impl Fn(usize) -> bool + 'a,
        may_drop: impl Fn(usize, usize) -> bool + 'a,
        on_drag: impl Fn(Drag) -> Message + 'a,
    ) -> Self {
        self.reorder = Some(Reorder {
            may_drag: Box::new(may_drag),
            may_drop: Box::new(may_drop),
            on_drag: Box::new(on_drag),
            handle: None,
            line: true,
            ground: Box::new(|_| Ground::default()),
            copy: None,
            onto: None,
        });
        self
    }

    /// The ground under the faint copy of the row dragged from each index,
    /// where it differs from the rows' own ([`Ground::default`]): a
    /// category's 36 px tall with 4 px corners, a person's dragged onto
    /// voice channels 40 px in from the left and 30 px tall, a role on
    /// [`Fill::Raised`] across the row with 6 px corners, opaque
    /// (`ui/rail.slint:1613`-`:1641`, `ui/roles.slint:157`). N04 builds the
    /// category's and the person's copy with [`Self::drag_copy`], laid out
    /// in this ground. Needs [`Self::reorder`].
    pub fn drag_ground(mut self, ground: impl Fn(usize) -> Ground + 'a) -> Self {
        if let Some(reorder) = &mut self.reorder {
            reorder.ground = Box::new(ground);
        }
        self
    }

    /// Builds the copy of the row dragged from each index, where it isn't
    /// the row itself: a category's chevron and heading, centred in its
    /// 36 px (`ui/rail.slint:1632`-`:1641`), or a person dragged onto voice
    /// channels in the text colour (`:1617`-`:1623`). It's laid out in the
    /// [ground](Self::drag_ground), the row's width less its insets and the
    /// ground's height (the row's when it has none), built apart at each
    /// layout while the drag lasts, and drawn faded on the ground. Without
    /// it the copy is the row. Needs [`Self::reorder`].
    pub fn drag_copy(mut self, copy: impl Fn(usize) -> Element<'a, Message> + 'a) -> Self {
        if let Some(reorder) = &mut self.reorder {
            reorder.copy = Some(Box::new(copy));
        }
        self
    }

    /// Whether a line shows where the dragged row would drop; on by
    /// default. Off, the rows are targets, as voice channels someone is
    /// dragged into: the owner marks the one [`Drag::Over`] names
    /// (`move-aimed`, `ui/rail.slint:1474`), and it drops only while the
    /// pointer is over the list across as well as down. Needs
    /// [`Self::reorder`].
    pub fn drop_line(mut self, shown: bool) -> Self {
        if let Some(reorder) = &mut self.reorder {
            reorder.line = shown;
        }
        self
    }

    /// Only a press this near a row's left edge picks it up, as a role is
    /// dragged by its handle. Needs [`Self::reorder`].
    pub fn drag_handle(mut self, width: f32) -> Self {
        if let Some(reorder) = &mut self.reorder {
            reorder.handle = Some(width);
        }
        self
    }
}
