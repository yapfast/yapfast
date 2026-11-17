//! Making a channel or a category: task X15's tests, driving
//! `anomi_settings::channel::create::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced::keyboard::key::Named;
use iced_test::simulator::Simulator;

use anomi_settings::channel::create::view;
use anomi_vm::fixtures::settings::create_channel as sample;
use anomi_vm::settings::create_channel::{ChannelType, CreateChannel, Intent};

use super::support::{SIZE, asked, front, settings};

fn simulate(
    dialog: &CreateChannel,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(dialog, front()).boxed())
}

/// Where the Voice and the Text segment lie in the card at the corner of the
/// simulated window.
fn segments() -> (iced::Point, iced::Point) {
    (
        iced::Point::new(340.0, 133.0),
        iced::Point::new(120.0, 133.0),
    )
}

#[test]
fn a_channel_offers_its_type_and_a_press_chooses_it() {
    let dialog = sample::channel();
    let mut ui = simulate(&dialog);
    // The segments are one widget, so by place: the card sits at the corner.
    let (voice, text) = segments();
    for at in [voice, text] {
        ui.point_at(at);
        ui.simulate(iced_test::simulator::click());
    }
    assert_eq!(
        asked(ui),
        [
            Intent::Kind(ChannelType::Voice),
            Intent::Kind(ChannelType::Text)
        ]
    );
}

#[test]
fn a_category_has_no_type() {
    let dialog = sample::category();
    let mut ui = simulate(&dialog);
    assert!(ui.find("Voice").is_err());
    assert!(ui.find("CATEGORY NAME").is_ok());
    assert!(ui.find("CHANNEL TYPE").is_err());
}

#[test]
fn typing_and_enter_in_the_name_ask_for_it() {
    let dialog = sample::channel();
    let mut ui = simulate(&dialog);
    ui.click("new-channel").expect("the field");
    ui.typewrite("x");
    ui.tap_key(Named::Enter);
    let intents = asked(ui);
    assert!(intents.contains(&Intent::Name("x".into())));
    assert!(intents.contains(&Intent::Confirm));
}

#[test]
fn cancel_closes() {
    let dialog = sample::delete();
    let mut ui = simulate(&dialog);
    // Cancel is a `Face`, which the selector cannot find by its text: by place.
    ui.point_at(iced::Point::new(246.0, 112.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn a_delete_has_no_field_and_enter_confirms_it() {
    let dialog = sample::delete();
    let mut ui = simulate(&dialog);
    assert!(ui.find("CHANNEL NAME").is_err());
    ui.tap_key(Named::Enter);
    assert_eq!(asked(ui), [Intent::Confirm]);
}

#[test]
fn nothing_is_asked_twice_while_it_is_sent() {
    let dialog = sample::saving();
    let mut ui = simulate(&dialog);
    ui.tap_key(Named::Enter);
    ui.click("game-night").expect("the field");
    ui.tap_key(Named::Enter);
    // The confirm button, disabled, at the card's foot.
    ui.point_at(iced::Point::new(420.0, 262.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), []);
}

#[test]
fn a_refusal_shows() {
    let dialog = sample::refused();
    let mut ui = simulate(&dialog);
    assert!(ui.find(dialog.error.as_str()).is_ok());
}

#[test]
fn the_confirm_button_is_where_the_test_presses_it() {
    let dialog = sample::voice();
    let mut ui = simulate(&dialog);
    ui.point_at(iced::Point::new(420.0, 262.0));
    ui.simulate(iced_test::simulator::click());
    assert_eq!(asked(ui), [Intent::Confirm]);
}
