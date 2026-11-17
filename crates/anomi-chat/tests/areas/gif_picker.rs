//! The GIF picker: task C18's tests, driving `anomi_chat::pickers::gif::view`
//! over its fixtures in the simulator (`super::support`).
//!
//! The picker lies at the window's top left, 498 wide: the search row over
//! its first 46 px (34 and 12 under), the grid under it with 12 px gutters,
//! two columns of 231 px. The samples' first GIF is 231 by 130.

use iced::advanced::widget::operation::focusable;
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key, Modifiers};
use iced::{Event, Point, widget::image};
use iced_test::{Simulator, simulator};

use anomi_chat::pickers::gif::{FIELD, view};
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::gif_picker::{GifPicker, Intent, Page};
use anomi_vm::fixtures::chat::gif_picker as sample;

use super::support::{self, SIZE};

const PANEL: f32 = 498.0;
/// Where the grid starts, and its gutter.
const GRID: f32 = 46.0;
const GUTTER: f32 = 12.0;

fn still(src: &str) -> image::Handle {
    sample::still(src)
}

fn simulate(picker: &GifPicker) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(
        support::settings(),
        SIZE,
        view(picker, PANEL, support::front()),
    )
}

/// What a click at `at` asks for, but the pictures it has asked for.
fn clicked(picker: &GifPicker, at: Point) -> Vec<Intent> {
    let mut ui = simulate(picker);
    ui.point_at(at);
    ui.simulate(simulator::click());
    acts(support::asked(ui))
}

fn acts(asked: Vec<Intent>) -> Vec<Intent> {
    asked
        .into_iter()
        .filter(|intent| !matches!(intent, Intent::Shown { .. }))
        .collect()
}

/// The middle of the first tile of the grid.
fn first_tile() -> Point {
    Point::new(GUTTER + 115.0, GRID + GUTTER + 60.0)
}

#[test]
fn the_home_tiles_open_their_pages_and_terms_search() {
    let home = sample::home();
    let mut ui = simulate(&home);
    ui.click("Favorites").unwrap();
    assert_eq!(
        acts(support::asked(ui)),
        vec![Intent::Show(Page::Favorites)]
    );

    let mut ui = simulate(&home);
    ui.click("Trending GIFs").unwrap();
    assert_eq!(acts(support::asked(ui)), vec![Intent::Show(Page::Trending)]);

    let mut ui = simulate(&home);
    ui.click("dance").unwrap();
    assert_eq!(
        acts(support::asked(ui)),
        vec![Intent::Category("dance".into())]
    );
}

#[test]
fn typing_waits_and_enter_searches_now() {
    let home = sample::home();
    let mut ui = simulate(&home);
    ui.click("Search Klipy").unwrap();
    ui.typewrite("ca");
    assert_eq!(
        acts(support::asked(ui)).last(),
        Some(&Intent::Edit("ca".into()))
    );

    let searched = sample::search_with("cat", still);
    let mut ui = simulate(&searched);
    ui.operate(&mut focusable::focus(FIELD.into()));
    ui.tap_key(Named::Enter);
    assert_eq!(acts(support::asked(ui)), vec![Intent::Search("cat".into())]);
}

#[test]
fn favorites_filter_as_they_are_typed() {
    let favorites = sample::favorites_with(still);
    let mut ui = simulate(&favorites);
    ui.click("Search Favorites").unwrap();
    ui.typewrite("c");
    assert_eq!(
        acts(support::asked(ui)).last(),
        Some(&Intent::Search("c".into()))
    );
}

#[test]
fn a_gif_clicked_is_sent_or_inserted_with_shift() {
    let mut picker = sample::search_with("cat", still);
    assert_eq!(
        clicked(&picker, first_tile()),
        vec![Intent::Pick {
            url: "https://klipy.example/gif-0".into(),
            insert: false
        }]
    );
    picker.shift = true;
    assert_eq!(
        clicked(&picker, first_tile()),
        vec![Intent::Pick {
            url: "https://klipy.example/gif-0".into(),
            insert: true
        }]
    );
}

#[test]
fn the_star_keeps_a_gif_or_lets_it_go() {
    // The star is the tile's top right corner, a 32 px chip 4 px in.
    let corner = Point::new(GUTTER + 231.0 - 20.0, GRID + GUTTER + 20.0);
    let picker = sample::search_with("cat", still);
    // Over the tile the star shows; a press on it is the star's alone.
    assert_eq!(
        clicked(&picker, corner),
        vec![Intent::Favorite("https://klipy.example/gif-0".into())]
    );
}

#[test]
fn a_star_on_a_favourite_lets_it_go() {
    let corner = Point::new(GUTTER + 231.0 - 20.0, GRID + GUTTER + 20.0);
    let mut picker = sample::search_with("cat", still);
    picker.tiles[0].favorite = true;
    assert_eq!(
        clicked(&picker, corner),
        vec![Intent::Favorite("https://klipy.example/gif-0".into())]
    );
}

#[test]
fn control_g_closes_the_picker() {
    let home = sample::home();
    let mut ui = simulate(&home);
    ui.simulate([Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character("g".into()),
        modified_key: Key::Character("g".into()),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::KeyG),
        location: keyboard::Location::Standard,
        modifiers: Modifiers::CTRL,
        text: None,
        repeat: false,
    })]);
    assert_eq!(acts(support::asked(ui)), vec![Intent::Close]);
}

#[test]
fn shift_pressed_and_released_is_told() {
    let home = sample::home();
    let mut ui = simulate(&home);
    ui.simulate([Event::Keyboard(keyboard::Event::ModifiersChanged(
        Modifiers::SHIFT,
    ))]);
    ui.simulate([Event::Keyboard(keyboard::Event::ModifiersChanged(
        Modifiers::empty(),
    ))]);
    assert_eq!(
        acts(support::asked(ui)),
        vec![Intent::Shift(true), Intent::Shift(false)]
    );
}

#[test]
fn the_back_button_leaves_a_page_and_the_home_has_none() {
    let trending = sample::trending_with(still);
    // The back button: 34 px, 16 px in.
    assert_eq!(
        clicked(&trending, Point::new(16.0 + 17.0, 17.0)),
        vec![Intent::Back]
    );
    let home = sample::home();
    assert!(!clicked(&home, Point::new(16.0 + 17.0, 17.0)).contains(&Intent::Back));
}

#[test]
fn escape_goes_back_from_a_page_not_from_the_home() {
    let trending = sample::trending_with(still);
    let mut ui = simulate(&trending);
    ui.tap_key(Named::Escape);
    assert_eq!(acts(support::asked(ui)), vec![Intent::Back]);

    let home = sample::home();
    let mut ui = simulate(&home);
    ui.tap_key(Named::Escape);
    assert!(
        acts(support::asked(ui)).is_empty(),
        "Escape on the home is the layer's to close"
    );
}

#[test]
fn a_search_with_nothing_offers_terms_to_try() {
    let nothing = sample::nothing("zzzz");
    let mut ui = simulate(&nothing);
    ui.click("happy").unwrap();
    assert_eq!(
        acts(support::asked(ui)),
        vec![Intent::Category("happy".into())]
    );
}

#[test]
fn an_empty_favorites_says_how_to_make_one() {
    let empty = sample::favorites_empty();
    let mut ui = simulate(&empty);
    assert!(
        ui.find(
            "Click the star in the corner of a GIF to favorite it. Favorites will show up here!"
        )
        .is_ok()
    );
}

#[test]
fn the_grid_reports_how_far_it_shows() {
    let picker = sample::search_with("cat", still);
    let asked = support::asked(simulate(&picker));
    assert!(
        asked
            .iter()
            .all(|intent| matches!(intent, Intent::Shown { .. })),
        "nothing else without a click: {asked:?}"
    );
}
