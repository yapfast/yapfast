//! The Game activity settings page: task S08's tests, driving
//! `anomi_settings::games::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::games::view;
use anomi_vm::fixtures::settings::games as sample;
use anomi_vm::settings::games::{Games, Intent};

use super::support::{SIZE, asked, front, settings};

fn simulate(page: &Games) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

#[test]
fn the_switch_asks_to_stop_sharing() {
    let page = sample::games();
    let mut ui = simulate(&page);
    ui.click("Share the game you play")
        .expect("the switch is there");
    assert_eq!(asked(ui), [Intent::Share(false)]);
}

#[test]
fn the_switch_is_locked_without_an_account() {
    let page = sample::unlinked();
    let mut ui = simulate(&page);
    let _ = ui.click("Share the game you play");
    assert!(asked(ui).is_empty());
}

#[test]
fn each_game_can_be_edited_and_removed() {
    let page = sample::custom();
    let mut ui = simulate(&page);
    assert!(ui.find("Blockhead").is_ok());
    ui.click("Remove").expect("a button each");
    assert_eq!(asked(ui), [Intent::Remove(0)]);
}

#[test]
fn adding_waits_for_both_fields() {
    let page = sample::games();
    let mut ui = simulate(&page);
    let _ = ui.click("Add game");
    assert!(asked(ui).is_empty());
}

#[test]
fn save_asks_to_keep_the_edit() {
    let page = sample::custom();
    let mut ui = simulate(&page);
    ui.click("Save game").expect("a game is being edited");
    assert_eq!(asked(ui), [Intent::Save]);
}

#[test]
fn cancel_shows_only_while_editing() {
    assert!(simulate(&sample::games()).find("Cancel").is_err());
    let page = sample::custom();
    let mut ui = simulate(&page);
    ui.click("Cancel").expect("an edit can be left");
    assert_eq!(asked(ui), [Intent::Cancel]);
}

#[test]
fn typing_reports_the_whole_text() {
    let page = sample::games();
    let mut ui = simulate(&page);
    ui.click("Executable name or full path")
        .expect("the executable's field");
    ui.typewrite("g");
    ui.click("Text after Playing (up to 128 characters)")
        .expect("the text's field");
    ui.typewrite("x");
    assert_eq!(
        asked(ui),
        [Intent::Executable("g".into()), Intent::Text("x".into())]
    );
}

#[test]
fn edit_names_the_game() {
    let page = sample::custom();
    let mut ui = simulate(&page);
    ui.click("Edit").expect("a button each");
    assert_eq!(asked(ui), [Intent::Edit(0)]);
}

#[test]
fn a_refused_save_says_why() {
    let page = sample::refused();
    assert!(
        simulate(&page)
            .find("Enter an executable and a name of up to 128 characters, without line breaks.")
            .is_ok()
    );
}
