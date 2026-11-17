//! Making a channel or a category: task X15's tests, driving
//! `anomi_settings::channel::create::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced::advanced::widget::operation::focusable;
use iced::advanced::widget::{Id, Operation};
use iced::keyboard::{self, key::Named};
use iced_test::simulator::Simulator;

use anomi_settings::channel::create::view;
use anomi_settings::channel::create::{CANCEL, CONFIRM};
use anomi_vm::fixtures::settings::create_channel as sample;
use anomi_vm::settings::create_channel::{ChannelType, CreateChannel, Intent};

use super::support::{SIZE, asked, front, settings};

fn simulate(
    dialog: &CreateChannel,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(dialog, front()).boxed())
}

/// Focuses the button with that id and taps Enter.
fn press(
    ui: &mut Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer>,
    id: &'static str,
) {
    let mut focus = focusable::focus::<()>(Id::new(id));
    ui.operate(&mut focus as &mut dyn Operation);
    ui.tap_key(keyboard::Key::Named(Named::Enter));
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
    press(&mut ui, CANCEL);
    assert_eq!(asked(ui), [Intent::Close]);
}

#[test]
fn the_x_closes() {
    let dialog = sample::delete();
    let mut ui = simulate(&dialog);
    ui.click("Close").expect("the X");
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
    press(&mut ui, CONFIRM);
    assert_eq!(asked(ui), []);
}

#[test]
fn a_refusal_shows() {
    let dialog = sample::refused();
    let mut ui = simulate(&dialog);
    assert!(ui.find(dialog.error.as_str()).is_ok());
}
