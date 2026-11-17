//! The confirmation before leaving for a link: task N13's tests, driving
//! `anomi_screens::places::leaving_link::view` over its fixtures in the
//! simulator (`super::support`).

use iced::Widget as _;
use iced::keyboard::key::Named;
use iced_test::simulator::Simulator;

use anomi_screens::places::leaving_link::view;
use anomi_vm::fixtures::places::leaving_link as sample;
use anomi_vm::places::leaving_link::{Intent, LeavingLink};

use super::support::{SIZE, asked, front, settings};

fn simulate(
    link: &LeavingLink,
) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(link, front()).boxed())
}

#[test]
fn it_names_the_site_and_shows_the_whole_link() {
    let link = sample::masked();
    let mut ui = simulate(&link);
    assert!(ui.find("Leaving Yapfast").is_ok());
    assert!(ui.find(link.site.as_str()).is_ok());
}

#[test]
fn visit_site_visits_and_go_back_closes() {
    let link = sample::masked();
    let mut ui = simulate(&link);
    ui.click("Visit site").expect("visit");
    ui.click("Go back").expect("back");
    assert_eq!(asked(ui), [Intent::Visit, Intent::Close]);
}

#[test]
fn enter_visits() {
    let link = sample::masked();
    let mut ui = simulate(&link);
    ui.tap_key(Named::Enter);
    assert_eq!(asked(ui), [Intent::Visit]);
}
