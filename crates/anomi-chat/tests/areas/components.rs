//! A bot message's components: task C06's tests, driving
//! `anomi_chat::components::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Size;
use iced_test::Simulator;

use anomi_chat::components;
use anomi_theme::{Renderer, Theme};
use anomi_vm::chat::components::{Components, Intent};
use anomi_vm::chat::timeline::group::key;
use anomi_vm::fixtures::chat::components as fixtures;

use super::support::{asked, front, settings};

const SIZE: Size = Size::new(760.0, 1100.0);

fn simulate(components: &Components) -> Simulator<'_, Intent, Theme, Renderer> {
    use iced::Widget as _;
    Simulator::with_size(
        settings(),
        SIZE,
        components::view(components, front()).boxed(),
    )
}

fn click(components: &Components, text: &str) -> Vec<Intent> {
    let mut ui = simulate(components);
    ui.click(text).unwrap_or_else(|_| panic!("no {text:?}"));
    asked(ui)
}

#[test]
fn a_button_tells_its_app() {
    let components = fixtures::components();
    assert_eq!(
        click(&components, "Approve"),
        vec![Intent::Use(key("deploy"), "ok".into())]
    );
}

#[test]
fn a_link_opens_its_address() {
    let components = fixtures::components();
    assert_eq!(
        click(&components, "Logs"),
        vec![Intent::Open(
            key("deploy"),
            "https://example.com/logs".into()
        )]
    );
}

#[test]
fn a_sections_button_opens_its_link() {
    let notice = fixtures::notice();
    assert_eq!(
        click(&notice, "Secure account"),
        vec![Intent::Open(
            key("notice"),
            "https://discord.com/settings/account".into()
        )]
    );
}

#[test]
fn an_offer_opens_the_apps_store_page() {
    let components = fixtures::components();
    assert_eq!(
        click(&components, "Open in Discord"),
        vec![Intent::Open(
            key("deploy"),
            "https://discord.com/application-directory/7/store/42".into()
        )]
    );
}

#[test]
fn an_offer_without_its_app_does_nothing() {
    let mut components = fixtures::components();
    components.app_id.clear();
    assert!(click(&components, "Open in Discord").is_empty());
}

#[test]
fn a_button_turned_off_does_nothing() {
    let components = fixtures::components();
    assert!(click(&components, "Retry").is_empty());
}

#[test]
fn a_waiting_control_holds_back_the_others() {
    let mut components = fixtures::components();
    components.busy = true;
    assert!(click(&components, "Approve").is_empty());
    assert!(click(&components, "Pick an environment").is_empty());
    // A link isn't an interaction: it opens all the same.
    assert_eq!(click(&components, "Logs").len(), 1);
}

#[test]
fn a_menu_opens_its_dropdown() {
    let components = fixtures::components();
    let asked = click(&components, "Pick an environment");
    assert!(matches!(
        asked.as_slice(),
        [Intent::OpenSelect(_, id, _)] if id == "environment"
    ));
}

#[test]
fn its_files_and_what_it_cannot_show_are_told() {
    let components = fixtures::components();
    let mut ui = simulate(&components);
    assert!(ui.find("deploy.log").is_ok());
    assert!(
        ui.find("Yapfast can't show this part of the message yet.")
            .is_ok()
    );
}
