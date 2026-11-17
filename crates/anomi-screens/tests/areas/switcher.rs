//! The quick switcher: task N10's tests, driving
//! `anomi_screens::switcher::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced::keyboard::key::Named;
use iced_test::Simulator;

use anomi_screens::switcher::{self, FIELD};
use anomi_vm::fixtures::switcher as samples;
use anomi_vm::switcher::{Intent, Switcher};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(620.0, 460.0);

fn simulate(vm: &Switcher) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, switcher::view(vm, front()))
}

/// Focuses the search field and types nothing.
fn focused(vm: &Switcher) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    let mut ui = simulate(vm);
    ui.click(iced_test::selector::id(FIELD)).unwrap();
    ui
}

#[test]
fn typing_asks_to_search() {
    let vm = samples::empty();
    let mut ui = focused(&vm);
    ui.typewrite("ob");
    assert_eq!(
        asked(ui),
        [Intent::Search("o".into()), Intent::Search("ob".into())]
    );
}

#[test]
fn enter_goes_to_the_chosen_place() {
    let mut vm = samples::empty();
    let mut ui = focused(&vm);
    ui.tap_key(Named::Enter);
    assert_eq!(asked(ui), [Intent::Go("discord:d9".into())]);
    vm.chosen = 2;
    let mut ui = focused(&vm);
    ui.tap_key(Named::Enter);
    assert_eq!(asked(ui), [Intent::Go("discord:c2".into())]);
}

#[test]
fn the_arrows_and_tab_choose_and_wrap() {
    let mut vm = samples::empty();
    let mut ui = focused(&vm);
    ui.tap_key(Named::ArrowDown);
    ui.tap_key(Named::Tab);
    ui.tap_key(Named::ArrowUp);
    assert_eq!(
        asked(ui),
        [Intent::Choose(1), Intent::Choose(1), Intent::Choose(5)]
    );
    vm.chosen = 5;
    let mut ui = focused(&vm);
    ui.tap_key(Named::ArrowDown);
    assert_eq!(asked(ui), [Intent::Choose(0)]);
}

#[test]
fn a_click_on_a_place_goes_there() {
    let vm = samples::query();
    let mut ui = simulate(&vm);
    ui.click("Bob").unwrap();
    assert_eq!(asked(ui), [Intent::Go("discord:d1".into())]);
}

#[test]
fn with_nothing_to_choose_the_keys_ask_nothing() {
    let vm = samples::nothing();
    let mut ui = focused(&vm);
    ui.tap_key(Named::ArrowDown);
    ui.tap_key(Named::Enter);
    assert!(asked(ui).is_empty());
}

#[test]
fn the_places_have_their_words() {
    let vm = samples::empty();
    let mut ui = simulate(&vm);
    for words in [
        "MENTIONS",
        "PREVIOUS CHANNELS",
        "Discord",
        "ideas",
        "Bob",
        "1",
    ] {
        assert!(ui.find(words).is_ok(), "{words}");
    }
    let vm = samples::nothing();
    let mut ui = simulate(&vm);
    assert!(ui.find("Nothing goes by that name.").is_ok());
}

#[test]
fn a_press_outside_the_field_keeps_its_focus() {
    let vm = samples::empty();
    let mut ui = focused(&vm);
    ui.click("Start a search with @ for people, # for text channels, ! for voice channels or * for servers.")
        .unwrap();
    ui.typewrite("b");
    ui.tap_key(Named::Enter);
    assert_eq!(
        asked(ui),
        [Intent::Search("b".into()), Intent::Go("discord:d9".into())]
    );
}

#[test]
fn a_long_name_keeps_the_badge() {
    let mut vm = samples::empty();
    vm.places[0].name =
        "Alice, Bob, Carol, Dave, Erin, Frank, Grace, Heidi, Ivan, Judy, Mallory".into();
    let mut ui = simulate(&vm);
    let badge = ui.find("1").unwrap();
    assert!(badge.visible_bounds().is_some());
}
