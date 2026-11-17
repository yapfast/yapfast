//! What shows the selection: rows built anew, as the list scrolls back to
//! them, show it again, and a layout touches no other; a frame selects
//! anew only where the pointer or the rows moved; a click takes it off
//! every row. Read from the names' marks, which the same walk covers as it
//! selects the texts, and from the selection's state.

use std::any::Any;

use iced::advanced::widget::operation::Operation;
use iced::{Point, mouse};

use super::super::marks::{Cover, Kind};
use super::super::model::State;
use super::*;
use crate::list::operation as scroll;

/// What `read` reads of the selection's state.
fn state<T: Default + Send + 'static>(ui: &mut Ui<'_>, read: fn(&State) -> T) -> T {
    struct Read<T>(fn(&State) -> T, T);
    impl<T: Send + 'static> Operation for Read<T> {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn custom(&mut self, _id: Option<&Id>, _bounds: Rectangle, state: &mut dyn Any) {
            if let Some(state) = state.downcast_ref::<State>() {
                self.1 = (self.0)(state);
            }
        }
    }
    let mut found = Read(read, T::default());
    ui.operate(&mut found);
    found.1
}

/// Whether each name shown is covered, top to bottom.
fn names(ui: &mut Ui<'_>) -> Vec<bool> {
    struct Names(Vec<(f32, bool)>);
    impl Operation for Names {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn custom(&mut self, _id: Option<&Id>, bounds: Rectangle, state: &mut dyn Any) {
            if let Some(cover) = state.downcast_ref::<Cover>()
                && cover.kind == Kind::Whole
                && bounds.y >= 0.0
                && bounds.y < SIZE.height
            {
                self.0.push((bounds.y, cover.on));
            }
        }
    }
    let mut names = Names(Vec::new());
    ui.operate(&mut names);
    names.0.sort_by(|a, b| a.0.total_cmp(&b.0));
    names.0.into_iter().map(|(_, on)| on).collect()
}

#[test]
fn rows_built_anew_show_the_selection_again() {
    let chat = Chat::many(40);
    let mut ui = simulate(chat.view(front()));
    let lowest = bounds(&mut ui, "Message 39.");
    move_to(&mut ui, end_of(lowest));
    press(&mut ui, mouse::Button::Left);
    move_to(&mut ui, Point::new(300.0, -200.0));
    frames(&mut ui, Instant::now(), Duration::from_millis(50), 80);
    release(&mut ui, mouse::Button::Left);
    let top = names(&mut ui);
    assert!(!top.is_empty() && top.iter().all(|on| *on), "{top:?}");
    // Back to the newest: those rows were dropped and are built again.
    ui.operate(&mut scroll::end::<()>(Id::new(LIST)));
    frames(&mut ui, Instant::now(), Duration::from_millis(16), 4);
    assert!(ui.find("Message 39.").is_ok(), "back at the end");
    let bottom = names(&mut ui);
    assert!(
        !bottom.is_empty() && bottom.iter().all(|on| *on),
        "{bottom:?}"
    );
    // A click on nothing takes it off them.
    click(&mut ui, Point::new(300.0, 4.0));
    let cleared = names(&mut ui);
    assert!(cleared.iter().all(|on| !on), "{cleared:?}");
}

#[test]
fn a_selection_within_a_row_covers_only_what_it_runs_through() {
    let chat = Chat::new(
        20,
        &[("Sam", "First."), ("Riley", "Second."), ("Sam", "Third.")],
    );
    let mut ui = simulate(chat.view(front()));
    let second = bounds(&mut ui, "Second.");
    let third = bounds(&mut ui, "Third.");
    // From the middle row's text down to the last row's: Riley's name is
    // before the press, the last Sam's after it.
    drag(
        &mut ui,
        Point::new(second.x + 2.0, second.center_y()),
        &[end_of(third)],
    );
    assert_eq!(names(&mut ui), [false, false, true]);
    assert_eq!(copy(&mut ui).as_deref(), Some("Second.\nSam 19:22\nThird."));
}

#[test]
fn a_layout_shows_the_selection_only_in_rows_built_anew() {
    let chat = Chat::new(
        20,
        &[("Sam", "First."), ("Riley", "Second."), ("Sam", "Third.")],
    );
    let older = Chat::new(
        18,
        &[
            ("Alex", "Older."),
            ("Alex", "Oldest."),
            ("Sam", "First."),
            ("Riley", "Second."),
            ("Sam", "Third."),
        ],
    );
    let mut ui = simulate(chat.view(front()));
    let second = bounds(&mut ui, "Second.");
    let third = bounds(&mut ui, "Third.");
    drag(
        &mut ui,
        end_of(third),
        &[Point::new(second.x - 4.0, second.center_y())],
    );
    // The same view again, as after any update: no row's texts are touched.
    let mut ui = ui.rebuild(chat.view(front()));
    assert_eq!(state(&mut ui, |state| state.reshown), 0);
    assert_eq!(names(&mut ui), [false, false, true]);
    // Two older rows built anew above: only they are.
    let mut ui = ui.rebuild(older.view(front()));
    assert_eq!(state(&mut ui, |state| state.reshown), 2);
    let mut ui = ui.rebuild(older.view(front()));
    assert_eq!(state(&mut ui, |state| state.reshown), 0);
    assert_eq!(names(&mut ui), [false, false, false, false, true]);
    assert_eq!(copy(&mut ui).as_deref(), Some("Second.\nSam 19:22\nThird."));
}

#[test]
fn a_frame_selects_anew_only_where_the_pointer_or_the_rows_moved() {
    let chat = Chat::many(40);
    let mut ui = simulate(chat.view(front()));
    let lowest = bounds(&mut ui, "Message 39.");
    let above = bounds(&mut ui, "Message 37.");
    move_to(&mut ui, end_of(lowest));
    press(&mut ui, mouse::Button::Left);
    move_to(&mut ui, above.center());
    let version = state(&mut ui, |state| state.version);
    frames(&mut ui, Instant::now(), Duration::from_millis(16), 3);
    assert_eq!(
        state(&mut ui, |state| state.version),
        version,
        "the pointer held still over rows that held still"
    );
    // The wheel moves the rows under the pointer: the next frame follows.
    ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Pixels { x: 0.0, y: 40.0 },
    })]);
    frames(&mut ui, Instant::now(), Duration::from_millis(16), 3);
    assert_ne!(state(&mut ui, |state| state.version), version);
    release(&mut ui, mouse::Button::Left);
    let copied = copy(&mut ui).expect("a selection");
    assert!(copied.ends_with("Message 39."), "{copied}");
}

#[test]
fn a_drag_and_a_clear_act_only_on_the_rows_they_change() {
    use super::super::act::ACTED;
    let chat = Chat::new(
        20,
        &[
            ("Sam", "First."),
            ("Riley", "Second."),
            ("Sam", "Third."),
            ("Riley", "Fourth."),
        ],
    );
    let mut ui = simulate(chat.view(front()));
    let second = bounds(&mut ui, "Second.");
    let third = bounds(&mut ui, "Third.");
    let fourth = bounds(&mut ui, "Fourth.");
    move_to(&mut ui, end_of(fourth));
    press(&mut ui, mouse::Button::Left);
    move_to(&mut ui, third.center());
    ACTED.set(0);
    // From the third row to the second: the third goes all in, the second
    // in from the pointer; the fourth and the first stay as they were.
    move_to(&mut ui, second.center());
    assert_eq!(ACTED.take(), 2, "the old focus row and the new");
    release(&mut ui, mouse::Button::Left);
    assert_eq!(names(&mut ui), [false, false, true, true]);
    // A click on nothing clears the three rows that showed some of it.
    ACTED.set(0);
    click(&mut ui, Point::new(300.0, 4.0));
    assert_eq!(ACTED.take(), 3, "the rows that showed something");
    assert_eq!(names(&mut ui), [false, false, false, false]);
    assert_eq!(copy(&mut ui), None);
}
