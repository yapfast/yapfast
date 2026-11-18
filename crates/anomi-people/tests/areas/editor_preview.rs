//! The profile editor's preview and colours: task P15's tests, driving
//! `anomi_people::editor::preview::view` and `colour::view` over their
//! fixtures in the simulator (`super::support`).

use iced::{Point, Size};
use iced_test::Simulator;

use anomi_people::editor::{colour, preview};
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::people::editor_preview as samples;
use anomi_vm::people::editor_preview::{Change, Colour, EditorPreview, Intent, Page};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(440.0, 700.0);

type Ui<'a> = Simulator<'a, Intent, Theme, Renderer>;

fn at(preview: &EditorPreview) -> Ui<'_> {
    Simulator::with_size(settings(), SIZE, preview::view(preview, front()))
}

fn click_text(mut ui: Ui<'_>, words: &str) -> Vec<Intent> {
    ui.click(words).expect(words);
    asked(ui)
}

#[test]
fn a_tab_is_chosen_by_its_words() {
    let sample = samples::on(Page::Board);
    assert_eq!(
        click_text(at(&sample), "Activity"),
        [Intent::Choose(Page::Activity)]
    );
    assert_eq!(
        click_text(at(&sample), "Wishlist"),
        [Intent::Choose(Page::Wishlist)]
    );
    assert_eq!(
        click_text(at(&sample), "Board"),
        [Intent::Choose(Page::Board)]
    );
}

#[test]
fn each_empty_tab_says_why() {
    let mut sample = samples::empty();
    {
        let mut ui = at(&sample);
        ui.find("Nothing on your board yet.")
            .expect("the board's heading");
        ui.find("Adding more from Yapfast is coming soon.")
            .expect("and its line");
    }
    sample.page = Page::Activity;
    at(&sample)
        .find("You don't have any activity here")
        .expect("activity's heading");
    sample.page = Page::Wishlist;
    at(&sample)
        .find("Nothing on your wishlist yet.")
        .expect("the wishlist's");
}

#[test]
fn a_note_stands_in_for_the_heading() {
    let mut sample = samples::empty();
    sample.board_note = "Their board is private.".into();
    let mut ui = at(&sample);
    ui.find("Their board is private.").expect("the note");
    assert!(ui.find("Nothing on your board yet.").is_err());
}

#[test]
fn the_tabs_list_what_the_profile_wears() {
    let board = samples::on(Page::Board);
    let mut ui = at(&board);
    ui.find("Your Widgets").expect("the board's title");
    ui.find("Factorio").expect("a game");
    let wishes = samples::on(Page::Wishlist);
    let mut ui = at(&wishes);
    ui.find("4 Items").expect("the wishlist's count");
    ui.find("Balatro").expect("an item");
    let themed = samples::themed(Page::Activity);
    at(&themed)
        .find("Weightless")
        .expect("a song, under a theme");
}

#[test]
fn an_activitys_title_opens_its_link() {
    let sample = samples::on(Page::Activity);
    assert_eq!(
        click_text(at(&sample), "Weightless"),
        [Intent::Link(
            "https://open.spotify.com/track/weightless".into()
        )]
    );
}

fn popover(colour: &Colour) -> Ui<'_> {
    Simulator::with_size(
        settings(),
        Size::new(240.0, 250.0),
        colour::view(colour, front()),
    )
}

#[test]
fn a_press_on_the_pad_places_the_colour() {
    let start = samples::colour();
    let mut ui = popover(&start);
    // The pad is 220 by 150 at (10, 10): its middle is half and half.
    ui.point_at(Point::new(120.0, 85.0));
    ui.simulate(iced_test::simulator::click());
    let said = asked(ui);
    let [Intent::Colour(Change::Pad { sat, val })] = said.as_slice() else {
        panic!("{said:?}");
    };
    assert!(
        (sat - 0.5).abs() < 0.02 && (val - 0.5).abs() < 0.02,
        "{sat} {val}"
    );
}

#[test]
fn a_press_on_the_bar_places_the_hue() {
    let start = samples::colour();
    let mut ui = popover(&start);
    // The bar is 220 wide at x 10, 175 down: a quarter along is 90 degrees.
    ui.point_at(Point::new(65.0, 175.0));
    ui.simulate(iced_test::simulator::click());
    let said = asked(ui);
    let [Intent::Colour(Change::Hue(hue))] = said.as_slice() else {
        panic!("{said:?}");
    };
    assert!((hue - 90.0).abs() < 3.0, "{hue}");
}

#[test]
fn a_wrong_code_stays_in_the_field() {
    let mut colour = samples::colour();
    assert_eq!(colour.apply(Change::Text("b8a58".into())), None);
    popover(&colour).find("b8a58").expect("the draft as typed");
}

#[test]
fn typing_in_the_code_asks_for_it_as_typed() {
    let start = samples::colour();
    let mut ui = popover(&start);
    // The field is 42 high, 194 down.
    ui.point_at(Point::new(120.0, 215.0));
    ui.simulate(iced_test::simulator::click());
    let _ = ui.typewrite("f");
    let said = asked(ui);
    let [Intent::Colour(Change::Text(code))] = said.as_slice() else {
        panic!("{said:?}");
    };
    assert_eq!(code.len(), 7, "{code}: one letter more than b8a587");
}
