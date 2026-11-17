//! Dragging rows: where they drop, what the owner hears on the way, the
//! scroll at the list's edges, and what a drag looks like.

use std::time::{Duration, Instant};

use iced::{Point, keyboard, mouse};

use super::super::Drag;
use super::*;

/// What the drags came to, without the rows they went over.
fn outcomes(ui: &mut Simulator<'_, Message, Theme, Renderer>) -> Vec<Message> {
    ui.drain()
        .filter(|message| !matches!(message, Message::Dragged(Drag::Over { .. })))
        .collect()
}

/// The rows a drag went over, as the owner heard of them.
fn overs(ui: &mut Simulator<'_, Message, Theme, Renderer>) -> Vec<Option<usize>> {
    ui.drain()
        .filter_map(|message| match message {
            Message::Dragged(Drag::Over { to, .. }) => Some(to),
            _ => None,
        })
        .collect()
}

fn draggable(rows: &Rows, motion: Motion) -> List<'_, Message> {
    rows.list(motion)
        .reorder(|index| index != 0, |_, to| to != 5, Message::Dragged)
}

#[test]
fn a_dragged_row_drops_where_it_may_and_goes_back_where_it_may_not() {
    let rows = Rows::even(20, 30.0);
    let mut ui = simulate(draggable(&rows, still()));
    let drag = |ui: &mut Simulator<'_, Message, Theme, Renderer>, from_y: f32, to_y: f32| {
        ui.point_at(Point::new(100.0, from_y));
        let _ = ui.simulate([press(mouse::Button::Left)]);
        let _ = ui.simulate([moved_to(Point::new(100.0, from_y + 5.0))]);
        ui.point_at(Point::new(100.0, to_y));
        let _ = ui.simulate([moved_to(Point::new(100.0, to_y))]);
        let _ = ui.simulate([release(mouse::Button::Left)]);
    };
    drag(&mut ui, 75.0, 200.0);
    drag(&mut ui, 75.0, 165.0);
    drag(&mut ui, 15.0, 200.0);
    assert_eq!(
        outcomes(&mut ui),
        vec![
            Message::Dragged(Drag::Started { from: 2 }),
            Message::Dragged(Drag::Dropped { from: 2, to: 6 }),
            Message::Dragged(Drag::Started { from: 2 }),
            Message::Dragged(Drag::Cancelled { from: 2 }),
        ],
        "row 5 refuses it, row 0 can't be picked up"
    );
}

#[test]
fn escape_puts_a_dragged_row_back_and_a_small_move_is_a_click() {
    let rows = Rows::even(20, 30.0);
    let mut ui = simulate(draggable(&rows, still()));
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved_to(Point::new(100.0, 78.0)),
    ]);
    let _ = ui.simulate([release(mouse::Button::Left)]);
    assert_eq!(ui.drain().count(), 0, "3 px: not picked up");
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved_to(Point::new(100.0, 120.0)),
    ]);
    let _ = ui.tap_key(keyboard::key::Named::Escape);
    assert_eq!(
        outcomes(&mut ui),
        vec![
            Message::Dragged(Drag::Started { from: 2 }),
            Message::Dragged(Drag::Cancelled { from: 2 }),
        ]
    );
}

#[test]
fn a_row_with_a_handle_is_picked_up_by_it_only() {
    let rows = Rows::even(20, 30.0);
    let mut ui = simulate(draggable(&rows, still()).drag_handle(24.0));
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved_to(Point::new(100.0, 120.0)),
    ]);
    let _ = ui.simulate([release(mouse::Button::Left)]);
    assert_eq!(ui.drain().count(), 0, "pressed away from the handle");
    ui.point_at(Point::new(10.0, 75.0));
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved_to(Point::new(10.0, 120.0)),
    ]);
    assert_eq!(
        outcomes(&mut ui),
        vec![Message::Dragged(Drag::Started { from: 2 })]
    );
}

#[test]
fn a_drag_onto_rows_tells_each_new_one_and_a_release_beside_the_list_goes_back() {
    let rows = Rows::even(20, 30.0);
    let targets = draggable(&rows, still()).drop_line(false).width(200.0);
    let mut ui = simulate(targets);
    let left = mouse::Button::Left;
    let over = |to| Message::Dragged(Drag::Over { from: 2, to });
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([press(left), moved_to(Point::new(100.0, 80.0))]);
    let _ = ui.simulate([moved_to(Point::new(100.0, 85.0))]);
    let _ = ui.simulate([moved_to(Point::new(100.0, 200.0))]);
    let _ = ui.simulate([moved_to(Point::new(300.0, 200.0))]);
    let _ = ui.simulate([release(left)]);
    assert_eq!(
        ui.drain().collect::<Vec<_>>(),
        vec![
            Message::Dragged(Drag::Started { from: 2 }),
            over(Some(2)),
            over(Some(6)),
            over(None),
            Message::Dragged(Drag::Cancelled { from: 2 }),
        ],
        "a move within a row says nothing new; beside the list, nothing is under it"
    );
    let _ = ui.simulate([press(left), moved_to(Point::new(100.0, 200.0))]);
    let _ = ui.simulate([release(left)]);
    assert_eq!(
        outcomes(&mut ui),
        vec![
            Message::Dragged(Drag::Started { from: 2 }),
            Message::Dragged(Drag::Dropped { from: 2, to: 6 }),
        ],
        "over it, it drops"
    );
}

#[test]
fn dragging_near_the_bottom_scrolls_the_list() {
    let rows = Rows::even(100, 30.0);
    let mut ui = simulate(draggable(&rows, front()));
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([
        press(mouse::Button::Left),
        moved_to(Point::new(100.0, 90.0)),
    ]);
    let _ = ui.simulate([moved_to(Point::new(100.0, VIEW.height - 10.0))]);
    let start = Instant::now();
    for step in 0..10 {
        frame(&mut ui, start + Duration::from_millis(16 * step));
    }
    assert!(
        top(&mut ui, 0).is_none_or(|top| top < 0.0),
        "scrolled at the edge"
    );
    let _ = ui.simulate([moved_to(Point::new(100.0, 300.0))]);
    let shown = (0..100).find(|key| top(&mut ui, *key).is_some_and(|top| top >= 0.0));
    let held = shown.and_then(|key| top(&mut ui, key));
    frame(&mut ui, start + Duration::from_millis(400));
    assert_eq!(
        shown.and_then(|key| top(&mut ui, key)),
        held,
        "away from it: still"
    );
}

#[test]
fn draws_a_dragged_row_far_from_its_place_or_onto_targets_an_autoscroll_and_the_bar() {
    let rows = Rows::even(200, 30.0);
    for theme in [
        Theme::built_in(None),
        Theme::built_in(Some("catppuccin-latte")),
    ] {
        let mut ui = simulate(draggable(&rows, front()));
        ui.point_at(Point::new(100.0, 75.0));
        let _ = ui.simulate([
            press(mouse::Button::Left),
            moved_to(Point::new(100.0, 90.0)),
        ]);
        let _ = ui.simulate([moved_to(Point::new(100.0, VIEW.height - 5.0))]);
        let start = Instant::now();
        for step in 0..80 {
            frame(&mut ui, start + Duration::from_millis(16 * step));
        }
        assert!(
            top(&mut ui, 2).is_none(),
            "the dragged row's place scrolled out"
        );
        ui.draw(&theme);
        let _ = ui.simulate([release(mouse::Button::Left)]);
        ui.point_at(Point::new(200.0, 300.0));
        let _ = ui.simulate([press(mouse::Button::Middle), release(mouse::Button::Middle)]);
        ui.point_at(Point::new(VIEW.width - 4.0, 300.0));
        let _ = ui.simulate([moved_to(Point::new(VIEW.width - 4.0, 300.0))]);
        ui.draw(&theme);
        // Onto rows that are targets, its copy hangs below the pointer.
        let mut ui = simulate(draggable(&rows, still()).drop_line(false));
        ui.point_at(Point::new(100.0, 75.0));
        let _ = ui.simulate([
            press(mouse::Button::Left),
            moved_to(Point::new(100.0, 235.0)),
        ]);
        ui.draw(&theme);
    }
}

#[test]
fn a_pointer_past_a_list_shorter_than_the_window_is_over_no_row() {
    let rows = Rows::even(200, 30.0);
    let short = iced::widget::column![draggable(&rows, still()).height(400.0), text("Below")];
    let mut ui: Simulator<'_, Message, Theme, Renderer> =
        Simulator::with_size(Settings::default(), VIEW, short);
    let left = mouse::Button::Left;
    let over = |to| Message::Dragged(Drag::Over { from: 2, to });
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([press(left), moved_to(Point::new(100.0, 80.0))]);
    let _ = ui.simulate([moved_to(Point::new(100.0, 450.0))]);
    let _ = ui.simulate([release(left)]);
    assert_eq!(
        ui.drain().collect::<Vec<_>>(),
        vec![
            Message::Dragged(Drag::Started { from: 2 }),
            over(Some(2)),
            over(None),
            Message::Dragged(Drag::Cancelled { from: 2 }),
        ],
        "the rows built below the list are out of view: no line, no drop"
    );
}

#[test]
fn a_drag_scrolling_at_the_bottom_stays_over_a_row() {
    let rows = Rows::even(200, 30.0);
    let mut ui = simulate(draggable(&rows, front()));
    let left = mouse::Button::Left;
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([press(left), moved_to(Point::new(100.0, 80.0))]);
    let _ = ui.simulate([moved_to(Point::new(100.0, VIEW.height - 1.0))]);
    let _ = ui.drain().count();
    let start = Instant::now();
    for step in 0..120 {
        frame(&mut ui, start + Duration::from_millis(16 * step));
    }
    let told = overs(&mut ui);
    assert!(told.len() > 20, "it scrolled row after row: {told:?}");
    assert!(
        told.windows(2).all(|pair| pair[0] < pair[1]),
        "down the rows, never off them: {told:?}"
    );
}

#[test]
fn a_pressed_row_is_picked_up_from_where_it_is_now() {
    let rows = Rows::even(20, 30.0);
    let more = Rows::with(
        std::iter::once(1000)
            .chain(rows.keys.iter().copied())
            .collect(),
        vec![30.0; 21],
    );
    let left = mouse::Button::Left;
    let mut ui = simulate(draggable(&rows, still()));
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([press(left)]);
    let mut ui = ui.rebuild(draggable(&more, still()));
    frame(&mut ui, Instant::now());
    let _ = ui.simulate([moved_to(Point::new(100.0, 81.0))]);
    let told: Vec<Message> = ui.drain().collect();
    assert_eq!(
        told.first(),
        Some(&Message::Dragged(Drag::Started { from: 3 })),
        "a row went in above: row 2 is at 3 now"
    );
    assert!(
        told.iter().all(|message| matches!(
            message,
            Message::Dragged(Drag::Started { from: 3 } | Drag::Over { from: 3, .. })
        )),
        "what it goes over is told from there too: {told:?}"
    );
}

#[test]
fn a_pressed_row_that_leaves_the_list_is_not_picked_up() {
    let rows = Rows::even(20, 30.0);
    let fewer = Rows::with(
        rows.keys.iter().copied().filter(|key| *key != 2).collect(),
        vec![30.0; 19],
    );
    let left = mouse::Button::Left;
    let mut ui = simulate(draggable(&rows, still()));
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([press(left)]);
    let mut ui = ui.rebuild(draggable(&fewer, still()));
    frame(&mut ui, Instant::now());
    let _ = ui.simulate([moved_to(Point::new(100.0, 81.0))]);
    let _ = ui.simulate([moved_to(Point::new(100.0, 200.0))]);
    let _ = ui.simulate([release(left)]);
    assert_eq!(ui.drain().count(), 0, "the owner never heard it start");
}

#[test]
fn a_dragged_row_that_leaves_the_list_goes_back() {
    let rows = Rows::even(20, 30.0);
    let fewer = Rows::with(
        rows.keys.iter().copied().filter(|key| *key != 2).collect(),
        vec![30.0; 19],
    );
    let left = mouse::Button::Left;
    let over = |to| Message::Dragged(Drag::Over { from: 2, to });
    let mut ui = simulate(draggable(&rows, still()));
    ui.point_at(Point::new(100.0, 75.0));
    let _ = ui.simulate([press(left), moved_to(Point::new(100.0, 80.0))]);
    let _ = ui.simulate([moved_to(Point::new(100.0, 200.0))]);
    let mut ui = ui.rebuild(draggable(&fewer, still()));
    frame(&mut ui, Instant::now());
    let _ = ui.simulate([moved_to(Point::new(100.0, 260.0))]);
    let _ = ui.simulate([release(left)]);
    assert_eq!(
        ui.drain().collect::<Vec<_>>(),
        vec![
            Message::Dragged(Drag::Started { from: 2 }),
            over(Some(2)),
            over(Some(6)),
            Message::Dragged(Drag::Cancelled { from: 2 }),
        ],
        "the owner unmarks what it marked"
    );
}
