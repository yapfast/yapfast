//! The timeline's view-model: when more may load, where the bar to the
//! "New" divider points, and when the conversation counts as read.

use super::*;

fn rows(count: usize, new: Option<usize>) -> Vec<Row> {
    (0..count)
        .map(|index| Row {
            key: index as u64,
            new: new == Some(index),
            ..Row::default()
        })
        .collect()
}

/// `count` rows keyed from `from`, as a page of other messages.
fn page(from: u64, count: u64) -> Vec<Row> {
    (from..from + count)
        .map(|key| Row {
            key,
            ..Row::default()
        })
        .collect()
}

/// Rows `first..=last` of `rows` in view, near both the list's edges.
fn near(rows: &[Row], first: usize, last: usize) -> Span {
    Span {
        near_start: true,
        near_end: true,
        ..Span::new(rows, first, last)
    }
}

#[test]
fn earlier_messages_load_only_while_there_are_more_and_nothing_holds_them() {
    let mut timeline = Timeline {
        ready: true,
        rows: rows(3, None),
        ..Timeline::default()
    };
    assert!(timeline.may_load_earlier());
    for edge in [Edge::Loading, Edge::Failed, Edge::Start] {
        timeline.earlier = edge;
        assert!(!timeline.may_load_earlier(), "{edge:?}");
    }
    timeline.earlier = Edge::More;
    timeline.locked = true;
    assert!(!timeline.may_load_earlier(), "a preview loads nothing");
    timeline.locked = false;
    timeline.ready = false;
    assert!(!timeline.may_load_earlier(), "not before the room is ready");
    assert!(!timeline.may_load_later(), "nothing newer to load");
    timeline.ready = true;
    timeline.later = Some(Edge::More);
    assert!(timeline.detached() && timeline.may_load_later());
    timeline.later = Some(Edge::Failed);
    assert!(!timeline.may_load_later(), "a failure waits for Retry");
}

#[test]
fn a_view_kept_near_an_edge_wants_more_once_it_may_load() {
    // The list told near the start while busy; it tells nothing more while
    // the view stays put.
    let mut timeline = Timeline {
        ready: true,
        locked: true,
        rows: rows(3, None),
        ..Timeline::default()
    };
    timeline.shown = Some(near(&timeline.rows, 0, 2));
    assert!(!timeline.wants_earlier(), "busy");
    timeline.locked = false;
    assert!(
        timeline.wants_earlier(),
        "busy ended, the view where it was"
    );
    assert!(!timeline.wants_later(), "it reaches the newest");
    // A failure goes back to More: it wants them again.
    timeline.earlier = Edge::Loading;
    assert!(!timeline.wants_earlier(), "on their way");
    timeline.earlier = Edge::More;
    timeline.later = Some(Edge::More);
    assert!(timeline.wants_earlier() && timeline.wants_later());
    timeline.shown = Some(Span::new(&timeline.rows, 0, 2));
    assert!(!timeline.wants_earlier() && !timeline.wants_later(), "far");
    timeline.shown = None;
    assert!(!timeline.wants_earlier(), "before the list told");
}

#[test]
fn the_bar_shows_while_the_divider_is_out_of_view_and_unread() {
    let mut timeline = Timeline {
        ready: true,
        rows: rows(100, Some(60)),
        unread: Some(Unread {
            label: "40 new messages".into(),
            caught_up: false,
        }),
        ..Timeline::default()
    };
    assert_eq!(timeline.unread_bar(), None, "before the list told");
    timeline.shown = Some(Span::new(&timeline.rows, 80, 99));
    assert_eq!(timeline.unread_bar(), Some(Out::Above));
    timeline.shown = Some(Span::new(&timeline.rows, 20, 40));
    assert_eq!(timeline.unread_bar(), Some(Out::Below));
    timeline.shown = Some(Span::new(&timeline.rows, 55, 70));
    assert_eq!(timeline.unread_bar(), None, "in view");
    timeline.shown = Some(Span::new(&timeline.rows, 80, 99));
    timeline.unread.as_mut().unwrap().caught_up = true;
    assert_eq!(timeline.unread_bar(), None, "read since");
}

#[test]
fn the_newest_followed_in_front_is_read() {
    let mut timeline = Timeline {
        room: Some(Room::default()),
        ready: true,
        rows: rows(10, None),
        ..Timeline::default()
    };
    let following = Shown {
        at_end: true,
        following: true,
        ..Shown::default()
    };
    assert!(timeline.reads(&following, true));
    assert!(!timeline.reads(&following, false), "behind another window");
    assert!(!timeline.reads(&Shown::default(), true), "scrolled up");
    // A jump stops following; placed at the end, it reads all the same.
    timeline.jump = Some(Jump { key: 5, lit: false });
    let placed = Shown {
        span: Span::new(&timeline.rows, 3, 9),
        at_end: true,
        ..Shown::default()
    };
    assert!(timeline.reads(&placed, true));
    // The list tells of its end a layout before the jump lands: the
    // divider's row out of view leaves the stretch unread.
    let unplaced = Shown {
        span: Span::new(&timeline.rows, 7, 9),
        at_end: true,
        ..Shown::default()
    };
    assert!(
        !timeline.reads(&unplaced, true),
        "the jump's row not yet in view"
    );
    timeline.later = Some(Edge::More);
    assert!(!timeline.reads(&placed, true), "a window of older ones");
}

#[test]
fn a_page_come_in_above_waits_for_the_list_to_tell_again() {
    let mut timeline = Timeline {
        ready: true,
        rows: rows(20, None),
        ..Timeline::default()
    };
    timeline.shown = Some(near(&timeline.rows, 0, 9));
    assert!(timeline.wants_earlier());
    // The page asked for came, and the edge is back to More: the report
    // was of the rows before it, so one page loads at a time.
    timeline.rows.splice(0..0, page(100, 30));
    assert!(!timeline.wants_earlier(), "a page in above since it told");
    timeline.shown = Some(near(&timeline.rows, 0, 9));
    assert!(
        timeline.wants_earlier(),
        "the list tells the view near again"
    );
}

#[test]
fn a_window_that_replaced_the_newest_waits_for_the_list_to_tell_of_it() {
    let mut timeline = Timeline {
        ready: true,
        rows: rows(20, None),
        ..Timeline::default()
    };
    timeline.shown = Some(near(&timeline.rows, 10, 19));
    // A reply's quote clicked: a window of older messages in their place.
    timeline.rows = page(100, 20);
    timeline.later = Some(Edge::More);
    assert!(!timeline.wants_later(), "the report was of the newest");
    assert!(!timeline.wants_earlier(), "nor of this window's start");
}

#[test]
fn a_page_come_in_above_leaves_the_divider_in_view() {
    let mut timeline = Timeline {
        ready: true,
        rows: rows(100, Some(60)),
        unread: Some(Unread::default()),
        ..Timeline::default()
    };
    timeline.shown = Some(Span::new(&timeline.rows, 55, 70));
    assert_eq!(timeline.unread_bar(), None);
    timeline.rows.splice(0..0, page(1_000, 50));
    assert_eq!(
        timeline.unread_bar(),
        None,
        "the divider moved with the view"
    );
    // Its rows gone, the span names nothing until the list tells again.
    timeline.rows = rows(100, Some(10));
    timeline.rows.iter_mut().for_each(|row| row.key += 500);
    assert_eq!(timeline.unread_bar(), None);
}

#[cfg(feature = "fixtures")]
#[test]
fn a_jump_loads_nothing_newer_until_its_row_is_in_view() {
    use crate::fixtures::chat::timeline::{message_key, older_window};
    let mut timeline = older_window(Edge::More);
    let at = |timeline: &Timeline, index| timeline.index_of(message_key(index)).unwrap();
    // The list at the window's end, before the jump reaches it.
    let last = timeline.rows.len() - 1;
    timeline.shown = Some(near(&timeline.rows, at(&timeline, 28), last));
    assert!(!timeline.wants_later(), "message 26 not placed yet");
    timeline.shown = Some(near(&timeline.rows, at(&timeline, 24), last));
    assert!(timeline.wants_later(), "message 26 in view");
    assert!(timeline.wants_earlier());
    timeline.jump = None;
    timeline.shown = Some(near(&timeline.rows, at(&timeline, 28), last));
    assert!(timeline.wants_later(), "no jump to wait for");
}
