//! The name style window: task P16's tests, driving
//! `anomi_people::editor::name_style::view` over its fixtures in the
//! simulator (`super::support`).

use iced::Size;
use iced::keyboard::{self, key::Named};
use iced_test::Simulator;

use anomi_people::editor::name_style::view;
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::people::name_style as samples;
use anomi_vm::people::editor_preview::Change;
use anomi_vm::people::name_style::{Intent, NameStyle};

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(1400.0, 860.0);

fn window(sample: &NameStyle) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, view(sample, front()))
}

#[test]
fn it_shows_its_choices_and_the_sample_names() {
    let sample = samples::gradient();
    let mut ui = window(&sample);
    for text in [
        "Change Display Name Style",
        "Font",
        "Effect",
        "Colour",
        "Solid",
        "Gradient",
        "Neon",
        "Toon",
        "Pop",
        "Gummy",
        "Prism",
        "Surprise Me",
        "Apply",
        "does anyone read this?",
    ] {
        ui.find(text).unwrap_or_else(|_| panic!("{text} shows"));
    }
}

#[test]
fn escape_closes_it() {
    let sample = samples::solid();
    let mut ui = window(&sample);
    ui.tap_key(keyboard::Key::Named(Named::Escape));
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn a_closed_window_draws_nothing() {
    let sample = NameStyle::default();
    let mut ui = window(&sample);
    assert!(ui.find("Apply").is_err());
}

#[test]
fn an_effect_and_surprise_me_ask_for_themselves() {
    let sample = samples::solid();
    let mut ui = window(&sample);
    ui.click("Neon").expect("the tile");
    ui.click("Surprise Me").expect("its button");
    assert_eq!(asked(ui), [Intent::Effect(3), Intent::Surprise]);
}

#[test]
fn apply_waits_for_a_change_then_carries_the_style() {
    let mut sample = samples::solid();
    let mut ui = window(&sample);
    let _ = ui.click("Apply");
    assert!(!asked(ui).iter().any(|i| matches!(i, Intent::Apply { .. })));
    sample.update(&Intent::Effect(4));
    let mut ui = window(&sample);
    ui.click("Apply").expect("enabled now");
    assert_eq!(
        asked(ui),
        [Intent::Apply {
            font: 11,
            effect: 4,
            colours: "f52399".into()
        }]
    );
}

#[test]
fn a_click_away_from_the_picker_closes_it() {
    let sample = samples::picking();
    let mut ui = window(&sample);
    ui.click("Font").expect("the heading");
    // The picker stands over the colour's heading, as in Slint: a press
    // on it is its own.
    ui.click("Colour").expect("the heading");
    assert_eq!(asked(ui), [Intent::Shut, Intent::Ignored]);
    let mut sample = sample;
    sample.update(&Intent::Shut);
    let mut ui = window(&sample);
    ui.click("Font").expect("the heading");
    assert!(asked(ui).is_empty(), "nothing to close");
}

#[test]
fn the_picker_edits_the_open_handle() {
    let sample = samples::picking();
    let mut ui = window(&sample);
    ui.click("ca63fa").expect("the code");
    ui.typewrite("0");
    let typed = asked(ui);
    assert!(
        typed
            .iter()
            .any(|i| matches!(i, Intent::Pick(Change::Text(text)) if text.contains('0'))),
        "{typed:?}"
    );
}
