//! What reaches the rows from outside: iced's window-wide selection and
//! copy don't, the words do (a test finds a row by them), and [`clear`]
//! does. The list scrolls at its edges only while the window is in front.
//!
//! [`clear`]: super::super::clear

use iced::advanced::widget::operation::{self, Operation, Outcome};
use iced::{Point, mouse};

use super::*;

/// What iced's own copy finds selected in the window.
fn window_copy(ui: &mut Ui<'_>) -> String {
    let mut copy = operation::text::copy();
    ui.operate(&mut operation::black_box(&mut copy));
    match copy.finish() {
        Outcome::Some(copied) => copied,
        _ => String::new(),
    }
}

#[test]
fn the_windows_own_selection_leaves_the_rows_alone() {
    let chat = Chat::new(20, &[("Sam", "First."), ("Riley", "Second.")]);
    let mut ui = simulate(chat.view(front()));
    ui.operate(&mut operation::text::select_all());
    assert_eq!(window_copy(&mut ui), "", "nothing selected by the window");
    assert!(ui.find("Second.").is_ok(), "the words still read");

    // Its own selection shows, yet iced's copy doesn't take it twice.
    let second = bounds(&mut ui, "Second.");
    drag(&mut ui, end_of(second), &[Point::new(300.0, 4.0)]);
    assert_eq!(window_copy(&mut ui), "");
    assert!(copy(&mut ui).is_some());
}

#[test]
fn clear_takes_the_selection_away() {
    let chat = Chat::new(20, &[("Sam", "First."), ("Riley", "Second.")]);
    let mut ui = simulate(chat.view(front()));
    let second = bounds(&mut ui, "Second.");
    drag(&mut ui, end_of(second), &[Point::new(300.0, 4.0)]);
    ui.operate(&mut super::super::forget(Id::new(SELECTION)));
    assert_eq!(copy(&mut ui), None);
}

/// The top of the newest message after a drag held past the list's top
/// for a second of frames, with the window in front or not.
fn held_past_the_top(front: bool) -> (f32, f32) {
    let chat = Chat::many(40);
    let motion = Motion {
        front,
        ..Motion::default()
    };
    let mut ui = simulate(chat.view(motion));
    let lowest = bounds(&mut ui, "Message 39.");
    move_to(&mut ui, end_of(lowest));
    press(&mut ui, mouse::Button::Left);
    move_to(&mut ui, Point::new(300.0, -40.0));
    frames(&mut ui, Instant::now(), Duration::from_millis(16), 60);
    let after = ui
        .find("Message 39.")
        .map_or(f32::INFINITY, |found| found.bounds().y);
    release(&mut ui, mouse::Button::Left);
    (lowest.y, after)
}

#[test]
fn the_list_scrolls_at_its_edges_only_in_front() {
    let (before, after) = held_past_the_top(true);
    assert!(
        after > before + 100.0,
        "it scrolled up: {before} to {after}"
    );
    let (before, after) = held_past_the_top(false);
    assert_eq!(before, after, "behind another window it holds still");
}
