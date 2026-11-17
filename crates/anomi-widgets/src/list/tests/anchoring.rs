//! The list holds its place by a row's key as rows come, go and grow; it
//! follows its end; jumps land where they're asked.

use std::time::Instant;

use super::super::{Place, operation};
use super::*;

/// A timeline's rows: the first carries a 42 px loading notice above it.
fn timeline(first: u64, count: usize) -> Rows {
    let keys: Vec<u64> = (first..first + count as u64).collect();
    let heights = keys
        .iter()
        .enumerate()
        .map(|(index, key)| height(*key) + if index == 0 { 42.0 } else { 0.0 })
        .collect();
    Rows::with(keys, heights)
}

fn bottom(ui: &mut Simulator<'_, Message, Theme, Renderer>, rows: &Rows, key: u64) -> f32 {
    let index = rows.keys.iter().position(|row| *row == key).unwrap();
    top(ui, key).expect("built") + rows.heights[index]
}

/// The first row whose top shows in the view.
fn first_shown(ui: &mut Simulator<'_, Message, Theme, Renderer>, rows: &Rows) -> u64 {
    *rows
        .keys
        .iter()
        .find(|key| top(ui, **key).is_some_and(|top| (0.0..VIEW.height).contains(&top)))
        .expect("a row in view")
}

#[test]
fn loading_older_rows_above_keeps_the_view_still() {
    let newer = Rows::mixed(1000, 100);
    let both = Rows::mixed(950, 150);
    let mut ui = simulate(newer.list(still()).from_end());
    scroll(&mut ui, -300.0);
    let shown = first_shown(&mut ui, &newer);
    let before = top(&mut ui, shown);
    let mut ui = ui.rebuild(both.list(still()).from_end());
    assert_eq!(top(&mut ui, shown), before);
}

#[test]
fn the_first_row_keeps_its_bottom_as_older_rows_go_in_above_it() {
    let newer = timeline(1000, 100);
    let both = timeline(950, 150);
    let mut ui = simulate(newer.list(still()).from_end());
    scroll(&mut ui, -1_000_000.0);
    assert_eq!(
        top(&mut ui, 1000),
        Some(0.0),
        "at the top, the notice in view"
    );
    scroll(&mut ui, 20.0);
    let edge = bottom(&mut ui, &newer, 1000);
    let mut ui = ui.rebuild(both.list(still()).from_end());
    assert_eq!(
        bottom(&mut ui, &both, 1000),
        edge,
        "its text stays where it was"
    );
    // A row deleted above leaves it where it is too.
    let fewer = Rows::with(
        both.keys
            .iter()
            .copied()
            .filter(|key| *key != 960)
            .collect(),
        both.keys
            .iter()
            .enumerate()
            .filter(|(_, key)| **key != 960)
            .map(|(index, _)| both.heights[index])
            .collect(),
    );
    let mut ui = ui.rebuild(fewer.list(still()).from_end());
    assert_eq!(bottom(&mut ui, &fewer, 1000), edge);
    scroll(&mut ui, -200.0);
    assert!(
        top(&mut ui, 999).is_some_and(|top| top >= 0.0),
        "scrolling up shows those that went in"
    );
}

#[test]
fn a_late_growing_row_at_the_end_keeps_the_list_at_the_end() {
    let rows = Rows::mixed(0, 100);
    let mut heights = rows.heights.clone();
    heights[99] = 300.0;
    let grown = Rows::with(rows.keys.clone(), heights);
    let mut ui = simulate(rows.list(still()).from_end());
    assert_eq!(bottom(&mut ui, &rows, 99), VIEW.height);
    let mut ui = ui.rebuild(grown.list(still()).from_end());
    assert_eq!(bottom(&mut ui, &grown, 99), VIEW.height);
}

#[test]
fn scrolling_away_stops_following_and_reaching_the_end_follows_again() {
    let (a, b, c, d) = (
        Rows::mixed(0, 100),
        Rows::mixed(0, 101),
        Rows::mixed(0, 102),
        Rows::mixed(0, 103),
    );
    let ui = simulate(a.list(still()).from_end());
    let mut ui = ui.rebuild(b.list(still()).from_end());
    assert_eq!(
        bottom(&mut ui, &b, 100),
        VIEW.height,
        "a new row is followed"
    );
    scroll(&mut ui, -200.0);
    let shown = first_shown(&mut ui, &b);
    let held = top(&mut ui, shown);
    let mut ui = ui.rebuild(c.list(still()).from_end());
    assert_eq!(top(&mut ui, shown), held, "scrolled away: the view holds");
    scroll(&mut ui, 1_000_000.0);
    let mut ui = ui.rebuild(d.list(still()).from_end());
    assert_eq!(
        bottom(&mut ui, &d, 102),
        VIEW.height,
        "at the end again: followed"
    );
}

#[test]
fn a_window_of_older_rows_holds_still_at_its_end() {
    let (a, b) = (Rows::mixed(0, 100), Rows::mixed(0, 101));
    let mut ui = simulate(a.list(still()).from_end().follow(false));
    assert_eq!(
        top(&mut ui, 0),
        Some(0.0),
        "at the top until a jump places it"
    );
    scroll(&mut ui, 1_000_000.0);
    let held = top(&mut ui, 99);
    let mut ui = ui.rebuild(b.list(still()).from_end().follow(false));
    assert_eq!(top(&mut ui, 99), held);
}

#[test]
fn a_short_list_sits_at_its_end_or_its_start() {
    let rows = Rows::even(3, 30.0);
    let mut ui = simulate(rows.list(still()).from_end());
    assert_eq!(
        top(&mut ui, 2),
        Some(VIEW.height - 30.0),
        "against the composer"
    );
    let mut ui = simulate(rows.list(still()));
    assert_eq!(top(&mut ui, 0), Some(0.0));
}

#[test]
fn rows_going_in_at_the_top_of_a_list_at_its_start_show() {
    let rows = Rows::even(50, 30.0);
    let more = Rows::with(std::iter::once(99).chain(0..50).collect(), vec![30.0; 51]);
    let ui = simulate(rows.list(still()));
    let mut ui = ui.rebuild(more.list(still()));
    assert_eq!(top(&mut ui, 99), Some(0.0));
    assert_eq!(top(&mut ui, 0), Some(30.0));
}

#[test]
fn a_jump_lands_at_a_third_or_the_top_held_inside_the_ends() {
    let rows = Rows::mixed(0, 1000);
    let mut ui = simulate(rows.list(still()).from_end());
    ui.operate(&mut operation::jump::<()>(Id::new(ID), 500, Place::Third));
    frame(&mut ui, Instant::now());
    assert_eq!(top(&mut ui, 500), Some(VIEW.height / 3.0));
    ui.operate(&mut operation::jump::<()>(Id::new(ID), 700, Place::Top));
    frame(&mut ui, Instant::now());
    assert_eq!(top(&mut ui, 700), Some(0.0));
    ui.operate(&mut operation::jump::<()>(Id::new(ID), 999, Place::Third));
    frame(&mut ui, Instant::now());
    assert_eq!(
        bottom(&mut ui, &rows, 999),
        VIEW.height,
        "near the end: held at it"
    );
}

#[test]
fn a_jump_waits_for_its_row_and_stops_following() {
    let rows = Rows::mixed(0, 1000);
    let keys: Vec<u64> = (0..500).chain([5000]).chain(500..999).collect();
    let later = Rows::with(keys.clone(), keys.iter().map(|key| height(*key)).collect());
    let mut appended = keys.clone();
    appended.push(6000);
    let longer = Rows::with(
        appended.clone(),
        appended.iter().map(|key| height(*key)).collect(),
    );
    let mut ui = simulate(rows.list(still()).from_end());
    ui.operate(&mut operation::jump::<()>(Id::new(ID), 5000, Place::Third));
    frame(&mut ui, Instant::now());
    assert!(
        top(&mut ui, 999).is_some(),
        "not there yet: still at the end"
    );
    let mut ui = ui.rebuild(later.list(still()).from_end());
    frame(&mut ui, Instant::now());
    assert_eq!(top(&mut ui, 5000), Some(VIEW.height / 3.0));
    let mut ui = ui.rebuild(longer.list(still()).from_end());
    assert_eq!(
        top(&mut ui, 5000),
        Some(VIEW.height / 3.0),
        "no longer following"
    );
}

#[test]
fn a_jump_revealing_a_far_row_taller_than_estimated_shows_its_bottom() {
    let mut heights = vec![30.0; 1000];
    heights[600] = 300.0;
    let rows = Rows::with((0..1000).collect(), heights);
    let mut ui = simulate(rows.list(still()));
    ui.operate(&mut operation::jump::<()>(
        Id::new(ID),
        600,
        Place::Reveal(10.0),
    ));
    frame(&mut ui, Instant::now());
    assert_eq!(bottom(&mut ui, &rows, 600), VIEW.height - 10.0);
}

#[test]
fn to_end_follows_again_and_to_start_goes_up() {
    let rows = Rows::mixed(0, 500);
    let mut ui = simulate(rows.list(still()).from_end());
    ui.operate(&mut operation::start::<()>(Id::new(ID)));
    frame(&mut ui, Instant::now());
    assert_eq!(top(&mut ui, 0), Some(0.0));
    ui.operate(&mut operation::end::<()>(Id::new(ID)));
    frame(&mut ui, Instant::now());
    assert_eq!(bottom(&mut ui, &rows, 499), VIEW.height);
}

#[test]
fn a_row_kept_in_view_comes_into_it_and_stays_as_it_grows() {
    let rows = Rows::even(100, 30.0);
    let mut heights = rows.heights.clone();
    heights[40] = 90.0;
    let grown = Rows::with(rows.keys.clone(), heights);
    let mut ui = simulate(rows.list(still()).reveal(Some(40), 10.0));
    assert_eq!(bottom(&mut ui, &rows, 40), VIEW.height - 10.0);
    let mut ui = ui.rebuild(grown.list(still()).reveal(Some(40), 10.0));
    assert_eq!(
        bottom(&mut ui, &grown, 40),
        VIEW.height - 10.0,
        "its field grew"
    );
}
