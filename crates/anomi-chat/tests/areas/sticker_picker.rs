//! The sticker picker: task C19's tests, driving
//! `anomi_chat::pickers::sticker::view` over its fixtures in the simulator
//! (`super::support`).
//!
//! The picker lies at the window's top left, 498 by 486: the search over its
//! first 50 px, the rail down its first 48, the grid beside it, and the foot
//! over its last 46. The samples' grid starts with Frequently Used's heading
//! (28 px) and its three stickers in one row of 104 px (112 with the gap
//! before the next heading), Server's heading and its three, then Wumpus's;
//! four tiles of 96 px, 8 px apart, hang from 12 px in.

use iced::advanced::widget::operation::focusable;
use iced::keyboard::key::Named;
use iced::{Event, Point, mouse};
use iced_test::{Simulator, simulator};

use anomi_chat::pickers::sticker::{FIELD, view};
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::sticker_picker::{Hovered, Intent, StickerPicker};
use anomi_vm::fixtures::chat::sticker_picker as sample;

use super::support::{self, SIZE};

/// Where the grid starts, and the middle of the sticker in column `column`
/// of the row that starts `top` px down it.
const GRID: Point = Point::new(48.0, 50.0);
fn sticker_at(column: f32, top: f32) -> Point {
    Point::new(GRID.x + 12.0 + 104.0 * column + 48.0, GRID.y + top + 48.0)
}

const WUMPUS: &str = "1400000000000000330";
const PARTY: &str = "1400000000000000331";
const BOUNCE: &str = "1400000000000000311";
const WAVE: &str = "1400000000000000310";

fn simulate(picker: &StickerPicker) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(support::settings(), SIZE, view(picker, support::front()))
}

/// What a click at `at` asks for, but the hover it brings and the rows it
/// shows.
fn clicked(picker: &StickerPicker, at: Point) -> Vec<Intent> {
    let mut ui = simulate(picker);
    ui.point_at(at);
    ui.simulate(simulator::click());
    acts(support::asked(ui))
}

fn acts(asked: Vec<Intent>) -> Vec<Intent> {
    asked
        .into_iter()
        .filter(|intent| {
            !matches!(
                intent,
                Intent::Hover(_) | Intent::Leave(_) | Intent::Shown(..)
            )
        })
        .collect()
}

#[test]
fn typing_in_the_field_searches() {
    let picker = sample::picker();
    let mut ui = simulate(&picker);
    ui.click("Find the perfect sticker").unwrap();
    ui.typewrite("wa");
    assert_eq!(
        acts(support::asked(ui)).last(),
        Some(&Intent::Search("wa".into()))
    );
}

#[test]
fn enter_picks_the_first_sticker() {
    let picker = sample::picker();
    let mut ui = simulate(&picker);
    ui.operate(&mut focusable::focus(FIELD.into()));
    ui.tap_key(Named::Enter);
    assert_eq!(
        acts(support::asked(ui)),
        vec![Intent::Pick(WUMPUS.into())],
        "even before anything is searched, as Slint's"
    );
    // A locked one offers Nitro, as a click on it does.
    let locked = sample::searched_locked("sticker_");
    let mut ui = simulate(&locked);
    ui.operate(&mut focusable::focus(FIELD.into()));
    ui.tap_key(Named::Enter);
    assert_eq!(acts(support::asked(ui)), vec![Intent::Locked]);
}

#[test]
fn a_sticker_clicked_is_picked() {
    let picker = sample::picker();
    assert_eq!(
        clicked(&picker, sticker_at(0.0, 28.0)),
        vec![Intent::Pick(WUMPUS.into())]
    );
    assert_eq!(
        clicked(&picker, sticker_at(1.0, 28.0)),
        vec![Intent::Pick(PARTY.into())]
    );
    // Server's Bounce: under Frequently Used's row and Server's heading.
    assert_eq!(
        clicked(&picker, sticker_at(1.0, 28.0 + 112.0 + 28.0)),
        vec![Intent::Pick(BOUNCE.into())]
    );
}

#[test]
fn a_locked_sticker_offers_nitro() {
    let picker = sample::searched_locked("sticker_");
    assert!(!picker.sections[0].stickers[0].usable);
    assert_eq!(
        clicked(&picker, sticker_at(0.0, 28.0)),
        vec![Intent::Locked]
    );
    assert_eq!(
        clicked(&picker, sticker_at(3.0, 28.0)),
        vec![Intent::Locked]
    );
}

#[test]
fn a_star_keeps_a_sticker_among_the_favourites_or_lets_it_go() {
    // The star is in the tile's top right corner, 4 px in.
    let corner = |at: Point| Point::new(at.x + 48.0 - 16.0, at.y - 48.0 + 16.0);
    let picker = sample::picker();
    assert_eq!(
        clicked(&picker, corner(sticker_at(0.0, 28.0))),
        vec![Intent::Favorite(WUMPUS.into())]
    );
    // A favourite's, filled: Wave heads the favourites.
    let picker = sample::favorited();
    assert!(picker.favorite(&picker.sections[0].stickers[0]));
    assert_eq!(
        clicked(&picker, corner(sticker_at(0.0, 28.0))),
        vec![Intent::Favorite(WAVE.into())]
    );
}

#[test]
fn the_rail_jumps_to_a_collection() {
    let picker = sample::picker();
    let button = |index: f32| Point::new(24.0, GRID.y + 40.0 * index + 22.0);
    assert_eq!(clicked(&picker, button(0.0)), vec![Intent::Jump(0)]);
    assert_eq!(clicked(&picker, button(2.0)), vec![Intent::Jump(2)]);
}

#[test]
fn what_the_pointer_is_over_is_named_and_forgotten() {
    let picker = sample::picker();
    let mut ui = simulate(&picker);
    let mut point = |at: Point| {
        ui.point_at(at);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: at })]);
    };
    point(sticker_at(1.0, 28.0));
    point(Point::new(24.0, GRID.y + 22.0));
    point(Point::new(300.0, 480.0));
    let moves: Vec<Intent> = support::asked(ui)
        .into_iter()
        .filter(|intent| matches!(intent, Intent::Hover(_) | Intent::Leave(_)))
        .collect();
    let party = Hovered::Sticker(PARTY.into());
    let first = Hovered::Section(0);
    // Each is named as the pointer comes and let go as it leaves; a leave may
    // come after the next one's hover, which is why it only clears a name that
    // is still its own.
    for intent in [
        Intent::Hover(party.clone()),
        Intent::Leave(party),
        Intent::Hover(first.clone()),
        Intent::Leave(first),
    ] {
        assert!(moves.contains(&intent), "{intent:?} in {moves:?}");
    }
    assert_eq!(moves.len(), 4);
}

#[test]
fn the_foot_says_what_a_click_does_until_something_is_pointed_at() {
    assert!(
        simulate(&sample::picker())
            .find("Click a sticker to send it.")
            .is_ok()
    );
    let hovered = sample::hovered();
    let mut ui = simulate(&hovered);
    assert!(ui.find("Bounce").is_ok());
    assert!(ui.find("Click a sticker to send it.").is_err());
    assert!(simulate(&sample::waiting()).find("Server").is_ok());
}

#[test]
fn the_headings_name_the_collections() {
    let ui_picker = sample::picker();
    let mut ui = simulate(&ui_picker);
    for name in ["Frequently Used", "Server", "Wumpus"] {
        assert!(ui.find(name).is_ok(), "{name}");
    }
}

#[test]
fn a_search_without_matches_says_so() {
    let picker = sample::nothing();
    assert!(
        simulate(&picker)
            .find("No stickers match your search.")
            .is_ok()
    );
}

#[test]
fn packs_that_did_not_come_say_why() {
    let picker = sample::failed();
    assert!(
        simulate(&picker)
            .find("Couldn't load the sticker packs.")
            .is_ok()
    );
}

#[test]
fn a_new_picker_with_nothing_yet_says_nothing() {
    let picker = StickerPicker::default();
    let mut ui = simulate(&picker);
    assert!(ui.find("No stickers match your search.").is_err());
}

#[test]
fn the_grid_tells_which_rows_show() {
    let picker = sample::picker();
    let mut ui = simulate(&picker);
    ui.draw(&Theme::default());
    let shown: Vec<Intent> = support::asked(ui)
        .into_iter()
        .filter(|intent| matches!(intent, Intent::Shown(..)))
        .collect();
    assert_eq!(shown.len(), 1, "once, as it is first laid out");
    let Intent::Shown(first, last) = shown[0] else {
        unreachable!()
    };
    assert_eq!(first, 0);
    assert!((3..=6).contains(&last), "a screen of rows, not all: {last}");
}

#[test]
fn a_sticker_with_no_picture_yet_still_takes_its_place() {
    // Waiting stickers are tiles too: a press picks them.
    let picker = sample::waiting();
    assert_eq!(
        clicked(&picker, sticker_at(0.0, 28.0)),
        vec![Intent::Pick(WUMPUS.into())]
    );
}
