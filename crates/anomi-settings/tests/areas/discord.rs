//! The Discord settings page: task S08's tests, driving
//! `anomi_settings::discord::view` over its fixtures in the simulator
//! (`super::support`).

use iced::Widget as _;
use iced_test::simulator::Simulator;

use anomi_settings::discord::view;
use anomi_vm::fixtures::settings::discord as sample;
use anomi_vm::settings::discord::{Discord, Intent};

use super::support::{SIZE, asked, front, settings};

fn simulate(page: &Discord) -> Simulator<'_, Intent, anomi_theme::Theme, anomi_theme::Renderer> {
    Simulator::with_size(settings(), SIZE, view(page, front()).boxed())
}

#[test]
fn a_switch_asks_for_the_value_it_should_have() {
    let page = sample::discord();
    let mut ui = simulate(&page);
    ui.click("Show channels you can't access")
        .expect("the switch is there");
    ui.click("Show me as idle when I'm away")
        .expect("the switch is there");
    assert_eq!(
        asked(ui),
        [Intent::ShowHiddenChannels(true), Intent::ShowIdle(false)]
    );
}

#[test]
fn the_blocked_list_shows_only_while_there_are_any() {
    let page = sample::nobody_blocked();
    assert!(simulate(&page).find("Blocked accounts").is_err());
    let page = sample::discord();
    assert!(simulate(&page).find("Blocked accounts").is_ok());
}

#[test]
fn without_an_account_the_risk_comes_first() {
    let page = sample::discord_unlinked();
    assert!(
        simulate(&page)
            .find("Discord doesn't allow apps like Yapfast on personal accounts. Linking puts your account at some risk of being suspended.")
            .is_ok()
    );
}

#[test]
fn five_blocked_show_and_see_more_asks_for_the_rest() {
    let page = sample::discord_blocked();
    let mut ui = simulate(&page);
    assert!(ui.find("Person 4").is_ok());
    assert!(ui.find("Person 5").is_err());
    ui.click("See 2 more").expect("two are hidden");
    assert_eq!(asked(ui), [Intent::MoreBlocked]);
}

#[test]
fn unblock_names_the_person() {
    let page = sample::discord_blocked();
    let mut ui = simulate(&page);
    ui.click("Unblock").expect("a button each");
    assert_eq!(asked(ui), [Intent::Unblock(page.blocked[0].id.clone())]);
}

#[test]
fn unlink_is_a_matrix_builds() {
    assert!(simulate(&sample::discord()).find("Unlink Discord").is_err());
    let page = sample::discord_blocked();
    let mut ui = simulate(&page);
    ui.click("Unlink Discord").expect("a Matrix build has it");
    assert_eq!(asked(ui), [Intent::Unlink]);
}

#[test]
fn without_an_account_the_page_offers_to_link_one() {
    let page = sample::discord_unlinked();
    let mut ui = simulate(&page);
    assert!(ui.find("Show channels you can't access").is_err());
    ui.click("Show the code").expect("the way in");
    assert_eq!(asked(ui), [Intent::Link]);
}
