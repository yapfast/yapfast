//! What the list tells, and what the pointer does to it beyond the wheel
//! and a drag: the report of the rows in view, autoscroll.

use std::time::{Duration, Instant};

use iced::{Point, mouse};

use super::super::{MARGIN, SPACING, Viewport, scroll_view};
use super::*;

#[test]
fn reports_the_rows_in_view_with_six_of_margin_at_most_every_50_ms() {
    let rows = Rows::even(100, 30.0);
    let mut ui = simulate(rows.list(front()).on_viewport(Message::Seen));
    let start = Instant::now();
    frame(&mut ui, start);
    let first = Viewport {
        first: 0,
        last: 19,
        keys: (0..=19 + MARGIN as u64).collect(),
        top: 0,
        near_start: true,
        near_end: false,
        at_end: false,
        following: false,
        offset: 0.0,
        height: VIEW.height,
    };
    assert_eq!(ui.drain().collect::<Vec<_>>(), vec![Message::Seen(first)]);
    scroll(&mut ui, 300.0);
    frame(&mut ui, start + Duration::from_millis(10));
    assert_eq!(ui.drain().count(), 0, "spaced: not yet");
    frame(&mut ui, start + Duration::from_millis(60));
    let seen: Vec<Message> = ui.drain().collect();
    let [Message::Seen(viewport)] = seen.as_slice() else {
        panic!("one report: {seen:?}");
    };
    assert_eq!((viewport.first, viewport.last, viewport.top), (10, 29, 10));
    assert_eq!(viewport.keys, (4..=35).collect::<Vec<u64>>());
    assert_eq!((viewport.offset, viewport.height), (300.0, VIEW.height));
    frame(&mut ui, start + Duration::from_millis(200));
    assert_eq!(ui.drain().count(), 0, "nothing new, nothing told");
    // Behind another window nothing scrolls: a change is told at once
    // rather than a frame being asked for later.
    let mut ui = ui.rebuild(rows.list(still()).on_viewport(Message::Seen));
    scroll(&mut ui, 300.0);
    frame(&mut ui, start + Duration::from_millis(210));
    assert_eq!(ui.drain().count(), 1);
}

#[test]
fn one_piece_that_scrolls_tells_how_far_it_is_scrolled_and_how_much_shows() {
    let piece = container(text("Piece")).height(2000).width(Fill);
    let mut ui = Simulator::with_size(
        Settings::default(),
        VIEW,
        scroll_view(piece, front())
            .on_viewport(Message::Seen)
            .boxed(),
    );
    let start = Instant::now();
    scroll(&mut ui, 300.0);
    frame(&mut ui, start);
    let seen: Vec<Message> = ui.drain().collect();
    let [Message::Seen(viewport)] = seen.as_slice() else {
        panic!("one report: {seen:?}");
    };
    assert_eq!((viewport.first, viewport.last), (0, 0), "one row, always");
    assert_eq!((viewport.offset, viewport.height), (300.0, 600.0));
    // Each pixel is news to it, its one row the same all along.
    scroll(&mut ui, 4.0);
    frame(&mut ui, start + SPACING);
    let seen: Vec<Message> = ui.drain().collect();
    let [Message::Seen(viewport)] = seen.as_slice() else {
        panic!("one report: {seen:?}");
    };
    assert_eq!(viewport.offset, 304.0);
}

#[test]
fn a_pixel_scroll_within_the_same_rows_tells_nothing_new() {
    // 18.75 rows in the view: a few pixels change none of those in it.
    let rows = Rows::even(100, 32.0);
    let mut ui = simulate(rows.list(front()).on_viewport(Message::Seen));
    let start = Instant::now();
    frame(&mut ui, start);
    assert_eq!(ui.drain().count(), 1, "the first report");
    for step in 1..=4 {
        scroll(&mut ui, 1.0);
        frame(&mut ui, start + SPACING * step);
    }
    assert_eq!(ui.drain().count(), 0, "rows 0 to 18 in view all along");
    // Row 19 comes into view: news, with the offset as it is then.
    scroll(&mut ui, 6.0);
    frame(&mut ui, start + SPACING * 5);
    let seen: Vec<Message> = ui.drain().collect();
    let [Message::Seen(viewport)] = seen.as_slice() else {
        panic!("one report: {seen:?}");
    };
    assert_eq!((viewport.first, viewport.last), (0, 19));
    assert_eq!(viewport.offset, 10.0);
}

#[test]
fn a_list_following_its_end_says_so() {
    let rows = Rows::even(100, 30.0);
    let mut ui = simulate(rows.list(still()).from_end().on_viewport(Message::Seen));
    frame(&mut ui, Instant::now());
    let seen: Vec<Message> = ui.drain().collect();
    let [Message::Seen(viewport)] = seen.as_slice() else {
        panic!("one report: {seen:?}");
    };
    assert!(viewport.following && viewport.at_end && viewport.near_end);
    assert_eq!(viewport.last, 99);
}

#[test]
fn a_middle_click_autoscrolls_towards_the_pointer_until_the_next_press() {
    let rows = Rows::even(1000, 30.0);
    let mut ui = simulate(rows.list(front()));
    ui.point_at(Point::new(200.0, 300.0));
    let _ = ui.simulate([press(mouse::Button::Middle), release(mouse::Button::Middle)]);
    ui.point_at(Point::new(200.0, 450.0));
    let _ = ui.simulate([moved_to(Point::new(200.0, 450.0))]);
    let start = Instant::now();
    frame(&mut ui, start);
    frame(&mut ui, start + Duration::from_millis(50));
    assert!(top(&mut ui, 0).is_none_or(|top| top < 0.0), "scrolled down");
    let statuses = ui.simulate([press(mouse::Button::Left), release(mouse::Button::Left)]);
    assert!(
        statuses
            .iter()
            .all(|status| *status == iced::event::Status::Captured),
        "the press that ends it goes no further"
    );
    let shown = (0..1000).find(|key| top(&mut ui, *key).is_some_and(|top| top >= 0.0));
    let held = shown.and_then(|key| top(&mut ui, key));
    frame(&mut ui, start + Duration::from_millis(100));
    assert_eq!(shown.and_then(|key| top(&mut ui, key)), held, "it stopped");
}

#[test]
fn an_autoscroll_ends_when_the_window_loses_the_focus() {
    let rows = Rows::even(1000, 30.0);
    let mut ui = simulate(rows.list(front()));
    ui.point_at(Point::new(200.0, 300.0));
    let _ = ui.simulate([press(mouse::Button::Middle), release(mouse::Button::Middle)]);
    let _ = ui.simulate([moved_to(Point::new(200.0, 500.0))]);
    let _ = ui.simulate([Event::Window(window::Event::Unfocused)]);
    let start = Instant::now();
    frame(&mut ui, start);
    frame(&mut ui, start + Duration::from_millis(50));
    assert_eq!(top(&mut ui, 0), Some(0.0));
}

#[test]
fn a_middle_click_in_a_list_that_fits_does_nothing() {
    let rows = Rows::even(5, 30.0);
    let mut ui = simulate(rows.list(front()));
    ui.point_at(Point::new(200.0, 50.0));
    let statuses = ui.simulate([press(mouse::Button::Middle)]);
    assert_eq!(statuses, vec![iced::event::Status::Ignored]);
}

#[test]
fn behind_another_window_a_middle_click_starts_nothing_and_a_sideways_wheel_passes() {
    let rows = Rows::even(1000, 30.0);
    let mut ui = simulate(rows.list(still()));
    ui.point_at(Point::new(200.0, 300.0));
    let statuses = ui.simulate([press(mouse::Button::Middle)]);
    assert_eq!(statuses, vec![iced::event::Status::Ignored]);
    let sideways = ui.scroll(mouse::ScrollDelta::Lines { x: 1.0, y: 0.0 });
    assert_eq!(sideways, iced::event::Status::Ignored);
}
