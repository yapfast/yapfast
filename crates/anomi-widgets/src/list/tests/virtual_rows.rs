//! Rows built only near the view, and scrolling through them.

use std::time::{Duration, Instant};

use iced::{Point, mouse};

use super::*;

#[test]
fn five_thousand_rows_of_mixed_heights_scroll_with_only_those_near_the_view_built() {
    let rows = Rows::mixed(0, 5000);
    let mut ui = simulate(rows.list(still()));
    assert_eq!(top(&mut ui, 0), Some(0.0));
    let first = rows.alive();
    assert!(first < 40, "{first} rows built for a 600 px view");
    assert!(top(&mut ui, 200).is_none(), "far rows aren't built");
    let mut most = first;
    for _ in 0..400 {
        scroll(&mut ui, 1000.0);
        most = most.max(rows.alive());
    }
    assert!(
        most < 60,
        "at most {most} rows alive at once while scrolling"
    );
    let last = 4999;
    let bottom = top(&mut ui, last).expect("the end reached") + height(last);
    assert!(
        (bottom - VIEW.height).abs() < 0.5,
        "the last row ends the view: {bottom}"
    );
    scroll(&mut ui, -500_000.0);
    assert_eq!(top(&mut ui, 0), Some(0.0), "and back to the start");
}

#[test]
fn a_long_scroll_asked_by_an_operation_builds_only_the_rows_where_it_lands() {
    let rows = Rows::mixed(0, 5000);
    let mut ui = simulate(rows.list(still()));
    let _ = rows.most_alive();
    ui.operate(&mut super::super::operation::by::<()>(
        Id::new(ID),
        100_000.0,
    ));
    frame(&mut ui, Instant::now());
    let most = rows.most_alive();
    assert!(most < 60, "{most} rows alive at once for the scroll");
    assert!(top(&mut ui, 0).is_none(), "it went far down");
    assert!(
        (1500..2100).any(|key| top(&mut ui, key).is_some()),
        "about 100,000 px down, by the rows' estimates"
    );
}

#[test]
fn rows_of_a_list_given_all_of_them_are_all_built() {
    let rows = Rows::even(50, 30.0);
    let built = super::super::List::with_rows(
        rows.keys
            .iter()
            .map(|key| (*key, iced::widget::text(label(*key)).boxed())),
        still(),
    );
    let mut ui = simulate(built);
    assert_eq!(top(&mut ui, 0), Some(0.0));
    let last = top(&mut ui, 49).expect("built though out of view");
    assert!(last > VIEW.height, "out of view: {last}");
    scroll(&mut ui, 100_000.0);
    assert!(top(&mut ui, 0).is_some(), "still built");
}

#[test]
fn a_wheel_notch_glides_60_px_and_lands_at_once_while_still() {
    let rows = Rows::even(100, 30.0);
    let mut ui = simulate(rows.list(front()));
    ui.point_at(Point::new(200.0, 300.0));
    let _ = ui.scroll(mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 });
    assert_eq!(top(&mut ui, 2), Some(60.0), "nothing moved before a frame");
    let start = Instant::now();
    frame(&mut ui, start + Duration::from_millis(40));
    let partway = top(&mut ui, 2).unwrap();
    assert!(partway < 60.0 && partway > 0.0, "gliding: {partway}");
    frame(&mut ui, start + Duration::from_secs(1));
    assert_eq!(top(&mut ui, 2), Some(0.0), "one notch: 60 px");

    let mut ui = simulate(rows.list(still()));
    ui.point_at(Point::new(200.0, 300.0));
    let _ = ui.scroll(mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 });
    assert_eq!(top(&mut ui, 2), Some(0.0), "still: at once");
}

#[test]
fn a_notch_while_one_glides_adds_to_what_is_left() {
    let rows = Rows::even(100, 30.0);
    let mut ui = simulate(rows.list(front()));
    ui.point_at(Point::new(200.0, 300.0));
    let _ = ui.scroll(mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 });
    frame(&mut ui, Instant::now() + Duration::from_millis(20));
    let _ = ui.scroll(mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 });
    settle(&mut ui);
    assert_eq!(top(&mut ui, 4), Some(0.0), "two notches: 120 px");
}

#[test]
fn a_wheel_at_the_end_it_reached_is_left_to_what_holds_the_list() {
    let rows = Rows::even(5, 30.0);
    let mut ui = simulate(rows.list(still()));
    ui.point_at(Point::new(200.0, 300.0));
    let status = ui.scroll(mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 });
    assert_eq!(status, iced::event::Status::Ignored);
}

#[test]
fn the_scrollbar_thumb_drags_the_list_and_stays_under_the_pointer() {
    let rows = Rows::even(1000, 30.0);
    let mut ui = simulate(rows.list(still()));
    ui.point_at(Point::new(VIEW.width - 4.0, 10.0));
    let _ = ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))]);
    let _ = ui.simulate([moved_to(Point::new(VIEW.width - 4.0, 300.0))]);
    settle(&mut ui);
    assert!(top(&mut ui, 0).is_none(), "half way down a long list");
    let seen: Vec<u64> = (400..600)
        .filter(|key| top(&mut ui, *key).is_some())
        .collect();
    assert!(!seen.is_empty(), "somewhere around the middle");
    let _ = ui.simulate([moved_to(Point::new(VIEW.width - 4.0, -50.0))]);
    settle(&mut ui);
    assert_eq!(top(&mut ui, 0), Some(0.0), "dragged back past the top");
    let _ = ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(
        mouse::Button::Left,
    ))]);
}

#[test]
fn padding_and_spacing_scroll_with_the_rows() {
    let rows = Rows::even(100, 30.0);
    let mut ui = simulate(rows.list(still()).padding(10).spacing(2.0));
    assert_eq!(top(&mut ui, 0), Some(10.0));
    assert_eq!(top(&mut ui, 1), Some(42.0));
    scroll(&mut ui, 10.0);
    assert_eq!(top(&mut ui, 0), Some(0.0));
}

#[test]
fn a_list_that_shrinks_is_as_tall_as_its_rows_up_to_its_room() {
    let empty = Rows::even(0, 30.0);
    let short = Rows::even(3, 30.0);
    let long = Rows::even(100, 30.0);
    for (rows, below) in [(&empty, 0.0), (&short, 90.0), (&long, VIEW.height)] {
        let column = iced::widget::column![
            rows.list(still()).height(iced::Shrink),
            iced::widget::text("Below")
        ];
        let mut ui: Simulator<'_, Message, Theme, Renderer> =
            Simulator::with_size(Settings::default(), VIEW, column);
        let found = ui.find("Below").expect("laid out").bounds().y;
        assert_eq!(found, below);
    }
}
