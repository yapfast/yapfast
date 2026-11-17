//! The bars over the messages: how much is new while the "New" divider is
//! out of view, and the way back from a window of older messages.

use anomi_vm::chat::timeline::{Edge, Intent, Span, Timeline, Unread};
use anomi_vm::fixtures::chat::timeline as fixtures;

use super::{asked, shown, shows};

const LABEL: &str = "5 new messages since 16:55";

#[test]
fn the_unread_bar_jumps_to_the_divider_while_its_out_of_view() {
    let timeline = fixtures::unread_above();
    let mut ui = shown(&timeline);
    ui.click(LABEL).expect("the unread bar");
    assert_eq!(asked(&mut ui), [Intent::JumpToUnread]);
    // Below the view too, as after scrolling up past it.
    let timeline = Timeline {
        shown: Some(Span { first: 0, last: 10 }),
        ..fixtures::general()
    };
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, LABEL));
}

#[test]
fn no_unread_bar_while_the_divider_shows_or_once_the_newest_is_read() {
    let timeline = fixtures::general();
    let mut ui = shown(&timeline);
    assert!(!shows(&mut ui, LABEL), "the divider is in view");
    let timeline = Timeline {
        unread: Some(Unread {
            caught_up: true,
            ..fixtures::unread_above().unread.unwrap()
        }),
        ..fixtures::unread_above()
    };
    let mut ui = shown(&timeline);
    assert!(!shows(&mut ui, LABEL), "read since");
}

#[test]
fn a_window_of_older_messages_offers_the_way_back() {
    let timeline = fixtures::older_window(Edge::More);
    let mut ui = shown(&timeline);
    assert!(shows(&mut ui, "You're viewing older messages"));
    ui.click("Jump to present").expect("the present bar");
    assert_eq!(asked(&mut ui), [Intent::JumpToPresent]);
    let timeline = fixtures::general();
    let mut ui = shown(&timeline);
    assert!(!shows(&mut ui, "Jump to present"), "at the newest");
}
