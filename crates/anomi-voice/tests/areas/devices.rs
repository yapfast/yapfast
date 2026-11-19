//! The device menus: task V09's tests, driving `anomi_voice::devices::view`
//! over its fixtures in the simulator (`super::support`).

use iced::keyboard::key::Named;
use iced::widget::container;
use iced::{Event, Length, Point, Size, mouse};
use iced_test::Simulator;

use anomi_theme::prelude::*;
use anomi_theme::{Renderer, Theme};
use anomi_vm::fixtures::voice::devices as samples;
use anomi_vm::menu::Key;
use anomi_vm::settings::voice::Profile;
use anomi_vm::voice::devices::{Devices, Intent, Part};
use anomi_voice::devices;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(900.0, 560.0);

fn press() -> Event {
    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
}

fn release() -> Event {
    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
}

fn simulate(vm: &Devices) -> Simulator<'_, Intent, Theme, Renderer> {
    Simulator::with_size(settings(), SIZE, devices::view(vm, front()))
}

/// The menu at the window's right edge, where its submenu has no room.
fn at_the_right(vm: &Devices) -> Simulator<'_, Intent, Theme, Renderer> {
    let view: Element<'_, Intent> = container(devices::view(vm, front()))
        .width(Length::Fill)
        .align_right(Length::Fill)
        .boxed();
    Simulator::with_size(settings(), SIZE, view)
}

#[test]
fn the_input_menu_has_discords_rows_and_nothing_more() {
    let vm = samples::microphone();
    let mut ui = simulate(&vm);
    for text in [
        "Input Device",
        "Input Profile",
        "Input Volume",
        "Voice Settings",
    ] {
        assert!(ui.find(text).is_ok(), "{text}");
    }
    for gone in ["100%", "INPUT LEVEL", "NOISE SUPPRESSION", "DeepFilterNet"] {
        assert!(ui.find(gone).is_err(), "{gone}");
    }
    // The submenus wait for their rows.
    assert!(ui.find("Webcam Microphone").is_err());
    assert!(ui.find("Studio").is_err());
}

#[test]
fn the_output_menu_has_no_profile() {
    let vm = samples::speakers();
    let mut ui = simulate(&vm);
    assert!(ui.find("Output Device").is_ok());
    assert!(ui.find("Output Volume").is_ok());
    assert!(ui.find("Input Profile").is_err());
}

#[test]
fn a_click_on_a_head_asks_for_its_submenu() {
    let vm = samples::microphone();
    let mut ui = simulate(&vm);
    ui.click("Input Profile").unwrap();
    let asked = asked(ui);
    assert!(!asked.is_empty());
    assert!(
        asked
            .iter()
            .all(|intent| *intent == Intent::Hover(Some(Part::Profile))),
        "{asked:?}"
    );
}

#[test]
fn an_open_submenu_names_every_device_beside_the_card_and_a_click_chooses_one() {
    let vm = samples::listing();
    let mut ui = simulate(&vm);
    let head = ui.find("Output Device").unwrap().bounds();
    let first = "System: BlackShark V3 X Monitor";
    let row = ui.find(first).unwrap().bounds();
    assert!(row.x > head.x + head.width, "on the right: {row:?}");
    // Level with its row, or as near as the window's 8 px margin lets it.
    assert!((0.0..=8.0).contains(&(row.y - head.y)), "{row:?} {head:?}");
    ui.click("Ryzen HD Audio Controller Analog Stereo").unwrap();
    assert_eq!(asked(ui), [Intent::Select(2)]);
}

#[test]
fn without_room_on_the_right_the_submenu_opens_on_the_left() {
    let vm = samples::listing();
    let mut ui = at_the_right(&vm);
    let head = ui.find("Output Device").unwrap().bounds();
    let row = ui.find("BlackShark V3 X Analog Stereo").unwrap().bounds();
    assert!(row.x + row.width < head.x, "on the left: {row:?}");
}

#[test]
fn a_profile_asks_for_itself() {
    let vm = samples::profiles();
    let mut ui = simulate(&vm);
    ui.click("Studio").unwrap();
    assert_eq!(asked(ui), [Intent::Profile(Profile::Studio)]);
}

#[test]
fn nothing_found_says_so_and_chooses_nothing() {
    let vm = samples::nothing_found();
    let mut ui = simulate(&vm);
    let row = ui.find("None found").unwrap().bounds();
    ui.point_at(row.center());
    ui.simulate([press(), release()]);
    assert!(asked(ui).is_empty());
}

#[test]
fn voice_settings_asks_for_them() {
    let vm = samples::speakers();
    let mut ui = simulate(&vm);
    ui.click("Voice Settings").unwrap();
    assert_eq!(asked(ui).last(), Some(&Intent::Settings));
}

#[test]
fn the_keys_go_to_the_menu_and_escape_only_while_a_submenu_shows() {
    let vm = samples::speakers();
    let mut ui = simulate(&vm);
    for key in [
        Named::ArrowDown,
        Named::ArrowRight,
        Named::Enter,
        Named::Escape,
    ] {
        ui.tap_key(key);
    }
    assert_eq!(
        asked(ui),
        [
            Intent::Key(Key::Down),
            Intent::Key(Key::Right),
            Intent::Key(Key::Enter)
        ],
        "Escape is the layer's"
    );
    let vm = samples::listing();
    let mut ui = simulate(&vm);
    ui.tap_key(Named::Escape);
    assert_eq!(asked(ui), [Intent::Key(Key::Escape)]);
}

#[test]
fn dragging_the_volume_asks_for_it_and_keeps_the_last() {
    let vm = samples::speakers();
    let mut ui = simulate(&vm);
    let track = ui.find("Output Volume").unwrap().bounds();
    // The header line, 6 px, then the 18 px track.
    ui.point_at(Point::new(track.x + 150.0, track.y + track.height + 15.0));
    ui.simulate([press(), release()]);
    let asked = asked(ui);
    assert!(
        matches!(asked.first(), Some(Intent::Volume(_))),
        "{asked:?}"
    );
    assert!(matches!(asked.last(), Some(Intent::Commit(_))), "{asked:?}");
}
