//! A server's Bans: task X06's tests, driving
//! `anomi_settings::server::bans::{view, dialog}` over its fixtures in the
//! simulator (`super::support`).

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::server::bans::{dialog, view};
use anomi_vm::fixtures::settings::bans as sample;
use anomi_vm::settings::bans::{Bans, Intent};

use super::support::{SIZE, asked, front, settings};

fn simulate(page: &Bans) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

#[test]
fn a_row_opens_its_ban_and_search_asks_to_search() {
    let page = sample::bans();
    let mut ui = simulate(&page);
    ui.click("Mallory").expect("the row is there");
    ui.click("Search").expect("the button is there");
    let asked = asked(ui);
    assert!(
        asked.contains(&Intent::Open("1400000000000000901".into())),
        "{asked:?}"
    );
    assert!(asked.contains(&Intent::Search), "{asked:?}");
}

#[test]
fn no_bans_says_so_and_loading_says_that() {
    let mut page = Bans::default();
    assert!(simulate(&page).find("No Bans").is_ok());
    page.fetching = true;
    assert!(simulate(&page).find("Loading...").is_ok());
    assert!(simulate(&page).find("No Bans").is_err());
}

#[test]
fn pages_go_back_and_forth_within_their_count() {
    let mut page = sample::bans();
    page.pages = 3;
    page.page = 1;
    let mut ui = simulate(&page);
    assert!(ui.find("Page 2 of 3").is_ok());
    ui.click("Previous").expect("the button is there");
    ui.click("Next").expect("the button is there");
    assert_eq!(asked(ui), [Intent::Page(0), Intent::Page(2)]);
}

#[test]
fn a_single_page_has_no_pager() {
    let page = sample::bans();
    assert!(simulate(&page).find("Next").is_err());
}

#[test]
fn the_dialog_revokes_or_closes_for_someone_who_may() {
    let open = sample::open();
    let mut ui = Simulator::with_size(settings(), SIZE, dialog(&open, front()).boxed());
    assert!(ui.find("Spam in #general").is_ok());
    ui.click("Revoke Ban").expect("the button is there");
    ui.click("Done").expect("the button is there");
    assert_eq!(asked(ui), [Intent::Unban, Intent::Close]);
}

#[test]
fn the_dialog_of_a_reader_has_no_buttons_and_shows_an_error() {
    let mut open = sample::open();
    open.may_unban = false;
    open.error = "Missing Permissions".into();
    let mut ui = Simulator::with_size(settings(), SIZE, dialog(&open, front()).boxed());
    assert!(ui.find("Revoke Ban").is_err());
    assert!(ui.find("Missing Permissions").is_ok());
}

#[test]
fn a_ban_on_its_way_out_asks_nothing_more() {
    let mut open = sample::open();
    open.unbanning = true;
    let mut ui = Simulator::with_size(settings(), SIZE, dialog(&open, front()).boxed());
    assert!(ui.find("Revoke Ban").is_err());
    ui.click("Revoking...").expect("the button is there");
    assert!(asked(ui).is_empty());
}
