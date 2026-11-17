//! The About settings page, with updates: task S10's tests, driving
//! `anomi_settings::about::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::about::view;
use anomi_vm::fixtures::settings::about as sample;
use anomi_vm::settings::about::{About, Intent};

use super::support::{SIZE, asked, front, settings};

fn simulate(page: &About) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

#[test]
fn a_fork_without_releases_has_nothing_to_check() {
    let page = sample::about();
    let mut ui = simulate(&page);
    assert!(ui.find("Check now").is_err());
    assert!(ui.find("Check for updates").is_err());
    assert!(ui.find("GitHub").is_ok());
}

#[test]
fn the_build_is_copied_by_its_button() {
    let page = sample::about();
    let mut ui = simulate(&page);
    ui.click("Copy version").expect("the button is there");
    assert_eq!(asked(ui), [Intent::CopyVersion]);
}

#[test]
fn an_available_update_offers_to_install_view_and_skip() {
    let page = sample::available();
    for (text, intent) in [
        ("Update and restart", Intent::Install),
        ("View release", Intent::ViewRelease),
        ("Skip this version", Intent::SkipVersion),
        ("Check now", Intent::CheckNow),
    ] {
        let mut ui = simulate(&page);
        ui.click(text).expect("the button is there");
        assert_eq!(asked(ui), [intent]);
    }
    assert!(simulate(&page).find("Yapfast 1.5.0 is available.").is_ok());
}

#[test]
fn an_update_being_prepared_swaps_the_buttons_for_cancel() {
    let page = About {
        installing: Some(anomi_vm::settings::about::Install {
            text: "Downloading 40%".into(),
            progress: Some(0.4),
            can_cancel: true,
        }),
        ..sample::available()
    };
    let mut ui = simulate(&page);
    assert!(ui.find("Check now").is_err());
    ui.click("Cancel").expect("cancel is there");
    assert_eq!(asked(ui), [Intent::CancelInstall]);
}

#[test]
fn the_check_switches_ask_for_their_opposite() {
    let page = sample::releases();
    let mut ui = simulate(&page);
    ui.click("Check for updates").expect("the switch is there");
    ui.click("Include nightly builds")
        .expect("the switch is there");
    assert_eq!(asked(ui), [Intent::Check(false), Intent::Nightlies(true)]);
}

#[test]
fn a_skipped_release_can_still_be_viewed() {
    let page = About {
        status: anomi_vm::settings::about::Status::Skipped,
        ..sample::available()
    };
    let mut ui = simulate(&page);
    assert!(ui.find("Skip this version").is_err());
    assert!(ui.find("Update and restart").is_err());
    ui.click("View release").expect("the release is there");
    assert_eq!(asked(ui), [Intent::ViewRelease]);
}

#[test]
fn the_links_open_the_fork_and_the_original() {
    let page = sample::about();
    let mut ui = simulate(&page);
    ui.click("GitHub").expect("the link is there");
    ui.click("Original: Yapfast").expect("the link is there");
    assert_eq!(asked(ui), [Intent::OpenGithub, Intent::OpenWebsite]);
}
