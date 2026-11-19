//! A list that makes room: where the pointer aims, the dwell before
//! "onto", what a release does, and rows sliding to their new places.

use std::time::{Duration, Instant};

use iced::{Point, keyboard, mouse};

use super::super::{DWELL, Drag, Zone};
use super::*;

/// Rows 48 px tall, any but row 0 dragged, onto any but row 1.
fn roomy(rows: &Rows, motion: Motion) -> List<'_, Message> {
    rows.list(motion)
        .reorder(|index| index != 0, |_, _| true, Message::Dragged)
        .live(|_, to| to != 1)
        .width(200.0)
}

fn aims(ui: &mut Simulator<'_, Message, Theme, Renderer>) -> Vec<Option<(usize, Zone)>> {
    ui.drain()
        .filter_map(|message| match message {
            Message::Dragged(Drag::Aimed { to, .. }) => Some(to),
            _ => None,
        })
        .collect()
}

/// Picks row 2 up by its middle.
fn pick_up(ui: &mut Simulator<'_, Message, Theme, Renderer>) {
    ui.point_at(Point::new(100.0, 120.0));
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved_to(Point::new(100.0, 126.0)),
    ]);
}

#[test]
fn the_edges_of_a_row_aim_beside_it_at_once_and_its_middle_onto_it_after_a_dwell() {
    let rows = Rows::even(10, 48.0);
    let mut ui = simulate(roomy(&rows, still()));
    pick_up(&mut ui);
    // Row 4 spans 192 to 240: its lower quarter, then its upper one.
    let _ = ui.simulate([moved_to(Point::new(100.0, 236.0))]);
    let _ = ui.simulate([moved_to(Point::new(100.0, 196.0))]);
    assert_eq!(
        aims(&mut ui),
        vec![Some((4, Zone::Below)), Some((4, Zone::Above))],
        "at once"
    );
    let _ = ui.simulate([moved_to(Point::new(100.0, 216.0))]);
    assert!(aims(&mut ui).is_empty(), "the middle waits");
    frame(&mut ui, Instant::now() + DWELL + Duration::from_millis(10));
    assert_eq!(aims(&mut ui), vec![Some((4, Zone::Onto))], "once it held");
    // Row 1 takes nothing onto it: its halves.
    let _ = ui.simulate([moved_to(Point::new(100.0, 70.0))]);
    frame(&mut ui, Instant::now() + 2 * DWELL);
    assert_eq!(aims(&mut ui), vec![Some((1, Zone::Above))]);
    // Over its own place it aims where it did; beside the list, nowhere.
    let _ = ui.simulate([moved_to(Point::new(100.0, 110.0))]);
    assert!(aims(&mut ui).is_empty());
    let _ = ui.simulate([moved_to(Point::new(300.0, 110.0))]);
    assert_eq!(aims(&mut ui), vec![None]);
}

#[test]
fn the_room_under_the_last_row_aims_past_it() {
    let rows = Rows::even(4, 48.0);
    let mut ui = simulate(roomy(&rows, still()));
    pick_up(&mut ui);
    // Row 3 ends at 192; under it the list is empty.
    let _ = ui.simulate([moved_to(Point::new(100.0, 260.0))]);
    assert_eq!(aims(&mut ui), vec![Some((3, Zone::Past))]);
    let _ = ui.simulate([moved_to(Point::new(100.0, 180.0))]);
    assert_eq!(
        aims(&mut ui),
        vec![Some((3, Zone::Below))],
        "its own lower quarter"
    );
}

#[test]
fn letting_go_over_the_list_drops_it_where_shown_and_beside_it_or_escape_puts_it_back() {
    let rows = Rows::even(10, 48.0);
    let mut ui = simulate(roomy(&rows, still()));
    let left = mouse::Button::Left;
    let ends = |ui: &mut Simulator<'_, Message, Theme, Renderer>| -> Vec<Message> {
        ui.drain()
            .filter(|message| !matches!(message, Message::Dragged(Drag::Aimed { .. })))
            .collect()
    };
    pick_up(&mut ui);
    let _ = ui.simulate([moved_to(Point::new(100.0, 300.0))]);
    let _ = ui.simulate([release(left)]);
    pick_up(&mut ui);
    let _ = ui.simulate([moved_to(Point::new(300.0, 300.0))]);
    let _ = ui.simulate([release(left)]);
    pick_up(&mut ui);
    let _ = ui.tap_key(keyboard::key::Named::Escape);
    let started = Message::Dragged(Drag::Started { from: 2 });
    let back = Message::Dragged(Drag::Cancelled { from: 2 });
    assert_eq!(
        ends(&mut ui),
        vec![
            started.clone(),
            Message::Dragged(Drag::Dropped { from: 2, to: 2 }),
            started.clone(),
            back.clone(),
            started,
            back,
        ]
    );
}

#[test]
fn rows_the_owner_moves_slide_to_their_places_and_jump_when_still() {
    let rows = Rows::even(6, 48.0);
    let mut ui = simulate(roomy(&rows, front()));
    settle(&mut ui);
    assert_eq!(top(&mut ui, 3), Some(144.0));
    // The owner moved row 3 to the top.
    let moved = Rows::with(vec![3, 0, 1, 2, 4, 5], vec![48.0; 6]);
    let mut ui = ui.rebuild(roomy(&moved, front()));
    let start = Instant::now();
    frame(&mut ui, start);
    let first = top(&mut ui, 3).unwrap();
    assert!(first > 100.0, "drawn where it was at first: {first}");
    frame(&mut ui, start + Duration::from_millis(90));
    let midway = top(&mut ui, 3).unwrap();
    assert!(midway < first && midway > 0.0, "on its way: {midway}");
    frame(&mut ui, start + Duration::from_millis(400));
    assert_eq!(top(&mut ui, 3), Some(0.0), "arrived");
    assert_eq!(top(&mut ui, 0), Some(48.0));
    let back = Rows::even(6, 48.0);
    let mut ui = ui.rebuild(roomy(&back, still()));
    frame(&mut ui, Instant::now());
    assert_eq!(top(&mut ui, 3), Some(144.0), "still: it jumps");
}
