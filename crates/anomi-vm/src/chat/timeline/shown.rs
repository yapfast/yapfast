//! What the list last told of the rows in view, and what the timeline
//! makes of it: whether to load more at an edge, where the "New" divider
//! is, whether the conversation is read.

use std::ops::RangeInclusive;

use super::{Edge, Row, Timeline};

/// The rows in view, and the edges near them, as the list last told. It
/// names its rows by key: rows put in or taken out since move their
/// indices, and the list tells again only on its next frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    /// The first and last row in view, by index in the rows as they were.
    pub first: usize,
    pub last: usize,
    /// The keys of the first row in view, to come back to, and of the last.
    pub top: u64,
    pub bottom: u64,
    /// The keys of the list's oldest and newest rows: once another stands
    /// there, a page came in at that edge since, and its nearness is old.
    pub oldest: u64,
    pub newest: u64,
    /// The list's start is less than a view's height above the view.
    pub near_start: bool,
    /// Its end is less than a view's height below.
    pub near_end: bool,
}

impl Span {
    /// Rows `first..=last` of `rows` in view, named by key; far from the
    /// list's edges.
    pub fn new(rows: &[Row], first: usize, last: usize) -> Self {
        let key = |row: Option<&Row>| row.map_or(0, |row| row.key);
        Self {
            first,
            last,
            top: key(rows.get(first)),
            bottom: key(rows.get(last)),
            oldest: key(rows.first()),
            newest: key(rows.last()),
            near_start: false,
            near_end: false,
        }
    }
}

/// What the list tells as its rows in view change, at most every 50 ms.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    pub span: Span,
    /// The keys of the rows in view and of six rows each side, in order:
    /// the messages to load pictures for and to count as seen.
    pub keys: Vec<u64>,
    /// The newest row's bottom is in view.
    pub at_end: bool,
    /// The list follows its end: rows that come stay in view, which reads
    /// the conversation while the window is in front
    /// (`timeline-at-end`, `reading-latest`).
    pub following: bool,
}

impl Timeline {
    /// Earlier messages may load when the list nears its start
    /// (`wants-earlier`, `ui/yapfast.slint:1922`).
    pub fn may_load_earlier(&self) -> bool {
        self.ready && !self.locked && !self.rows.is_empty() && self.earlier == Edge::More
    }

    /// Newer ones may load as a window of older ones nears its end
    /// (`wants-later`, `:1930`).
    pub fn may_load_later(&self) -> bool {
        self.ready && !self.locked && !self.rows.is_empty() && self.later == Some(Edge::More)
    }

    /// The view nears the start as the list last told, of the rows it told
    /// of, and earlier messages may load: load them. A page come in above
    /// since waits for the list to tell where the view is now, so one page
    /// loads at a time.
    pub fn wants_earlier(&self) -> bool {
        self.may_load_earlier()
            && self.shown.is_some_and(|span| {
                span.near_start
                    && self.rows.first().map(|row| row.key) == Some(span.oldest)
                    && self.placed(span)
            })
    }

    /// The view nears the end of a window of older messages, of the rows the
    /// list told of, and newer ones may load: load them. A window that has
    /// just replaced the rows waits for the list to tell of it.
    pub fn wants_later(&self) -> bool {
        self.may_load_later()
            && self.shown.is_some_and(|span| {
                span.near_end
                    && self.rows.last().map(|row| row.key) == Some(span.newest)
                    && self.placed(span)
            })
    }

    /// No jump waits for its row to be placed: none is under way, or its row
    /// is in `span`. A window opens with its list at its end until the jump
    /// reaches it, a layout after the list first tells, and nothing loads
    /// at an edge or counts as read before (`jump-placed`,
    /// `ui/yapfast.slint:1924`).
    fn placed(&self, span: Span) -> bool {
        let Some(jump) = self.jump else {
            return true;
        };
        self.rows_in(span)
            .zip(self.index_of(jump.key))
            .is_some_and(|(view, row)| view.contains(&row))
    }

    /// The rows in view as the list last told, by index in the rows now:
    /// none before it told, or once a row it named has gone.
    pub fn in_view(&self) -> Option<RangeInclusive<usize>> {
        self.rows_in(self.shown?)
    }

    /// The rows `span` names, by index in the rows now: none once one of
    /// them has gone.
    fn rows_in(&self, span: Span) -> Option<RangeInclusive<usize>> {
        Some(self.index_of(span.top)?..=self.index_of(span.bottom)?)
    }

    /// The row the "New" divider is over, by index (`unread-index`).
    pub fn unread_index(&self) -> Option<usize> {
        self.rows.iter().position(|row| row.new)
    }

    /// Where the "New" divider is while the bar that jumps to it shows: out
    /// of view above or below, until the newest message is read
    /// (`unread-above`, `unread-below`, `ui/yapfast.slint:1917`).
    pub fn unread_bar(&self) -> Option<Out> {
        self.unread.as_ref().filter(|unread| !unread.caught_up)?;
        let index = self.unread_index()?;
        let view = self.in_view()?;
        if index < *view.start() {
            Some(Out::Above)
        } else if index > *view.end() {
            Some(Out::Below)
        } else {
            None
        }
    }

    /// The index of the row with `key`.
    pub fn index_of(&self, key: u64) -> Option<usize> {
        self.rows.iter().position(|row| row.key == key)
    }

    /// Whether the person reads the conversation as `shown` shows it, the
    /// window in `front`: the newest message in view and followed, or a
    /// jump's message placed in view with the list at the newest's end, as
    /// a short unread stretch's divider is (`reading-latest`,
    /// `ui/yapfast.slint:1818`). The list tells of its end before the jump
    /// lands, so a long stretch's divider out of view leaves it unread. The
    /// app tells the core when this changes.
    pub fn reads(&self, shown: &Shown, front: bool) -> bool {
        let at_newest = match self.jump {
            None => shown.following,
            Some(_) => !self.detached() && shown.at_end && self.placed(shown.span),
        };
        front && self.room.is_some() && at_newest
    }
}

/// Where something out of view is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Above,
    Below,
}
