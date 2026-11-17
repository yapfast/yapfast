//! The Keybinds settings page: task S09's tests, driving
//! `anomi_settings::keybinds::view` over its fixtures in the simulator
//! (`super::support`).

use iced::widget::container;
use iced_test::Simulator;

use anomi_settings::keybinds::view;
use anomi_theme::Motion;
use anomi_vm::fixtures::settings::keybinds::{empty, some};
use anomi_vm::settings::keybinds::{Intent, Keybinds};

use super::support::{SIZE, asked, front, settings};

fn ui(
    page: &Keybinds,
    motion: Motion,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, container(view(page, motion)))
}

#[test]
fn the_add_button_asks_for_a_row() {
    let page = empty();
    let mut ui = ui(&page, front());
    ui.click("Add a Keybind").expect("the button");
    assert_eq!(asked(ui), [Intent::Add]);
}

#[test]
fn a_row_records_edits_and_removes() {
    let page = some();
    let mut ui = ui(&page, front());
    ui.click("Edit keybind").expect("a row with keys");
    ui.click("Record keybind").expect("the row without");
    // The first row's remove button: an icon its tooltip names, which is no text to find.
    ui.point_at(iced::Point::new(1368.0, 114.0));
    ui.simulate(iced_test::simulator::click());
    let asked = asked(ui);
    assert!(asked.contains(&Intent::Record(0, true)), "{asked:?}");
    assert!(asked.contains(&Intent::Record(2, true)), "{asked:?}");
    assert!(asked.contains(&Intent::Remove(0)), "{asked:?}");
}

#[test]
fn a_recording_row_shows_the_keys_so_far_and_stops() {
    let mut page = some();
    page.recording = Some(1);
    let mut ui = ui(&page, front());
    ui.find("Press a key...").expect("nothing pressed yet");
    ui.click("Stop recording").expect("the button");
    assert_eq!(asked(ui), [Intent::Record(1, false)]);
    page.recorded = "Ctrl + ".into();
    let mut ui = self::ui(&page, front());
    ui.find("Ctrl +").expect("the keys so far");
}

#[test]
fn a_row_asks_for_the_action_picked_from_its_list() {
    let page = some();
    let mut ui = ui(&page, front());
    ui.click("Toggle Mute").expect("the second row's picker");
    ui.click("Toggle Deafen")
        .expect("an action in the open list");
    assert_eq!(asked(ui), [Intent::Action(1, 4)]);
}
