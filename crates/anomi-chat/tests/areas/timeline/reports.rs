//! What the list tells as it shows: the rows in view with six of margin,
//! whether it follows the newest, and whether to load more at an edge; a
//! long history built only near the view.

use std::time::{Duration, Instant};

use iced::{Point, mouse};

use anomi_vm::chat::timeline::Edge;
use anomi_vm::fixtures::chat::timeline::{self as fixtures, message_key};

use super::{SIZE, frame, reports, shown, shows, simulate};

#[test]
fn it_tells_the_rows_in_view_from_the_newest_and_follows_them() {
    let timeline = fixtures::long(1_000);
    let mut ui = shown(&timeline);
    frame(&mut ui, Instant::now());
    let told = reports(&mut ui);
    let [shown] = told.as_slice() else {
        panic!("one report: {told:?}");
    };
    let rows = timeline.rows.len();
    assert_eq!(shown.span.last, rows - 1);
    assert_eq!(shown.keys.last(), Some(&message_key(rows - 1)));
    assert!(shown.at_end && shown.following);
    // Six rows of margin above the first in view.
    let first = shown.span.first;
    assert_eq!(shown.keys.len(), rows - first.saturating_sub(6));
    assert_eq!(shown.top, timeline.rows[first].key);
    assert!(!shown.earlier, "far from the start");
}

#[test]
fn near_a_start_with_more_it_asks_for_earlier_messages() {
    for (edge, wants) in [
        (Edge::More, true),
        (Edge::Loading, false),
        (Edge::Failed, false),
        (Edge::Start, false),
    ] {
        let timeline = fixtures::start_of_general(edge);
        let mut ui = shown(&timeline);
        frame(&mut ui, Instant::now());
        let told = reports(&mut ui);
        assert_eq!(told.len(), 1, "{edge:?}");
        assert_eq!(told[0].earlier, wants, "{edge:?}");
        assert!(!told[0].later, "nothing newer: it reaches the newest");
    }
}

#[test]
fn a_window_of_older_messages_isnt_followed_and_asks_for_newer_ones() {
    let timeline = fixtures::older_window(Edge::More);
    let mut ui = shown(&timeline);
    frame(&mut ui, Instant::now());
    let told = reports(&mut ui);
    assert_eq!(told.len(), 1);
    assert!(told[0].later);
    assert!(!told[0].following);
}

#[test]
fn scrolling_up_stops_following_and_reports_at_most_every_50_ms() {
    let timeline = fixtures::long(1_000);
    let mut ui = simulate(&timeline, anomi_theme::Motion::default());
    let start = Instant::now();
    frame(&mut ui, start);
    assert_eq!(reports(&mut ui).len(), 1);
    ui.point_at(Point::new(SIZE.width / 2.0, SIZE.height / 2.0));
    let _ = ui.scroll(mouse::ScrollDelta::Pixels { x: 0.0, y: 2_000.0 });
    frame(&mut ui, start + Duration::from_millis(60));
    let told = reports(&mut ui);
    let [shown] = told.as_slice() else {
        panic!("one report: {told:?}");
    };
    assert!(!shown.following && !shown.at_end);
    assert!(shown.span.last < 999);
}

#[test]
fn a_thousand_messages_build_only_those_near_the_view() {
    let timeline = fixtures::long(1_000);
    let mut ui = shown(&timeline);
    frame(&mut ui, Instant::now());
    // The day over the first message is far above the view: never built.
    assert!(!shows(&mut ui, "1 October 2026"));
    let told = reports(&mut ui);
    // Until the messages (C02's) have their height, a row is its padding
    // alone, so a few hundred show; never the thousand.
    assert!(told[0].keys.len() < 500, "{} rows told", told[0].keys.len());
}
