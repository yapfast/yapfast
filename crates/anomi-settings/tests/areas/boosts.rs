//! A server's Boost Perks page: task X02's tests, driving
//! `anomi_settings::server::boosts::view` over its fixtures in the simulator
//! (`super::support`).

use anomi_theme::prelude::*;
use iced_test::Simulator;

use anomi_settings::server::boosts::view;
use anomi_vm::fixtures::settings::boosts::{boosts, unlocked};
use anomi_vm::settings::boosts::{Background, Boosts, Intent};

use super::support::{SIZE, asked, front, settings};

fn simulate(page: &Boosts) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

fn click(page: &Boosts, text: &str) -> Vec<Intent> {
    let mut ui = simulate(page);
    ui.click(text).unwrap_or_else(|_| panic!("{text}"));
    asked(ui)
}

#[test]
fn a_locked_background_says_its_level_and_cannot_be_uploaded() {
    let page = boosts();
    let mut ui = simulate(&page);
    assert!(ui.find("Unlocks at Level 2").is_ok());
    assert!(ui.find("Unlocks at Level 1").is_ok());
    assert!(ui.find("Remove Background").is_err());
    assert!(click(&page, "Upload Background").is_empty());
}

#[test]
fn an_unlocked_background_is_uploaded_and_removed() {
    let page = unlocked();
    assert_eq!(
        click(&page, "Upload Background"),
        vec![Intent::Pick(Background::Banner)]
    );
    assert_eq!(
        click(&page, "Remove Background"),
        vec![Intent::Remove(Background::Banner)]
    );
}

#[test]
fn the_progress_bar_switch_reports_the_flip() {
    assert_eq!(
        click(&unlocked(), "Show Boost progress bar"),
        vec![Intent::ToggleProgressBar]
    );
}

#[test]
fn the_page_waits_while_saving_or_reading() {
    let saving = Boosts {
        saving: true,
        ..unlocked()
    };
    assert!(click(&saving, "Remove Background").is_empty());
    assert!(click(&saving, "Show Boost progress bar").is_empty());
    let reading = Boosts {
        reading: true,
        ..unlocked()
    };
    assert!(click(&reading, "Upload Background").is_empty());
}

#[test]
fn an_empty_unlocked_well_says_so() {
    let page = Boosts {
        invite: Default::default(),
        banner: Default::default(),
        ..Boosts::default()
    };
    assert!(simulate(&page).find("No image").is_ok());
}
