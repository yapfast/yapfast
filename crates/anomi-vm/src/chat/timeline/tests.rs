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
    timeline.shown = Some(Span {
        first: 80,
        last: 99,
    });
    assert_eq!(timeline.unread_bar(), Some(Out::Above));
    timeline.shown = Some(Span {
        first: 20,
        last: 40,
    });
    assert_eq!(timeline.unread_bar(), Some(Out::Below));
    timeline.shown = Some(Span {
        first: 55,
        last: 70,
    });
    assert_eq!(timeline.unread_bar(), None, "in view");
    timeline.shown = Some(Span {
        first: 80,
        last: 99,
    });
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
        at_end: true,
        ..Shown::default()
    };
    assert!(timeline.reads(&placed, true));
    timeline.later = Some(Edge::More);
    assert!(!timeline.reads(&placed, true), "a window of older ones");
}
